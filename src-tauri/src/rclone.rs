use crate::model::{Connection, Protocol};
use crate::store;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use tauri::{AppHandle, Manager};

/// rclone 远程名统一加前缀，避免和用户自己的 remote 冲突
pub fn remote_name(id: &str) -> String {
    format!("rmm_{id}")
}

/// 按优先级查找 rclone 可执行文件：
/// 1. 环境变量 RCLONE_PATH
/// 2. 与本程序同目录（打包时可把 rclone 放进来，实现"开箱即用"）
/// 3. 应用配置目录下的 bin/
/// 4. 系统 PATH
pub fn find_rclone(app: &AppHandle) -> Option<PathBuf> {
    candidates(app).into_iter().next().map(|(p, _)| p)
}

/// rclone 的来源，界面上要区分「内置 / 用户更新过 / 系统装的」
pub fn find_rclone_with_source(app: &AppHandle) -> Option<(PathBuf, &'static str)> {
    candidates(app).into_iter().next()
}

/// 按优先级列出所有可能的 rclone 位置：
/// 1. 环境变量 RCLONE_PATH（用户显式指定，最高优先级）
/// 2. 应用配置目录 bin/（用户点「更新」后放这里的版本，优先于内置版本）
/// 3. 本程序同目录 / bin 子目录（便携用法）
/// 4. 安装包内置的 resources 目录
/// 5. 系统 PATH
fn candidates(app: &AppHandle) -> Vec<(PathBuf, &'static str)> {
    let exe = if cfg!(windows) { "rclone.exe" } else { "rclone" };
    let mut out: Vec<(PathBuf, &'static str)> = Vec::new();

    if let Ok(p) = std::env::var("RCLONE_PATH") {
        let p = PathBuf::from(p);
        if p.is_file() {
            out.push((p, "env"));
        }
    }

    if let Ok(dir) = store::config_dir(app) {
        let cand = dir.join("bin").join(exe);
        if cand.is_file() {
            out.push((cand, "updated"));
        }
    }

    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            for cand in [dir.join(exe), dir.join("bin").join(exe)] {
                if cand.is_file() {
                    out.push((cand, "sidecar"));
                }
            }
        }
    }

    // 安装包内置的 rclone（Tauri 会把 resources 原样复制进去）
    if let Ok(res) = app.path().resource_dir() {
        for cand in [
            res.join("resources").join("rclone").join(exe),
            res.join("rclone").join(exe),
        ] {
            if cand.is_file() {
                out.push((cand, "bundled"));
            }
        }
    }

    if let Ok(paths) = std::env::var("PATH") {
        for dir in std::env::split_paths(&paths) {
            let cand = dir.join(exe);
            if cand.is_file() {
                out.push((cand, "path"));
            }
        }
    }

    out
}

pub fn require_rclone(app: &AppHandle) -> Result<PathBuf, String> {
    find_rclone(app).ok_or_else(|| {
        "未找到 rclone。请安装 rclone 并加入 PATH，或把 rclone 可执行文件放到本程序同目录下。"
            .to_string()
    })
}

