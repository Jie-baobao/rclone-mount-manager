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
    /// S3 兼容对象存储（AWS S3 / MinIO / 阿里云 OSS / 腾讯 COS / R2 / B2 / Wasabi…）
    S3,
}

impl Protocol {
    pub fn as_rclone_type(self) -> &'static str {
        match self {
            Protocol::Smb => "smb",
            Protocol::Webdav => "webdav",
            Protocol::Ftp => "ftp",
            Protocol::Sftp => "sftp",
            Protocol::S3 => "s3",
        }
    }

    /// 各协议常用默认端口（WebDAV 标准化端口为 80/443，非标端口可在界面上改）
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Smb => 445,
            Protocol::Webdav => 80,
            Protocol::Ftp => 21,
            Protocol::Sftp => 22,
            // S3 走 HTTPS，端口由 endpoint 决定，这个值只用于表单占位
            Protocol::S3 => 443,
        }
    }

    pub fn is_webdav(self) -> bool {
        matches!(self, Protocol::Webdav)
    }

    pub fn is_s3(self) -> bool {
        matches!(self, Protocol::S3)
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Protocol::Smb => "SMB",
            Protocol::Webdav => "WebDAV",
            Protocol::Ftp => "FTP",
            Protocol::Sftp => "SFTP",
            Protocol::S3 => "S3",
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
    /// S3 兼容：rclone 的 provider 取值（AWS / Minio / Alibaba / TencentCOS…）
    pub s3_provider: String,
    /// S3 兼容：区域，例如 ap-northeast-1
    pub region: String,
    /// S3 兼容：强制 path-style（MinIO 与多数自建网关需要）
    pub s3_path_style: bool,
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
    /// 分组名（可空）。挂载时拼进卷标：`[组名] 名称`，资源管理器按名称排序自然聚组
    pub group: String,
    /// 挂载与探测都走 Windows/macOS 系统代理（默认关）
    pub use_system_proxy: bool,
    /// 探测方式：full=完整（真实列目录）/ light=轻量（TCP 连端口）/ process=仅看挂载进程
    /// 默认 light：不读文件系统，不会唤醒休眠中的硬盘
    pub probe_mode: String,
    /// 追加给 rclone mount 的自定义参数（例如 `--transfers 4 --buffer-size 32M`）。
    /// 只做「按空格/引号切分后追加」，不校验语义；与本应用自己控制的参数冲突时会被拒绝。
    pub extra_args: String,
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
            s3_provider: "Other".into(),
            region: String::new(),
            s3_path_style: false,
            read_only: false,
            network_mode: false,
            bwlimit: String::new(),
            cache_max_size: "10G".into(),
            proxy: String::new(),
            reconnect: true,
            auto_mount: false,
            pinned: false,
            group: String::new(),
            use_system_proxy: false,
            probe_mode: "light".into(),
            extra_args: String::new(),
            created_at: 0,
        }
    }
}

/// 一个连接的上传/下载流量账本。按天记账（`date` 变了就把 up/down 清零），
/// `total_*` 是装机以来的累计值（不随日期清零）。
/// 口径注意：rclone 的 `core/stats` 只给「收发合计」，方向是按「是否有活跃上传」推断的，
/// 详见 `spawn_stats_monitor` 里的注释。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TrafficDay {
    /// 记账日期（本地时区，YYYY-MM-DD）
    pub date: String,
    /// 今天上行（写远端）
    pub up: u64,
    /// 今天下行（从远端读）
    pub down: u64,
    /// 累计上行
    pub total_up: u64,
    /// 累计下行
    pub total_down: u64,
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
    /// healthy（绿）/ slow（黄）/ error（红）/ stopped（灰）
    pub health: String,
    /// 最近一次探测的往返毫秒数
    pub latency_us: Option<u64>,
    /// 最近一次探测时间（unix 秒）
    pub last_check: Option<i64>,
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
    pub s3_provider: String,
    pub region: String,
    pub s3_path_style: bool,
    pub read_only: bool,
    pub network_mode: bool,
    pub bwlimit: String,
    pub cache_max_size: String,
    pub proxy: String,
    pub reconnect: bool,
    pub auto_mount: bool,
    pub pinned: bool,
    pub group: String,
    pub use_system_proxy: bool,
    pub probe_mode: String,
    pub extra_args: String,
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
            s3_provider: "Other".into(),
            region: String::new(),
            s3_path_style: false,
            read_only: false,
            network_mode: false,
            bwlimit: String::new(),
            cache_max_size: "10G".into(),
            proxy: String::new(),
            reconnect: true,
            auto_mount: false,
            pinned: false,
            group: String::new(),
            use_system_proxy: false,
            probe_mode: "light".into(),
            extra_args: String::new(),
        }
    }
}
