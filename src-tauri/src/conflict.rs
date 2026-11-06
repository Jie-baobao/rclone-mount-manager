//! 挂载前的位置冲突检测。
//!
//! 只负责「发现」：盘符或挂载点被系统、U 盘、别的软件占了，或者被另一条连接
//! 配了同一个位置。改哪个位置由用户在弹窗里挑——这里刻意不做自动顺延。
//! 盘符一改，用户桌面上的快捷方式、批处理脚本里的 `Z:` 就全失效了，
//! 悄悄换个名字比直接失败更难受。

use crate::model::Connection;
use crate::rclone;
use crate::store;
use tauri::AppHandle;

/// 前端靠这个前缀认出「这不是普通错误，是要弹窗让用户换位置」。
pub const TAG: &str = "rmm-conflict:";

#[derive(serde::Serialize, Clone, Debug)]
pub struct Conflict {
    pub id: String,
    pub name: String,
    /// 现在配置的位置（被占用的那个）
    pub target: String,
    /// "occupied"（系统或其他程序占了） / "other-connection"（另一条连接配了同一位置）
    pub reason: String,
    /// reason=other-connection 时占用者的连接名
    pub owner: String,
    /// 可以改用的候选位置（已排除系统占用的、其他连接配置里的）
    pub candidates: Vec<String>,
}

/// 把冲突列表塞进一条 Err，让前端能解析出结构化数据。
pub fn payload(list: &[Conflict]) -> String {
    let json = serde_json::to_string(list).unwrap_or_else(|_| "[]".into());
    format!("{TAG}{json}")
}

/// 给一批「准备挂载」的连接做预检。调用方负责先滤掉已挂载的——
/// 自己正挂在原位不算冲突。
pub fn scan(app: &AppHandle, conns: &[Connection]) -> Vec<Conflict> {
    let all = store::load_connections(app);
    conns.iter().filter_map(|c| one(app, &all, c)).collect()
}

/// 单条连接的冲突判定。先看配置层（另一条连接占了同一位置），再看系统层。
fn one(app: &AppHandle, all: &[Connection], conn: &Connection) -> Option<Conflict> {
    let t = conn.target.trim();
    if t.is_empty() {
        return None;
    }
    let base = |reason: &str, owner: String| Conflict {
        id: conn.id.clone(),
        name: conn.name.clone(),
        target: t.to_string(),
        reason: reason.to_string(),
        owner,
        candidates: candidates(app, conn),
    };

    if let Some(other) = all
        .iter()
        .find(|c| c.id != conn.id && c.target.trim().eq_ignore_ascii_case(t))
    {
        return Some(base("other-connection", other.name.clone()));
    }
    if rclone::target_taken(t) {
        return Some(base("occupied", String::new()));
    }
    None
}

/// 候选位置：Windows 从 Z 往前找没被分配、也没被别的连接配过的盘符；
/// 类 Unix 在同一个父目录下给 `名字-2`…`名字-7`（都是还不存在的目录）。
/// 只给建议，用户可以在弹窗里手动改成别的。
pub fn candidates(app: &AppHandle, conn: &Connection) -> Vec<String> {
    let used: Vec<String> = store::load_connections(app)
        .into_iter()
        .filter(|c| c.id != conn.id)
        .map(|c| c.target.trim().to_ascii_uppercase())
        .collect();
    let mut out = Vec::new();

    #[cfg(windows)]
    {
        // 跳过 A/B（软驱）和 C（系统盘）；倒序取，越靠后的盘符越像外挂盘
        for code in (b'D'..=b'Z').rev() {
            let t = format!("{}:", code as char);
            if used.iter().any(|u| u == &t) || rclone::target_taken(&t) {
                continue;
            }
            out.push(t);
            if out.len() >= 6 {
                break;
            }
        }
    }

    #[cfg(not(windows))]
    {
        let (dir, stem) = split_target(conn.target.trim());
        for i in 2..=7 {
            let cand = format!("{dir}/{stem}-{i}");
            if used.iter().any(|u| u == &cand.to_ascii_uppercase()) {
                continue;
            }
            // 目录已存在说明上一轮可能留了东西，别往那儿挂
            if std::path::Path::new(&cand).exists() {
                continue;
            }
            out.push(cand);
        }
    }

    out
}

#[cfg(not(windows))]
fn split_target(t: &str) -> (String, String) {
    let t = t.trim_end_matches('/');
    match t.rfind('/') {
        Some(i) if i > 0 => (t[..i].to_string(), t[i + 1..].to_string()),
        Some(_) => ("/".into(), t[1..].to_string()),
        None => ("/".into(), t.to_string()),
    }
}
