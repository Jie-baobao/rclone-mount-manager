use crate::model::{Connection, DiagResult, Protocol};
use crate::rclone;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use tauri::AppHandle;

/// 依赖环境自检结果，UI 启动时展示
#[derive(serde::Serialize, Clone, Debug)]
pub struct EnvInfo {
    pub platform: String,
    /// 管理器自身版本（来自 Cargo.toml）
    pub app_version: String,
    pub rclone_path: String,
    pub rclone_version: String,
    /// bundled / updated / sidecar / env / path
    pub rclone_source: String,
    /// 是否用的是安装包内置的 rclone
    pub rclone_bundled: bool,
    pub fuse_ready: bool,
    pub fuse_hint: String,
    /// 挂载驱动版本（Windows 上为 WinFsp 版本）
    pub fuse_version: String,
}

pub fn env_info(app: &AppHandle) -> EnvInfo {
    let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);

    let (rclone_path, rclone_version, rclone_source) = match rclone::find_rclone_with_source(app) {
        Some((p, source)) => (
            p.to_string_lossy().to_string(),
            rclone::version_of(&p),
            source.to_string(),
        ),
        None => (String::new(), String::new(), String::new()),
    };

    let (fuse_ready, fuse_hint) = check_fuse();
    let fuse_version = fuse_version();

    EnvInfo {
        platform,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        rclone_path,
        rclone_version,
        rclone_bundled: rclone_source == "bundled",
        rclone_source,
        fuse_ready,
        fuse_hint,
        fuse_version,
    }
}

/// 挂载驱动版本：Windows 读 WinFsp 的卸载项，macOS / Linux 尽力而为
fn fuse_version() -> String {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
        use winreg::RegKey;

        let roots = [
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ];
        for root in roots {
            let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(root, KEY_READ) else {
                continue;
            };
            for sub in key.enum_keys().flatten() {
                let Ok(item) = key.open_subkey_with_flags(&sub, KEY_READ) else {
                    continue;
                };
                let name: String = item.get_value("DisplayName").unwrap_or_default();
                if name.to_lowercase().contains("winfsp") {
                    let ver: String = item.get_value("DisplayVersion").unwrap_or_default();
                    if !ver.is_empty() {
                        return ver;
                    }
                }
            }
        }
        String::new()
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

/// 检查平台挂载驱动是否就位
fn check_fuse() -> (bool, String) {
    #[cfg(windows)]
    {
        let svc = std::process::Command::new("sc")
            .args(["query", "WinFsp.Launcher"])
            .output();
        let driver_dir = std::path::Path::new(r"C:\Program Files (x86)\WinFsp\bin");
        let ok = svc.map(|o| String::from_utf8_lossy(&o.stdout).contains("RUNNING")).unwrap_or(false)
            || driver_dir.exists();
        (
            ok,
            if ok {
                "WinFsp 已就绪".into()
            } else {
                "未检测到 WinFsp，请先安装 https://winfsp.dev".into()
            },
        )
    }
    #[cfg(target_os = "macos")]
    {
        let ok = std::path::Path::new("/usr/local/lib/libfuse-t.dylib").exists()
            || std::path::Path::new("/Library/Filesystems/macfuse.fs").exists();
        (
            ok,
            if ok {
                "FUSE 已就绪".into()
            } else {
                "请先安装 macFUSE 或 FUSE-T（brew install --cask macfuse）".into()
            },
        )
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let ok = std::path::Path::new("/dev/fuse").exists();
        (
            ok,
            if ok {
                "fuse3 已就绪".into()
            } else {
                "请先安装 fuse3（apt install fuse3 / dnf install fuse3）".into()
            },
        )
    }
}

/// 连接测试：
/// - WebDAV 用真实 PROPFIND 请求，能区分 401 / 403 / 404，直接说明问题在哪
/// - 其它协议只能做 TCP 可达性探测（认证要等实际挂载才知道）
pub fn test_connection(
    app: &AppHandle,
    conn: &Connection,
    plain_password: Option<String>,
) -> DiagResult {
    match conn.protocol {
        Protocol::Webdav => test_webdav(app, conn, plain_password),
        Protocol::S3 => test_s3(conn),
        _ => test_tcp(conn),
    }
}

fn test_webdav(app: &AppHandle, conn: &Connection, plain_password: Option<String>) -> DiagResult {
    let url = rclone::normalize_webdav_url(&conn.host, conn.port, &conn.path);
    let password = plain_password.or_else(|| rclone::reveal_password(app, &conn.id));

    let client = match reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(conn.ignore_cert)
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return DiagResult::fail(format!("创建 HTTP 客户端失败：{e}"), url),
    };

    let mut req = client
        .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), url.as_str())
        .header("Depth", "0");
    if !conn.user.is_empty() {
        req = req.basic_auth(&conn.user, password.as_deref());
    }

    match req.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let www = resp
                .headers()
                .get("www-authenticate")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            let body = resp.text().unwrap_or_default();
            let detail = format!(
                "{}{}",
                if www.is_empty() {
                    String::new()
                } else {
                    format!("WWW-Authenticate: {www}\n")
                },
                body.chars().take(400).collect::<String>()
            );

            let (ok, message) = match status {
                207 | 200 => (true, "认证通过，WebDAV 服务正常".to_string()),
                401 => (
                    false,
                    "账号或密码不正确（服务端返回 401）。请检查：1) 输入法是否把 ! 打成全角！；2) 密码末尾是否多了空格；3) 是否开启了二次验证需要专用密码".to_string(),
                ),
                403 => (false, "认证通过但无权访问该路径（403），请检查共享目录授权".into()),
                404 => (false, "路径不存在（404），请检查子路径是否写错".into()),
                405 => (false, "该地址不响应 WebDAV 请求（405），可能不是 WebDAV 服务或需要用 HTTPS".into()),
                s if s >= 500 => (false, format!("服务端错误（{s}），请查看 NAS 日志")),
                s => (false, format!("未预期的状态码 {s}")),
            };

            DiagResult {
                ok,
                code: Some(status),
                message,
                url,
                detail,
            }
        }
        Err(e) => {
            let msg = if e.is_timeout() {
                "连接超时：地址或端口不通，或被防火墙拦截".to_string()
            } else if e.is_connect() {
                "无法建立连接：端口未开放或主机不可达".to_string()
            } else if e.to_string().contains("certificate") {
                "证书校验失败：自签证书请勾选「忽略证书校验」".to_string()
            } else {
                format!("请求失败：{e}")
            };
            DiagResult::fail(msg, url)
        }
    }
}

