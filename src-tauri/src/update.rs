use crate::rclone;
use crate::store;
use serde::Serialize;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const UA: &str = "rclone-mount-manager";
const RCLONE_VERSION_URL: &str = "https://downloads.rclone.org/version.txt";
const WINFSP_LATEST_API: &str = "https://api.github.com/repos/winfsp/winfsp/releases/latest";
const APP_LATEST_API: &str =
    "https://api.github.com/repos/Jie-baobao/rclone-mount-manager/releases/latest";

#[derive(Serialize, Clone, Debug)]
pub struct VersionInfo {
    /// rclone / WinFsp / 管理器
    pub name: String,
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    /// 官网或发布页
    pub url: String,
    /// 出错时的说明（例如接口限流）
    pub note: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct UpdateReport {
    pub items: Vec<VersionInfo>,
}

fn client() -> Result<reqwest::blocking::Client, String> {
    let mut builder = reqwest::blocking::Client::builder()
        .user_agent(UA)
        .timeout(std::time::Duration::from_secs(30));
    // 更新检测固定跟随系统代理（Windows「Internet 选项」/ 类 Unix 环境变量）：
    // 更新源在境外，部分网络不开代理连不上
    if let Some(p) = crate::rclone::system_proxy_url() {
        if let Ok(proxy) = reqwest::Proxy::all(&p) {
            builder = builder.proxy(proxy);
        }
    }
    builder
        .build()
        .map_err(|e| format!("创建网络客户端失败：{e}"))
}

fn get_text(url: &str) -> Result<String, String> {
    let resp = client()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("请求失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(friendly_http_error(resp.status().as_u16()));
    }
    resp.text().map_err(|e| format!("读取响应失败：{e}"))
}

/// 把常见 HTTP 错误翻译成人话，别再甩一个 403 让用户摸不着头脑
fn friendly_http_error(code: u16) -> String {
    match code {
        403 => "GitHub 接口额度已用完（共享出口 IP 的匿名额度是每小时 60 次），\
            请稍后再试，或直接到发布页查看新版本"
            .into(),
        429 => "请求太频繁被限流，请稍后再试".into(),
        402..=499 => format!("请求被服务器拒绝（HTTP {code}），若在境外网络受限环境，请检查系统代理"),
        500..=599 => format!("GitHub 服务端异常（HTTP {code}），请稍后再试"),
        _ => format!("HTTP {code}"),
    }
}

/// 走 `github.com/<owner>/<repo>/releases/latest` 的重定向拿最新 tag，
/// 不消耗 GitHub API 的匿名配额（api.github.com 每出口 IP 每小时只有 60 次）。
/// reqwest 默认跟随重定向，最终 URL 一定以 /tag/<版本号> 结尾。
fn latest_via_redirect(page_url: &str) -> Result<String, String> {
    let resp = client()?
        .get(page_url)
        .send()
        .map_err(|e| format!("请求失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(friendly_http_error(resp.status().as_u16()));
    }
    let final_url = resp.url().to_string();
    let tag = final_url.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if tag.is_empty() {
        return Err("未取到版本号".into());
    }
    Ok(normalize(tag))
}

/// 去掉前缀 v，只留数字版本
fn normalize(tag: &str) -> String {
    tag.trim().trim_start_matches('v').to_string()
}

fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .take(3)
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };
    let (l, c) = (parse(latest), parse(current));
    if l.is_empty() || c.is_empty() {
        return false;
    }
    for i in 0..l.len().max(c.len()) {
        let a = *l.get(i).unwrap_or(&0);
        let b = *c.get(i).unwrap_or(&0);
        if a != b {
            return a > b;
        }
    }
    false
}

fn rclone_latest() -> Result<String, String> {
    let text = get_text(RCLONE_VERSION_URL)?;
    // 文件内容形如 "rclone v1.75.2"
    let ver = text
        .split_whitespace()
        .find(|w| w.starts_with('v'))
        .map(normalize)
        .unwrap_or_default();
    if ver.is_empty() {
        return Err("解析 rclone 版本号失败".into());
    }
    Ok(ver)
}

fn winfsp_latest() -> Result<(String, String), String> {
    // 优先走重定向（零配额），失败再退回 GitHub API
    let url = "https://github.com/winfsp/winfsp/releases/latest".to_string();
    if let Ok(tag) = latest_via_redirect(&url) {
        if !tag.is_empty() {
            return Ok((tag, url));
        }
    }
    let text = get_text(WINFSP_LATEST_API)?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("解析失败：{e}"))?;
    let tag = v["tag_name"].as_str().unwrap_or("").to_string();
    let url = v["html_url"]
        .as_str()
        .unwrap_or("https://winfsp.dev/rel/")
        .to_string();
    if tag.is_empty() {
        return Err("未取到 WinFsp 版本号".into());
    }
    Ok((normalize(&tag), url))
}

