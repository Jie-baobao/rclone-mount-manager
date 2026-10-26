use serde::{Deserialize, Serialize};
use std::fmt;

/// 支持的连接协议。rclone 的 backend 名称与这里一一对应。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Smb,
    Webdav,
    Ftp,
    Sftp,
}

impl Protocol {
    pub fn as_rclone_type(self) -> &'static str {
        match self {
            Protocol::Smb => "smb",
            Protocol::Webdav => "webdav",
            Protocol::Ftp => "ftp",
            Protocol::Sftp => "sftp",
        }
    }

    /// 各协议常用默认端口（WebDAV 标准化端口为 80/443，非标端口可在界面上改）
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Smb => 445,
            Protocol::Webdav => 80,
            Protocol::Ftp => 21,
            Protocol::Sftp => 22,
        }
    }

    pub fn is_webdav(self) -> bool {
        matches!(self, Protocol::Webdav)
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Protocol::Smb => "SMB",
            Protocol::Webdav => "WebDAV",
            Protocol::Ftp => "FTP",
            Protocol::Sftp => "SFTP",
        };
        write!(f, "{s}")
    }
}

/// 一条连接配置。**不保存密码**：密码经 `rclone obscure` 后写入 rclone.conf，
/// 这样既支持开机自动挂载，又不会在应用自己的配置文件里留明文。
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    /// 主机名或 IP，WebDAV 可直接填完整 URL
    pub host: String,
    pub port: u16,
    /// SMB 共享名 / WebDAV 子路径（可空）
    pub path: String,
    pub user: String,
    /// Windows 盘符（"Z:"）或类 Unix 挂载点（"/mnt/nas"）
    pub target: String,
    pub cache_dir: String,
    /// WebDAV 自签证书时忽略校验
    pub ignore_cert: bool,
    /// 只读挂载
    pub read_only: bool,
    /// Windows：挂成「网络驱动器」而不是「本地磁盘」
    pub network_mode: bool,
    /// 带宽限制，例如 "10M"；空字符串表示不限
    pub bwlimit: String,
    /// VFS 缓存上限，例如 "10G"
    pub cache_max_size: String,
    /// HTTP 代理，例如 http://127.0.0.1:7890
    pub proxy: String,
    /// 意外退出时自动重连
    pub reconnect: bool,
    /// 启动应用时自动挂载
    pub auto_mount: bool,
    /// 列表置顶
    pub pinned: bool,
    pub created_at: i64,
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            protocol: Protocol::Smb,
            host: String::new(),
            port: 445,
            path: String::new(),
            user: String::new(),
            target: String::new(),
            cache_dir: String::new(),
            ignore_cert: false,
            read_only: false,
            network_mode: false,
            bwlimit: String::new(),
            cache_max_size: "10G".into(),
            proxy: String::new(),
            reconnect: true,
            auto_mount: false,
            pinned: false,
            created_at: 0,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct MountState {
    pub id: String,
    pub mounted: bool,
    pub pid: Option<u32>,
    pub since: Option<u64>,
    pub restarts: u32,
    pub last_error: Option<String>,
    pub log_path: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct DiagResult {
    pub ok: bool,
    /// HTTP 状态码（仅 WebDAV）；TCP 探测时为 None
    pub code: Option<u16>,
    /// 面向用户的中文结论
    pub message: String,
    /// 实际请求的地址，便于排查"到底打到了哪个端口/路径"
    pub url: String,
    /// 原始响应片段（服务器报错细节）
    pub detail: String,
}

impl DiagResult {
    pub fn fail(message: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            ok: false,
            code: None,
            message: message.into(),
            url: url.into(),
            detail: String::new(),
        }
    }
}

/// 新连接表单（含明文密码，只在内存里存活到写入 rclone.conf 为止）
#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct ConnectionInput {
    pub id: Option<String>,
    pub name: String,
    pub protocol: Protocol,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub user: String,
    /// 新建时必填；编辑时留空表示"沿用旧密码"
    pub password: String,
    pub target: String,
    pub cache_dir: String,
    pub ignore_cert: bool,
    pub read_only: bool,
    pub network_mode: bool,
    pub bwlimit: String,
    pub cache_max_size: String,
    pub proxy: String,
    pub reconnect: bool,
    pub auto_mount: bool,
    pub pinned: bool,
}

impl Default for ConnectionInput {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            protocol: Protocol::Smb,
            host: String::new(),
            port: None,
            path: String::new(),
            user: String::new(),
            password: String::new(),
            target: String::new(),
            cache_dir: String::new(),
            ignore_cert: false,
            read_only: false,
            network_mode: false,
            bwlimit: String::new(),
            cache_max_size: "10G".into(),
            proxy: String::new(),
            reconnect: true,
            auto_mount: false,
            pinned: false,
        }
    }
}
