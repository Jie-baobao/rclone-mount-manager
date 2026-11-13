use crate::model::{Connection, TrafficDay};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// 应用配置目录：
/// - Windows: %APPDATA%\io.github.rclonemount.manager
/// - macOS:   ~/Library/Application Support/io.github.rclonemount.manager
/// - Linux:   ~/.config/io.github.rclonemount.manager
pub fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法定位配置目录：{e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败：{e}"))?;
    Ok(dir)
}

pub fn connections_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("connections.json"))
}

pub fn rclone_conf_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("rclone.conf"))
}

pub fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = config_dir(app)?.join("logs");
    fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败：{e}"))?;
    Ok(dir)
}

/// 读原始连接列表（不查密码状态）。saved_password_ids 内部用它，
/// 避免 load_connections ↔ saved_password_ids 互相递归。
pub fn load_connections_raw(app: &AppHandle) -> Vec<Connection> {
    let Ok(path) = connections_path(app) else {
        return Vec::new();
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// 读取连接列表；文件不存在或损坏时返回空列表而不是报错，
/// 避免一份坏配置把整个应用卡死。
pub fn load_connections(app: &AppHandle) -> Vec<Connection> {
    let mut list = load_connections_raw(app);
    // conf 里到底存没存过密码：表单的「已保存 / 必填」提示靠它。
    // 每次读列表都现查一遍（conf 就几 KB），别在保存时落盘——那会变陈旧。
    let saved = crate::rclone::saved_password_ids(app);
    for c in &mut list {
        c.has_password = saved.contains(&c.id);
    }
    // 加密目录那把密码同理（只有勾了加密的连接才需要）
    crate::rclone::fill_crypt_state(app, &mut list);
    list
}

/// 原子写入：先写临时文件，再替换目标，并保留一份 .bak。
/// 断电或被杀进程时不会留下半截 JSON。
pub fn save_connections(app: &AppHandle, list: &[Connection]) -> Result<(), String> {
    let path = connections_path(app)?;
    let text = serde_json::to_string_pretty(list).map_err(|e| format!("序列化失败：{e}"))?;
    write_atomic(&path, &text)
}

pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(|e| format!("写入临时文件失败：{e}"))?;
    if path.exists() {
        let bak = path.with_extension("json.bak");
        let _ = fs::copy(path, bak);
    }
    fs::rename(&tmp, path).map_err(|e| format!("替换配置文件失败：{e}"))?;
    Ok(())
}

/// 生成本应用的日志文件路径（每个连接一份）
pub fn mount_log_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    Ok(log_dir(app)?.join(format!("mount-{id}.log")))
}

pub fn traffic_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("traffic.json"))
}

/// 读取上传/下载账本（按连接 id 索引）。文件不存在或损坏时返回空表：
/// 流量只是参考数据，不值得因为它报错拦住启动。
pub fn load_traffic(app: &AppHandle) -> HashMap<String, TrafficDay> {
    let Ok(path) = traffic_path(app) else {
        return HashMap::new();
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

pub fn save_traffic(app: &AppHandle, map: &HashMap<String, TrafficDay>) -> Result<(), String> {
    let path = traffic_path(app)?;
    let text = serde_json::to_string_pretty(map).map_err(|e| format!("序列化流量账本失败：{e}"))?;
    write_atomic(&path, &text)
}

// ---------------------------------------------------------------- 流量历史

/// 近 7 天的每日流量合计：日期 → (上行字节, 下行字节)。
/// 与 traffic.json（当天计数器 + 累计值）分开存：这是给折线图用的账本，
/// 每天一行，过期就剪掉，不会无限长。
pub type TrafficHistory = HashMap<String, (u64, u64)>;

pub fn traffic_history_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("traffic_history.json"))
}

pub fn load_traffic_history(app: &AppHandle) -> TrafficHistory {
    let Ok(path) = traffic_history_path(app) else {
        return HashMap::new();
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

pub fn save_traffic_history(app: &AppHandle, map: &TrafficHistory) -> Result<(), String> {
    let path = traffic_history_path(app)?;
    let text =
        serde_json::to_string_pretty(map).map_err(|e| format!("序列化流量历史失败：{e}"))?;
    write_atomic(&path, &text)
}

// ---------------------------------------------------------------- 探测历史

/// 探测历史：连接 id → [(时间戳秒, 延迟微秒)]，0 表示这次探测失败。
/// 只留最近 7 天，够画「这几天的稳定性」，也不会无限长大。
pub type HistoryMap = HashMap<String, Vec<(i64, u64)>>;

pub fn history_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("history.json"))
}

pub fn load_history(app: &AppHandle) -> HistoryMap {
    let Ok(path) = history_path(app) else {
        return HashMap::new();
    };
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

pub fn save_history(app: &AppHandle, map: &HistoryMap) -> Result<(), String> {
    let path = history_path(app)?;
    let text = serde_json::to_string(map).map_err(|e| format!("序列化探测历史失败：{e}"))?;
    write_atomic(&path, &text)
}
