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
    let raw = host.trim().trim_end_matches('/');
    let url = if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };

    // 注意：必须从协议之后开始找斜杠，否则 "http://" 里的双斜杠会被当成路径分隔符，
    // 拼出 "http:5005//192.168.50.4" 这种非法地址（rclone 报 builder error）。
    let scheme_end = url.find("://").map(|i| i + 3).unwrap_or(0);
    let after_scheme = &url[scheme_end..];
    let (authority, rest) = match after_scheme.find('/') {
        Some(idx) => (&after_scheme[..idx], &after_scheme[idx..]),
        None => (after_scheme, ""),
    };

    // IPv6 字面量形如 [::1]:5005，只有 "]" 之后的冒号才是端口
    let has_port = match authority.rfind(']') {
        Some(idx) => authority[idx..].contains(':'),
        None => authority.contains(':'),
    };

    let is_https = url.starts_with("https://");
    let default_port = if is_https { port == 443 } else { port == 80 };
    let mut out = format!("{}{}", &url[..scheme_end], authority);
    if !has_port && !default_port {
        out.push(':');
        out.push_str(&port.to_string());
    }

    // 地址里自带的子路径（例如 https://example.com/dav）要保留
    out.push_str(rest.trim_end_matches('/'));

    let sub = path.trim().trim_matches('/');
    if !sub.is_empty() {
        out.push('/');
        out.push_str(sub);
    }
    out
}

/// 把一条连接写进 rclone.conf 中名为 `rmm_<id>` 的 section。
/// 只动自己那一节，用户其它 remote 原样保留。
pub fn upsert_remote(
    app: &AppHandle,
    conn: &Connection,
    plain_password: Option<&str>,
) -> Result<(), String> {
    write_remote(app, conn, PasswordSource::Plain(plain_password))
}

/// 导入连接配置时用：密码已经是 rclone obscure 过的字符串，直接落盘即可。
pub fn upsert_remote_obscured(
    app: &AppHandle,
    conn: &Connection,
    obscured: Option<&str>,
) -> Result<(), String> {
    write_remote(app, conn, PasswordSource::Obscured(obscured))
}

