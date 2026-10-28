use crate::model::{Connection, DiagResult, Protocol};
use crate::rclone;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use tauri::AppHandle;

/// 平台挂载驱动。三个系统跑的根本不是同一个东西：
///   Windows → WinFsp（独立的内核驱动，要单独安装）
///   macOS   → macFUSE（内核扩展）或 FUSE-T（NFS 实现，不需要内核扩展）
///   Linux   → 内核自带的 FUSE 子系统 + libfuse（/dev/fuse），发行版一般已带
/// 所以界面上不能写死「WinFsp」，得按实际平台显示。
#[derive(serde::Serialize, Clone, Debug)]
pub struct MountDriver {
    /// 界面直接显示的名字：WinFsp / macFUSE / FUSE-T / FUSE 3 / FUSE
    pub name: String,
    /// 驱动版本（Linux 上是 libfuse 的版本），探测不到时为空
    pub version: String,
    pub ready: bool,
    /// 未就绪时的安装指引；就绪时为空
    pub hint: String,
    /// 能否在应用内一键安装（目前只有 Windows 的 WinFsp 具备）
    pub installable: bool,
    /// 官网 / 安装说明
    pub url: String,
}

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
    /// 当前平台实际的挂载驱动
    pub driver: MountDriver,
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

    EnvInfo {
        platform,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        rclone_path,
        rclone_version,
        rclone_bundled: rclone_source == "bundled",
        rclone_source,
        driver: mount_driver(),
    }
}

/// 按平台探测挂载驱动（名字、版本、是否就绪、怎么装）
pub fn mount_driver() -> MountDriver {
    #[cfg(windows)]
    {
        let version = winfsp_version();
        let driver_dir = std::path::Path::new(r"C:\Program Files (x86)\WinFsp\bin");
        let svc_running = std::process::Command::new("sc")
            .args(["query", "WinFsp.Launcher"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("RUNNING"))
            .unwrap_or(false);
        let ready = svc_running || driver_dir.exists();
        MountDriver {
            name: "WinFsp".into(),
            version,
            ready,
            hint: if ready {
                String::new()
            } else {
                "未检测到 WinFsp：点「一键安装」即可（会弹管理员授权）".into()
            },
            installable: true,
            url: "https://winfsp.dev/rel/".into(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        mac_driver()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        linux_driver()
    }
}

// ---------------------------------------------------------------- Windows

/// WinFsp 版本：读卸载项里的 DisplayVersion
#[cfg(windows)]
fn winfsp_version() -> String {
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

// ---------------------------------------------------------------- macOS

/// macOS：macFUSE（内核扩展）与 FUSE-T（NFS 实现）二选一，rclone 两个都认
#[cfg(target_os = "macos")]
fn mac_driver() -> MountDriver {
    if std::path::Path::new("/Library/Filesystems/macfuse.fs").exists() {
        let version = command_output(
            "defaults",
            &[
                "read",
                "/Library/Filesystems/macfuse.fs/Contents/Info",
                "CFBundleShortVersionString",
            ],
        )
        .and_then(|s| first_version_token(&s))
        .unwrap_or_default();
        return MountDriver {
            name: "macFUSE".into(),
            version,
            ready: true,
            hint: String::new(),
            installable: false,
            url: "https://macfuse.io/".into(),
        };
    }

    if std::path::Path::new("/usr/local/lib/libfuse-t.dylib").exists() {
        let version = ["/opt/homebrew/Cellar/fuse-t", "/usr/local/Cellar/fuse-t"]
            .iter()
            .filter_map(|p| newest_dir_version(p))
            .max_by(|a, b| cmp_version(a, b))
            .unwrap_or_default();
        return MountDriver {
            name: "FUSE-T".into(),
            version,
            ready: true,
            hint: String::new(),
            installable: false,
            url: "https://www.fuse-t.org/".into(),
        };
    }

    MountDriver {
        name: "macFUSE".into(),
        version: String::new(),
        ready: false,
        hint: "未检测到 macFUSE / FUSE-T：可安装 macFUSE（brew install --cask macfuse）或 FUSE-T（brew install --cask fuse-t），装完需重启".into(),
        installable: false,
        url: "https://macfuse.io/".into(),
    }
}

/// 取目录下形如 1.2.3 的子目录名里最新的那个（Homebrew Cellar 布局）
#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn newest_dir_version(dir: &str) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut versions: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .filter(|name| name.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
        .collect();
    versions.sort_by(|a, b| cmp_version(a, b));
    versions.pop()
}

// ---------------------------------------------------------------- Linux

/// Linux：驱动就是内核自带的 FUSE 子系统，能不能挂载看 /dev/fuse 在不在；
/// 版本取的是 libfuse（rclone 链接的动态库），各家发行版包名和查询方式都不一样。
#[cfg(all(unix, not(target_os = "macos")))]
fn linux_driver() -> MountDriver {
    let ready = std::path::Path::new("/dev/fuse").exists();
    let version = linux_libfuse_version();
    let name = if version.starts_with("2.") {
        "FUSE 2".to_string()
    } else if version.starts_with("3.") {
        "FUSE 3".to_string()
    } else {
        "FUSE".to_string()
    };

    MountDriver {
        name,
        version,
        ready,
        hint: if ready {
            String::new()
        } else {
            "未检测到 /dev/fuse：请安装 fuse3（Debian/Ubuntu/deepin: sudo apt install fuse3；Fedora: sudo dnf install fuse3），容器/虚拟机里还要放开 FUSE 设备".into()
        },
        installable: false,
        url: "https://github.com/libfuse/libfuse".into(),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_libfuse_version() -> String {
    // 1) fusermount3 自带版本号，最省事（Debian 系、Arch 都有）
    if let Some(v) = command_output("fusermount3", &["--version"]).and_then(|s| first_version_token(&s)) {
        return v;
    }
    // 2) Debian / Ubuntu / deepin
    if let Some(v) = command_output("dpkg-query", &["-W", "-f=${Version}", "libfuse3-3"])
        .and_then(|s| first_version_token(&s))
    {
        return v;
    }
    // 3) Fedora / RHEL / openSUSE
    if let Some(v) = command_output("rpm", &["-q", "--qf", "%{VERSION}", "fuse3-libs"])
        .and_then(|s| first_version_token(&s))
    {
        return v;
    }
    // 4) 装了开发包 / 源码编译的
    if let Some(v) = command_output("pkg-config", &["--modversion", "fuse3"])
        .and_then(|s| first_version_token(&s))
    {
        return v;
    }
    // 5) 只有 libfuse2 的老系统
    if let Some(v) = command_output("fusermount", &["--version"]).and_then(|s| first_version_token(&s)) {
        return v;
    }
    String::new()
}

// ---------------------------------------------------------------- 小工具

#[allow(dead_code)] // Windows 上走注册表，用不到这三个函数
fn command_output(prog: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(prog).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// 从「fusermount3 version: 3.14.0」「3.14.0-4」这类文本里抠出版本号
#[allow(dead_code)]
fn first_version_token(text: &str) -> Option<String> {
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| c == 'v' || c == 'V');
        let mut parts = trimmed.split('.');
        let (Some(major), Some(minor)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !major.is_empty()
            && major.chars().all(|c| c.is_ascii_digit())
            && minor.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
        {
            // 去掉 Debian 的 "-4" 后缀：3.14.0-4 → 3.14.0
            let clean: String = trimmed
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            return Some(clean.trim_end_matches('.').to_string());
        }
    }
    None
}

#[allow(dead_code)]
fn cmp_version(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> { s.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect() };
    parse(a).cmp(&parse(b))
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