fn test_tcp(conn: &Connection) -> DiagResult {
    let addr = format!("{}:{}", conn.host.trim(), conn.port);
    let target = format!("{}:{}", conn.host.trim(), conn.port);
    match addr.to_socket_addrs() {
        Ok(mut addrs) => {
            if let Some(sock) = addrs.next() {
                match TcpStream::connect_timeout(&sock, Duration::from_secs(6)) {
                    Ok(_) => DiagResult {
                        ok: true,
                        code: None,
                        message: format!("{} 端口可达，可以尝试挂载", conn.protocol),
                        url: target,
                        detail: String::new(),
                    },
                    Err(e) => DiagResult::fail(format!("端口连接失败：{e}"), target),
                }
            } else {
                DiagResult::fail("域名解析结果为空", target)
            }
        }
        Err(e) => DiagResult::fail(format!("无法解析主机名：{e}"), target),
    }
}

/// S3 兼容：先确认 endpoint 可解析、可连通（真实的 AK/SK 校验要等挂载）
fn test_s3(conn: &Connection) -> DiagResult {
    let raw = conn.host.trim();
    let url = if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.trim_end_matches('/').to_string()
    } else {
        format!("https://{}", raw.trim_end_matches('/'))
    };
    let host_port = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string();
    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(if url.starts_with("https") { 443 } else { 80 })),
        None => (
            host_port,
            if url.starts_with("https://") { 443 } else { 80 },
        ),
    };

    match (host.as_str(), port).to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(sock) => match TcpStream::connect_timeout(&sock, Duration::from_secs(8)) {
                Ok(_) => DiagResult {
                    ok: true,
                    code: None,
                    message: format!(
                        "服务地址可达。凭据与存储桶的校验会在挂载时完成（Bucket：{}）",
                        if conn.path.trim().is_empty() { "未指定" } else { conn.path.trim() }
                    ),
                    url,
                    detail: String::new(),
                },
                Err(e) => DiagResult::fail(format!("连接失败：{e}"), url),
            },
            None => DiagResult::fail("域名解析结果为空", url),
        },
        Err(e) => DiagResult::fail(format!("无法解析主机名：{e}"), url),
    }
}
