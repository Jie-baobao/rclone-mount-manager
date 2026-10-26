//! 应用设置（settings.json）：语言、启动行为、探测规则、日志策略。
//! 与 connections.json 分开存放，互不影响；升级重装都不会动用户配置目录。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// system / zh-CN / zh-TW / en / ja / ko
    pub language: String,
    /// 开机自启动后不弹主界面（只在系统托盘里待着）
    pub silent_start: bool,
    /// 探测间隔（秒）
    pub probe_interval_secs: u64,
    /// 单次探测超过该毫秒数 → 黄（网络波动）
    pub latency_threshold_ms: u64,
    /// 连续失败达到该次数 → 黄
    pub warn_fails: u32,
    /// 连续失败达到该次数 → 红（连接异常）
    pub error_fails: u32,
    /// 单个连接日志文件上限（MB），超出自动裁剪
    pub log_max_mb: u64,
    /// 日志保留天数，超期自动清理
    pub log_retention_days: u32,
    /// 连接列表排序：default（添加顺序）/ name / status / group
    pub sort_by: String,
    /// 排序方向：asc / desc（仅对 name / status / group 有意义）
    pub sort_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "system".into(),
            silent_start: true,
            probe_interval_secs: 15,
            latency_threshold_ms: 1200,
            warn_fails: 2,
            error_fails: 3,
            log_max_mb: 5,
            log_retention_days: 30,
            sort_by: "default".into(),
            sort_dir: "asc".into(),
        }
    }
}

impl Settings {
    pub fn validate(mut self) -> Result<Self, String> {
        self.probe_interval_secs = self.probe_interval_secs.clamp(5, 3600);
        self.latency_threshold_ms = self.latency_threshold_ms.clamp(100, 60_000);
        self.warn_fails = self.warn_fails.clamp(1, 99);
        self.error_fails = self.error_fails.max(self.warn_fails).clamp(1, 99);
        self.log_max_mb = self.log_max_mb.clamp(1, 500);
        self.log_retention_days = self.log_retention_days.clamp(1, 3650);
        if !["default", "name", "status", "group"].contains(&self.sort_by.as_str()) {
            self.sort_by = "default".into();
        }
        if self.sort_dir != "desc" {
            self.sort_dir = "asc".into();
        }
        Ok(self)
    }
}

pub fn path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(super::store::config_dir(app)?.join("settings.json"))
}

/// 读取设置；文件缺失或损坏时返回默认值，保证应用永远能启动。
pub fn load(app: &AppHandle) -> Settings {
    let Ok(p) = path(app) else {
        return Settings::default();
    };
    match fs::read_to_string(&p) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let p = path(app)?;
    let text = serde_json::to_string_pretty(s).map_err(|e| format!("序列化失败：{e}"))?;
    super::store::write_atomic(&p, &text)
}