fn command_for(rclone: &Path) -> Command {
    let mut cmd = Command::new(rclone);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 用 rclone 自己的算法混淆密码（不是加密，但足以避免明文出现在配置文件里）
pub fn obscure(app: &AppHandle, plain: &str) -> Result<String, String> {
    let rclone = require_rclone(app)?;
    let out = command_for(&rclone)
        .arg("obscure")
        .arg(plain)
        .output()
        .map_err(|e| format!("执行 rclone obscure 失败：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "rclone obscure 失败：{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 规范化 WebDAV 地址：
/// - 没写协议时补 http://
/// - 没写端口时补上端口（不少 NAS 的 WebDAV 走非标端口，漏了会打到 80 端口的反代上）
/// - 自动拼接子路径
pub fn normalize_webdav_url(host: &str, port: u16, path: &str) -> String {
    let mut url = host.trim().to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        url = format!("http://{url}");
    }

    let scheme_end = url.find("://").map(|i| i + 3).unwrap_or(0);
    let after_scheme = &url[scheme_end..];
    let has_port = match after_scheme.rfind(':') {
        Some(idx) => !after_scheme[idx..].contains('/'),
        None => false,
    };
    let default_port = url.starts_with("https://") && port == 443
        || url.starts_with("http://") && port == 80;
    if !has_port && !default_port {
        url = match url.find('/') {
            Some(idx) => format!("{}:{}{}", &url[..idx], port, &url[idx..]),
            None => format!("{url}:{port}"),
        };
    } else if !has_port && default_port {
        // 默认端口不必写出来
    }

    let sub = path.trim().trim_matches('/');
    if !sub.is_empty() {
        if !url.ends_with('/') {
            url.push('/');
        }
        url.push_str(sub);
    }
    url
}

/// 把一条连接写进 rclone.conf 中名为 `rmm_<id>` 的 section。
/// 只动自己那一节，用户其它 remote 原样保留。
pub fn upsert_remote(
    app: &AppHandle,
    conn: &Connection,
    plain_password: Option<&str>,
) -> Result<(), String> {
    let conf_path = store::rclone_conf_path(app)?;
    let existing = fs::read_to_string(&conf_path).unwrap_or_default();
    let section = remote_name(&conn.id);

    let mut kept: Vec<String> = Vec::new();
    let mut skipping = false;
    let mut old_pass: Option<String> = None;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let name = &trimmed[1..trimmed.len() - 1];
            skipping = name == section;
            if skipping {
                continue;
            }
        }
        if skipping {
            if trimmed.starts_with("pass =") || trimmed.starts_with("secret_access_key =") {
                old_pass = Some(trimmed.to_string());
            }
            continue;
        }
        kept.push(line.to_string());
    }
    while kept.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        kept.pop();
    }

    let mut body = Vec::new();
    body.push(format!("[{section}]"));
    body.push(format!("type = {}", conn.protocol.as_rclone_type()));

    match conn.protocol {
        Protocol::S3 => {
            let provider = if conn.s3_provider.trim().is_empty() {
                "Other".to_string()
            } else {
                conn.s3_provider.trim().to_string()
            };
            body.push(format!("provider = {provider}"));
            let endpoint = conn.host.trim().trim_end_matches('/');
            if !endpoint.is_empty() {
                body.push(format!("endpoint = {endpoint}"));
            }
            if !conn.region.trim().is_empty() {
                body.push(format!("region = {}", conn.region.trim()));
            }
            if conn.s3_path_style {
                body.push("force_path_style = true".into());
            }
            if !conn.user.is_empty() {
                body.push(format!("access_key_id = {}", conn.user));
            }
            match plain_password {
                Some(p) if !p.is_empty() => {
                    body.push(format!("secret_access_key = {}", obscure(app, p)?))
                }
                _ => {
                    if let Some(keep) = &old_pass {
                        body.push(keep.clone());
                    }
                }
            }
            let mut text = kept.join("\n");
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&body.join("\n"));
            text.push('\n');
            return store::write_atomic(&conf_path, &text);
        }
        Protocol::Webdav => {
            body.push(format!(
                "url = {}",
                normalize_webdav_url(&conn.host, conn.port, &conn.path)
            ));
            body.push("vendor = other".into());
        }
        _ => {
            let host = conn
                .host
                .trim()
                .trim_start_matches("smb://")
                .trim_end_matches('/');
            body.push(format!("host = {host}"));
            body.push(format!("port = {}", conn.port));
            if !conn.path.trim().is_empty() {
                body.push(format!("path = {}", conn.path.trim().trim_matches('/')));
            }
        }
    }

    if !conn.user.is_empty() && !conn.protocol.is_s3() {
        body.push(format!("user = {}", conn.user));
    }
    if let Some(pass) = plain_password {
        if !pass.is_empty() {
            body.push(format!("pass = {}", obscure(app, pass)?));
        }
    } else if let Some(keep) = old_pass {
        // 编辑连接时没重新输入密码，沿用 rclone.conf 里已有的
        body.push(keep);
    }

    let mut text = kept.join("\n");
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(&body.join("\n"));
    text.push('\n');

    store::write_atomic(&conf_path, &text)
}