fn app_latest() -> Result<(String, String), String> {
    let fallback_url = "https://github.com/Jie-baobao/rclone-mount-manager/releases".to_string();
    if let Ok(tag) = latest_via_redirect("https://github.com/Jie-baobao/rclone-mount-manager/releases/latest") {
        if !tag.is_empty() {
            return Ok((tag, fallback_url));
        }
    }
    let text = get_text(APP_LATEST_API)?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("解析失败：{e}"))?;
    let tag = v["tag_name"].as_str().unwrap_or("").to_string();
    let url = v["html_url"]
        .as_str()
        .unwrap_or(&fallback_url)
        .to_string();
    if tag.is_empty() {
        return Err("还没发布过版本".into());
    }
    Ok((normalize(&tag), url))
}

/// 检查三个组件的版本。任何一个失败都不影响其它项返回。
pub fn check(app: &AppHandle) -> UpdateReport {
    let env = crate::diag::env_info(app);
    let mut items = Vec::new();

    // 管理器自身
    let (latest, url, note) = match app_latest() {
        Ok((v, u)) => (v, u, String::new()),
        Err(e) => (String::new(), "https://github.com/Jie-baobao/rclone-mount-manager/releases".into(), e),
    };
    items.push(VersionInfo {
        name: "管理器".into(),
        current: env.app_version.clone(),
        has_update: !latest.is_empty() && is_newer(&latest, &env.app_version),
        latest,
        url,
        note,
    });

    // rclone
    let (latest, note) = match rclone_latest() {
        Ok(v) => (v, String::new()),
        Err(e) => (String::new(), e),
    };
    items.push(VersionInfo {
        name: "rclone".into(),
        current: env.rclone_version.clone(),
        has_update: !latest.is_empty() && is_newer(&latest, &env.rclone_version),
        latest,
        url: "https://rclone.org/downloads/".into(),
        note,
    });

    // 挂载驱动：Windows 的 WinFsp 有独立版本号，能查官方最新版；
    // macOS 的 macFUSE / Linux 的 libfuse 都跟着系统或 Homebrew/包管理器走，
    // 拿 WinFsp 的版本号去比是错的（之前 Linux 上就显示「最新 2.1」这种毫无意义的对比）。
    let driver = &env.driver;
    let (latest, url, note) = if cfg!(windows) {
        match winfsp_latest() {
            Ok((v, u)) => (v, u, String::new()),
            Err(e) => (String::new(), driver.url.clone(), e),
        }
    } else {
        // 非 Windows 没有"最新版"可查（libfuse / macFUSE 跟着系统走），
        // 界面按语言自己补一句说明，这里留空避免中英混排
        (String::new(), driver.url.clone(), String::new())
    };
    items.push(VersionInfo {
        name: driver.name.clone(),
        current: driver.version.clone(),
        has_update: !latest.is_empty() && is_newer(&latest, &driver.version),
        latest,
        url,
        note,
    });

    UpdateReport { items }
}

// ---------------------------------------------------------------- rclone

fn platform_tag() -> Result<&'static str, String> {
    // 与 rclone 官方发布包命名保持一致
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    Ok(match (os, arch) {
        ("windows", "x86_64") => "windows-amd64",
        ("windows", "aarch64") => "windows-arm64",
        ("macos", "x86_64") => "osx-amd64",
        ("macos", "aarch64") => "osx-arm64",
        ("linux", "x86_64") => "linux-amd64",
        ("linux", "aarch64") => "linux-arm64",
        _ => return Err(format!("暂不支持自动安装该平台：{os}/{arch}，请手动下载 rclone")),
    })
}

fn download(url: &str, dest: &Path) -> Result<(), String> {
    let mut resp = client()?
        .get(url)
        .send()
        .map_err(|e| format!("下载失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败：HTTP {}", resp.status().as_u16()));
    }
    let mut file = fs::File::create(dest).map_err(|e| format!("写入临时文件失败：{e}"))?;
    std::io::copy(&mut resp, &mut file).map_err(|e| format!("保存失败：{e}"))?;
    file.flush().ok();
    Ok(())
}

