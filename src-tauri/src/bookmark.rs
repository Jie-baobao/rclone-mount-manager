//! Linux 文件管理器侧栏书签。
//!
//! 为什么需要这个：FUSE 挂载不是块设备，而 deepin 文件管理器左侧的「磁盘列表」是
//! 从 udisks 取块设备的，所以挂载点怎么都不会出现在那里（Windows 的 WinFsp 会向系统
//! 注册卷，才有盘符可显示）。要让它「看得见」，只能塞一条书签。
//!
//! 两处都写：
//!   - `~/.config/deepin/dde-file-manager.json`：deepin / UOS 文件管理器真正读的书签表，
//!     `BookMark.Items` 里存 locateUrl（路径的 base64）、url、name，侧栏顺序另记在
//!     `SideBar/ItemOrder`。**文件不存在或解析失败时不动它**，避免把用户配置写坏。
//!   - `~/.config/gtk-3.0/bookmarks`：GTK 系文件管理器（nautilus 等）读的标准文件，
//!     一行 `file:///path 名字`。
//!
//! 卸载时把对应条目删掉：否则侧栏会留一堆点进去是空目录的书签。

#[cfg(target_os = "linux")]
use std::path::PathBuf;

/// 挂载成功后加书签，卸载后删掉。非 Linux 平台什么也不做。
#[cfg(not(target_os = "linux"))]
#[inline]
pub fn sync(_target: &str, _label: &str, _mounted: bool) {}

#[cfg(target_os = "linux")]
pub fn sync(target: &str, label: &str, mounted: bool) {
    let t = target.trim();
    if t.is_empty() {
        return;
    }
    sync_gtk(t, label, mounted);
    sync_deepin(t, label, mounted);
}

#[cfg(target_os = "linux")]
fn config_dir() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        let p = PathBuf::from(x);
        if p.is_absolute() {
            return Some(p);
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
}

/// file:// 里的路径要转义：空格、中文、`#` 这类字符不转义会让整行被文件管理器忽略。
#[cfg(target_os = "linux")]
fn encode_path(p: &str) -> String {
    let mut out = String::with_capacity(p.len());
    for b in p.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// deepin 的 locateUrl 是路径的 base64（不带 `=` 补位）
#[cfg(target_os = "linux")]
fn path_b64(p: &str) -> String {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    B64.encode(p.as_bytes()).trim_end_matches('=').to_string()
}

#[cfg(target_os = "linux")]
fn sync_gtk(target: &str, label: &str, mounted: bool) {
    let Some(path) = config_dir().map(|d| d.join("gtk-3.0").join("bookmarks")) else {
        return;
    };
    let uri = format!("file://{}", encode_path(target));
    let mut lines: Vec<String> = std::fs::read_to_string(&path)
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let before = lines.len();
    lines.retain(|l| l.split_whitespace().next() != Some(uri.as_str()));
    if mounted {
        lines.push(format!("{uri} {label}"));
    } else if lines.len() == before {
        // 本来就没有，别平白改用户的文件
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let body = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };
    let _ = std::fs::write(&path, body);
}

#[cfg(target_os = "linux")]
fn sync_deepin(target: &str, label: &str, mounted: bool) {
    use serde_json::{json, Value};

    let Some(path) = config_dir().map(|d| d.join("deepin").join("dde-file-manager.json")) else {
        return;
    };
    // 没这个文件 = 不是 deepin / UOS，或用户还没打开过文件管理器。不主动创建，
    // 免得写出一份结构不全的配置把它顶坏。
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(mut root) = serde_json::from_str::<Value>(&raw) else {
        return;
    };
    if !root.is_object() {
        return;
    }
    let url = format!("file://{}", encode_path(target));

    // ---- BookMark.Items ----
    if !root.get("BookMark").map(Value::is_object).unwrap_or(false) {
        root["BookMark"] = json!({ "Items": [] });
    }
    if !root["BookMark"].get("Items").map(Value::is_array).unwrap_or(false) {
        root["BookMark"]["Items"] = json!([]);
    }
    let existed = root["BookMark"]["Items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .any(|it| it.get("url").and_then(Value::as_str) == Some(url.as_str()))
        })
        .unwrap_or(false);

    let mut changed = false;
    if mounted && !existed {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some(items) = root["BookMark"]["Items"].as_array_mut() {
            items.push(json!({
                "locateUrl": path_b64(target),
                "mountPoint": "",
                "name": label,
                "url": url,
                "created": now,
                "lastModified": now,
            }));
            changed = true;
        }
    } else if !mounted && existed {
        if let Some(items) = root["BookMark"]["Items"].as_array_mut() {
            items.retain(|it| it.get("url").and_then(Value::as_str) != Some(url.as_str()));
            changed = true;
        }
    }

    // ---- 侧栏顺序：bookmark:file:///path?#名字 ----
    // 只在它本来就是数组时维护，格式不认识就交给文件管理器自己补。
    let entry = format!("bookmark:{url}?#{label}");
    if let Some(order) = root.get_mut("SideBar/ItemOrder").and_then(Value::as_object_mut) {
        if let Some(list) = order.get_mut("bookmark").and_then(Value::as_array_mut) {
            let has = list.iter().any(|v| v.as_str() == Some(entry.as_str()));
            if mounted && !has {
                list.push(Value::String(entry));
                changed = true;
            } else if !mounted && has {
                list.retain(|v| v.as_str() != Some(entry.as_str()));
                changed = true;
            }
        }
    }

    if !changed {
        return;
    }
    // 改的是别人的配置文件，先留一份 .bak
    let _ = std::fs::copy(&path, path.with_extension("json.bak"));
    if let Ok(text) = serde_json::to_string_pretty(&root) {
        let _ = std::fs::write(&path, text);
    }
}