/// 从 rclone.conf 里删掉对应 section（删除连接时调用）
pub fn remove_remote(app: &AppHandle, id: &str) -> Result<(), String> {
    let conf_path = store::rclone_conf_path(app)?;
    let Ok(existing) = fs::read_to_string(&conf_path) else {
        return Ok(());
    };
    let section = remote_name(id);
    let mut kept = Vec::new();
    let mut skipping = false;
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = &trimmed[1..trimmed.len() - 1] == section;
            if skipping {
                continue;
            }
        }
        if !skipping {
            kept.push(line.to_string());
        }
    }
    store::write_atomic(&conf_path, &kept.join("\n"))
}

/// 挂载命令行参数（跨平台差异集中在这里）
pub fn build_mount_args(
    app: &AppHandle,
    conn: &Connection,
    log_path: &Path,
) -> Result<Vec<String>, String> {
    let conf = store::rclone_conf_path(app)?;
    let mut spec = format!("{}:", remote_name(&conn.id));
    if conn.protocol.is_s3() {
        let bucket = conn.path.trim().trim_matches('/');
        if !bucket.is_empty() {
            spec = format!("{}:{}", remote_name(&conn.id), bucket);
        }
    }

    let mut args: Vec<String> = vec![
        "mount".into(),
        spec,
        conn.target.clone(),
        "--config".into(),
        conf.to_string_lossy().to_string(),
        "--vfs-cache-mode".into(),
        "full".into(),
        "--dir-cache-time".into(),
        "30s".into(),
        "--attr-timeout".into(),
        "5s".into(),
        "--log-file".into(),
        log_path.to_string_lossy().to_string(),
        "--log-level".into(),
        "INFO".into(),
        "--volname".into(),
        conn.name.clone(),
    ];

    let cache = if conn.cache_dir.trim().is_empty() {
        store::config_dir(app)?.join("cache").join(&conn.id)
    } else {
        PathBuf::from(conn.cache_dir.trim())
    };
    let _ = fs::create_dir_all(&cache);
    args.push("--cache-dir".into());
    args.push(cache.to_string_lossy().to_string());

    if conn.protocol.is_webdav() && conn.ignore_cert {
        args.push("--no-check-certificate".into());
    }

    // 只读挂载：适合「只看不改」的资料库，能避免误删
    if conn.read_only {
        args.push("--read-only".into());
    }

    // 带宽限制：异地走 P2P / 公网时很有用，避免把上行占满
    let bw = conn.bwlimit.trim();
    if !bw.is_empty() {
        args.push("--bwlimit".into());
        args.push(bw.to_string());
    }

    // VFS 缓存上限，防止大文件把本地磁盘撑爆
    let cache_max = conn.cache_max_size.trim();
    if !cache_max.is_empty() {
        args.push("--vfs-cache-max-size".into());
        args.push(cache_max.to_string());
    }

    // 网络抖动时多试几次，比直接断开对用户友好
    args.push("--low-level-retries".into());
    args.push("10".into());
    args.push("--retries".into());
    args.push("3".into());

    #[cfg(windows)]
    {
        // 盘符必须形如 Z:
        let t = conn.target.trim();
        let bytes = t.as_bytes();
        let valid = bytes.len() == 2
            && (bytes[0] as char).is_ascii_alphabetic()
            && bytes[1] == b':';
        if !valid {
            return Err("Windows 上目标必须是盘符，例如 Z:".into());
        }
        // 挂成「网络驱动器」还是「本地磁盘」
        if conn.network_mode {
            args.push("--network-mode".into());
        }
    }

    Ok(args)
}

/// 启动挂载进程（不阻塞 UI）
pub fn spawn_mount(
    app: &AppHandle,
    conn: &Connection,
    log_path: &Path,
) -> Result<Child, String> {
    let rclone = require_rclone(app)?;
    let args = build_mount_args(app, conn, log_path)?;

    let mut cmd = command_for(&rclone);
    cmd.args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    // 代理：rclone 读标准的环境变量
    let proxy = conn.proxy.trim();
    if !proxy.is_empty() {
        for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
            cmd.env(key, proxy);
        }
    }

    // 类 Unix 上脱离父进程，避免应用退出时把挂载一起带走
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn().map_err(|e| {
        format!(
            "启动 rclone 挂载失败：{e}\n命令：{} {}",
            rclone.display(),
            args.join(" ")
        )
    })
}