/// 下载并安装指定版本的 rclone 到应用配置目录的 bin/ 下（优先于内置版本）
pub fn install_rclone(app: &AppHandle, version: &str) -> Result<String, String> {
    let ver = normalize(version);
    if ver.is_empty() {
        return Err("版本号为空".into());
    }
    let plat = platform_tag()?;
    let url = format!("https://downloads.rclone.org/v{ver}/rclone-v{ver}-{plat}.zip");

    let tmp_dir = std::env::temp_dir().join("rclone-mount-manager");
    fs::create_dir_all(&tmp_dir).map_err(|e| format!("创建临时目录失败：{e}"))?;
    let zip_path = tmp_dir.join(format!("rclone-{ver}-{plat}.zip"));
    download(&url, &zip_path)?;

    let file = fs::File::open(&zip_path).map_err(|e| format!("打开压缩包失败：{e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("解析压缩包失败：{e}"))?;

    let want = if cfg!(windows) { "rclone.exe" } else { "rclone" };
    let mut extracted: Option<Vec<u8>> = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("读取压缩包失败：{e}"))?;
        let name = entry.name().to_string();
        if name.ends_with(&format!("/{want}")) || name == want {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("解压失败：{e}"))?;
            extracted = Some(buf);
            break;
        }
    }
    let bytes = extracted.ok_or_else(|| "压缩包里没找到 rclone 可执行文件".to_string())?;

    let bin_dir = store::config_dir(app)?.join("bin");
    fs::create_dir_all(&bin_dir).map_err(|e| format!("创建 bin 目录失败：{e}"))?;
    let target = bin_dir.join(want);

    // 先写临时文件再改名，避免半截二进制被当成可用程序
    let tmp = bin_dir.join(format!("{want}.new"));
    fs::write(&tmp, &bytes).map_err(|e| format!("写入失败：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755));
    }
    fs::rename(&tmp, &target).map_err(|e| format!("替换 rclone 失败：{e}"))?;

    let _ = fs::remove_file(&zip_path);

    let real = rclone::version_of(&target);
    if real.is_empty() {
        return Err("安装完成但无法执行，可能是文件损坏".into());
    }
    Ok(real)
}

// ---------------------------------------------------------------- WinFsp

fn find_bundled_msi(app: &AppHandle) -> Option<PathBuf> {
    let res = app.path().resource_dir().ok()?;
    let cands = [
        res.join("resources").join("winfsp"),
        res.join("winfsp"),
    ];
    for dir in cands {
        if let Ok(entries) = fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().map(|x| x.eq_ignore_ascii_case("msi")).unwrap_or(false) {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn winfsp_msi_url() -> Result<String, String> {
    let text = get_text(WINFSP_LATEST_API)?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("解析失败：{e}"))?;
    let assets = v["assets"].as_array().cloned().unwrap_or_default();
    for a in assets {
        let name = a["name"].as_str().unwrap_or("");
        if name.to_ascii_lowercase().ends_with(".msi") {
            if let Some(url) = a["browser_download_url"].as_str() {
                return Ok(url.to_string());
            }
        }
    }
    Err("官方发布页里没找到 .msi 安装包".into())
}

/// 安装 / 升级 WinFsp。msiexec 需要管理员权限，这里通过 PowerShell 的 RunAs 触发 UAC。
#[cfg(windows)]
pub fn install_winfsp(app: &AppHandle) -> Result<String, String> {
    let msi = match find_bundled_msi(app) {
        Some(p) => p,
        None => {
            let url = winfsp_msi_url()?;
            let tmp_dir = std::env::temp_dir().join("rclone-mount-manager");
            fs::create_dir_all(&tmp_dir).map_err(|e| format!("创建临时目录失败：{e}"))?;
            let dest = tmp_dir.join("winfsp.msi");
            download(&url, &dest)?;
            dest
        }
    };

    let script = format!(
        "Start-Process msiexec -ArgumentList '/i','\"{}\"','/qn','/norestart' -Verb RunAs -Wait",
        msi.display()
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|e| format!("调用安装程序失败：{e}"))?;

    if !status.success() {
        return Err("安装被取消或失败（可能拒绝了 UAC 授权）".into());
    }
    Ok("WinFsp 安装完成".into())
}

#[cfg(not(windows))]
pub fn install_winfsp(_app: &AppHandle) -> Result<String, String> {
    Err("该平台请用系统包管理器安装 FUSE 驱动：macOS 用 brew install --cask macfuse，Linux 用 apt install fuse3".into())
}

/// 用系统默认浏览器打开链接
pub fn open_url(url: &str) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("只允许打开 http/https 链接".into());
    }
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.spawn().map_err(|e| format!("打开链接失败：{e}"))?;
    Ok(())
}