/// 密码来源：明文（需先 obscure）或已混淆（导入时直接写）。
enum PasswordSource<'a> {
    Plain(Option<&'a str>),
    Obscured(Option<&'a str>),
}

fn write_remote(
    app: &AppHandle,
    conn: &Connection,
    source: PasswordSource<'_>,
) -> Result<(), String> {
    let plain_password = match &source {
        PasswordSource::Plain(p) => *p,
        PasswordSource::Obscured(_) => None,
    };
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
            match (&source, plain_password) {
                (PasswordSource::Plain(Some(p)), _) if !p.is_empty() => {
                    body.push(format!("secret_access_key = {}", obscure(app, p)?))
                }
                (PasswordSource::Obscured(Some(o)), _) if !o.is_empty() => {
                    body.push(format!("secret_access_key = {o}"))
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
    match &source {
        PasswordSource::Plain(Some(pass)) if !pass.is_empty() => {
            body.push(format!("pass = {}", obscure(app, pass)?));
        }
        PasswordSource::Obscured(Some(o)) if !o.is_empty() => {
            body.push(format!("pass = {o}"));
        }
        _ => {
            // 编辑连接时没重新输入密码，沿用 rclone.conf 里已有的
            if let Some(keep) = old_pass {
                body.push(keep);
            }
        }
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
    rc_port: u16,
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
        volname(conn),
        // 本机 rc 端口：界面靠它读实时吞吐/传输统计，只绑定回环地址
        "--rc".into(),
        "--rc-no-auth".into(),
        "--rc-addr".into(),
        format!("127.0.0.1:{rc_port}"),
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

/// 卷标：有分组时显示成 `[工作] NAS`，资源管理器按名称排序时同组磁盘自然相邻。
/// 注意：卷名在挂载那一刻定死，改组名/名称后要重新挂载才会刷新。
fn volname(conn: &Connection) -> String {
    let g = conn.group.trim();
    let n = conn.name.trim();
    if g.is_empty() {
        n.to_string()
    } else {
        format!("[{g}] {n}")
    }
}

/// 读系统代理设置。Windows 读「Internet 选项」注册表；类 Unix 读环境变量。
pub fn system_proxy_url() -> Option<String> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let ok = hkcu
            .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
            .ok()?;
        let enabled: u32 = ok.get_value("ProxyEnable").ok()?;
        if enabled != 1 {
            return None;
        }
        let server: String = ok.get_value("ProxyServer").ok()?;
        let server = server.trim().to_string();
        if server.is_empty() {
            return None;
        }
        // 可能是 "127.0.0.1:7890"，也可能是 "http=...;https=..." 的分段写法
        if server.contains('=') {
            let https = server
                .split(';')
                .find_map(|p| {
                    let (k, v) = p.split_once('=')?;
                    k.trim().eq_ignore_ascii_case("https").then(|| v.trim().to_string())
                })
                .or_else(|| {
                    server.split(';').find_map(|p| {
                        let (k, v) = p.split_once('=')?;
                        k.trim().eq_ignore_ascii_case("http").then(|| v.trim().to_string())
                    })
                })?;
            return Some(format!("http://{https}"));
        }
        if server.contains("://") {
            Some(server)
        } else {
            Some(format!("http://{server}"))
        }
    }
    #[cfg(not(windows))]
    {
        for key in ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
            if let Ok(v) = std::env::var(key) {
                let v = v.trim().to_string();
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
        None
    }
}

/// 给子进程注入代理环境变量：手动填写的 proxy 优先，其次按开关走系统代理。
/// 无论如何都把回环地址排除掉，避免本机 rc 端口的统计请求被代理截走。
fn apply_proxy_env(cmd: &mut Command, conn: &Connection) {
    let proxy = if !conn.proxy.trim().is_empty() {
        Some(conn.proxy.trim().to_string())
    } else if conn.use_system_proxy {
        system_proxy_url()
    } else {
        None
    };
    if let Some(p) = proxy {
        for key in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
            cmd.env(key, &p);
        }
        cmd.env("NO_PROXY", "localhost,127.0.0.1,::1");
        cmd.env("no_proxy", "localhost,127.0.0.1,::1");
    }
}

/// 启动挂载进程（不阻塞 UI）
pub fn spawn_mount(
    app: &AppHandle,
    conn: &Connection,
    log_path: &Path,
    rc_port: u16,
) -> Result<Child, String> {
    let rclone = require_rclone(app)?;
    let args = build_mount_args(app, conn, log_path, rc_port)?;

    let mut cmd = command_for(&rclone);
    cmd.args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    // 代理：rclone 读标准的环境变量（手动填写优先，其次系统代理）
    apply_proxy_env(&mut cmd, conn);

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
    let obscured = raw_password(app, id)?;
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

/// 取出 rclone.conf 中该连接已混淆的密码原文（导出配置时随连接一起带走，
/// 明文密码永远不会出现在导出文件里）。
pub fn raw_password(app: &AppHandle, id: &str) -> Option<String> {
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
            for key in ["pass =", "secret_access_key ="] {
                if let Some(rest) = trimmed.strip_prefix(key) {
                    let v = rest.trim();
                    if !v.is_empty() {
                        obscured = Some(v.to_string());
                    }
                }
            }
            if obscured.is_some() {
                break;
            }
        }
    }
    obscured
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
///
/// **注意碰盘**：lsd 会让 NAS 读一次目录元数据，会把休眠中的硬盘唤醒。
/// 有休眠需求的连接请在表单里把「探测方式」改成轻量或仅进程。
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
    // 只看根目录这一层就够了。注意 --max-depth 0 在 rclone 里表示「不限深度」，
    // 会递归遍历整棵树（几万个目录），几十秒都跑不完，必然被判超时。
    cmd.arg("lsd")
        .arg(&spec)
        .arg("--max-depth")
        .arg("1")
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

    // 自签证书的场景下挂载能连（挂了 --no-check-certificate），探测也得跟上，否则永远报失败
    if conn.protocol.is_webdav() && conn.ignore_cert {
        cmd.arg("--no-check-certificate");
    }

    apply_proxy_env(&mut cmd, conn);

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

/// 轻量探测：只做一次 TCP 握手，不发起任何文件系统请求。
/// 完全不碰盘，NAS 硬盘可以安心休眠；代价是测不出「服务活着但文件层异常」的情况。
pub fn tcp_probe(conn: &Connection, timeout_secs: u64) -> Result<u64, String> {
    use std::net::{SocketAddr, TcpStream, ToSocketAddrs};

    // WebDAV 允许直接填完整 URL，取里面的主机；其余协议 host 就是主机名/IP
    let mut host = conn.host.trim().to_string();
    if let Some(pos) = host.find("://") {
        host = host[pos + 3..].to_string();
    }
    let host = host.split('/').next().unwrap_or("").trim_end_matches('/');
    if host.is_empty() {
        return Err("未配置服务器地址".into());
    }
    let port = if conn.port == 0 { 80 } else { conn.port };

    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("域名解析失败：{e}"))?
        .collect();
    if addrs.is_empty() {
        return Err("域名解析失败：没有可用地址".into());
    }

    let timeout = std::time::Duration::from_secs(timeout_secs.max(1));
    let started = std::time::Instant::now();
    let mut last_err = String::new();
    for addr in &addrs {
        // IPv6 回环不该走外网代理，TcpStream 本来就不经代理，这里保持直连即可
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(_) => return Ok(started.elapsed().as_millis() as u64),
            Err(e) => last_err = format!("{addr}：{e}"),
        }
    }
    Err(format!("端口 {port} 连不上（{last_err}）"))
}

/// 调本机 rclone 的 rc 接口读统计（只读、2 秒超时，失败就当作没有数据）。
/// 走 127.0.0.1 回环，不算外网流量，开销可以忽略。
pub fn rc_call(port: u16, endpoint: &str) -> Option<serde_json::Value> {
    if port == 0 {
        return None;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .no_proxy()
        .build()
        .ok()?;
    let url = format!("http://127.0.0.1:{port}/{endpoint}");
    // 手动解析：reqwest 的 json feature 没开，直接用 serde_json 读文本更省依赖
    let body = client
        .post(&url)
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .ok()?;
    serde_json::from_str::<serde_json::Value>(&body).ok()
}