/// 卸载：类 Unix 走 fusermount，Windows 直接结束进程
pub fn unmount(child: &mut Child, target: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        let _ = Command::new("fusermount")
            .arg("-u")
            .arg(target)
            .status()
            .or_else(|_| Command::new("umount").arg(target).status());
    }
    #[cfg(not(unix))]
    let _ = target;

    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

/// 从 rclone.conf 里取回某个连接的明文密码（供"测试连接"复用已保存的密码）。
/// rclone 的 obscure 是可逆混淆，用 `rclone reveal` 还原。
pub fn reveal_password(app: &AppHandle, id: &str) -> Option<String> {
    let conf_path = store::rclone_conf_path(app).ok()?;
    let text = fs::read_to_string(conf_path).ok()?;
    let section = remote_name(id);

    let mut in_section = false;
    let mut obscured: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = &trimmed[1..trimmed.len() - 1] == section;
            continue;
        }
        if in_section {
            if let Some(rest) = trimmed.strip_prefix("pass =") {
                obscured = Some(rest.trim().to_string());
                break;
            }
        }
    }

    let obscured = obscured?;
    let rclone = find_rclone(app)?;
    let out = command_for(&rclone)
        .arg("reveal")
        .arg(&obscured)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 读取 rclone 版本号（取 `rclone version` 第一行里的 vX.Y.Z）
pub fn version_of(rclone: &Path) -> String {
    command_for(rclone)
        .arg("version")
        .output()
        .ok()
        .map(|o| {
            let first = String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            first
                .split_whitespace()
                .find(|w| w.starts_with('v'))
                .unwrap_or("")
                .trim_start_matches('v')
                .to_string()
        })
        .unwrap_or_default()
}

/// 读取日志尾部，供 UI 展示真实错误
pub fn tail_log(path: &Path, max_lines: usize) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

/// 对一条连接做一次轻量探测，返回往返毫秒数。
///
/// 用 `rclone lsd --max-depth 0` 而不是 ping：走的是真实协议栈，
/// 能同时反映「网络慢」和「认证被拒 / 服务端异常」两种情况。
pub fn probe_latency(app: &AppHandle, conn: &Connection, timeout_secs: u64) -> Result<u64, String> {
    let rclone = find_rclone(app).ok_or_else(|| "未找到 rclone".to_string())?;
    let conf = store::rclone_conf_path(app)?;

    let mut spec = format!("{}:", remote_name(&conn.id));
    if conn.protocol.is_s3() {
        let bucket = conn.path.trim().trim_matches('/');
        if !bucket.is_empty() {
            spec = format!("{}:{}", remote_name(&conn.id), bucket);
        }
    }

    let mut cmd = command_for(&rclone);
    cmd.arg("lsd")
        .arg(&spec)
        .arg("--max-depth")
        .arg("0")
        .arg("--config")
        .arg(conf.to_string_lossy().to_string())
        .arg("--timeout")
        .arg(format!("{timeout_secs}s"))
        .arg("--contimeout")
        .arg(format!("{timeout_secs}s"))
        .arg("--retries")
        .arg("1")
        .arg("--low-level-retries")
        .arg("1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let proxy = conn.proxy.trim();
    if !proxy.is_empty() {
        for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
            cmd.env(key, proxy);
        }
    }

    let started = std::time::Instant::now();
    let mut child = cmd.spawn().map_err(|e| format!("探测失败：{e}"))?;
    let deadline = std::time::Duration::from_secs(timeout_secs + 2);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(started.elapsed().as_millis() as u64);
                }
                let mut buf = String::new();
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut buf);
                }
                let line = buf
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("探测失败")
                    .trim()
                    .to_string();
                return Err(line);
            }
            Ok(None) => {
                if started.elapsed() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("探测超时".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            Err(e) => return Err(format!("探测失败：{e}")),
        }
    }
}
