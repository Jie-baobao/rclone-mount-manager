//! Linux 文件管理器侧栏书签。
//!
//! 为什么需要这个：FUSE 挂载不是块设备，而文件管理器左侧的「磁盘 / 设备」列表是
//! 从 udisks 取块设备的，所以挂载点怎么都不会出现在那里（Windows 的 WinFsp 会向系统
//! 注册卷，才有盘符可显示）。要让它「看得见」，只能塞一条书签。各家书签存的地方不一样：
//!
//!   - `~/.config/gtk-3.0/bookmarks`（一行 `file:///path 名字`）：GNOME Nautilus、XFCE Thunar、
//!     Cinnamon Nemo、MATE Caja、UKUI Peony，以及老版 LXQt PCManFM-Qt 都读它。
//!   - `~/.local/share/libfm-qt/bookmarks`（格式同上）：libfm-qt 2.1（LXQt 2.x）起改用这个文件，
//!     不再读 GTK 文件。**只在它已存在时写**——它不存在说明用户还没打开过 PCManFM-Qt，
//!     贸然创建会让 libfm-qt 忽略 GTK 书签迁移，把用户原有书签"顶掉"。
//!   - `~/.config/deepin/dde-file-manager.json`（UOS / deepin V20）和
//!     `~/.config/deepin/dde-file-manager/dde-file-manager.json`（deepin V23 起）：
//!     deepin 文件管理器真正读的书签表，`BookMark.Items` 里存 locateUrl（路径的 base64）、
//!     url、name，侧栏顺序另记在扁平键 `SideBar/ItemOrder`。**文件不存在或解析失败时不动它**，
//!     避免把用户配置写坏。
//!   - `~/.local/share/user-places.xbel`（XBEL XML）：KDE Plasma 5 / 6 的 Dolphin「位置」面板。
//!     同样**只在已存在时写**——Plasma 第一次跑 Dolphin 会往里填默认位置，提前创建会顶掉它们。
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
    sync_lines(t, label, mounted);
    sync_deepin(t, label, mounted);
    sync_kde(t, label, mounted);
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
fn data_dir() -> Option<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_DATA_HOME") {
        let p = PathBuf::from(x);
        if p.is_absolute() {
            return Some(p);
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
}

/// 一行 `file:///path 名字` 格式的书签文件（GTK 系与 libfm-qt 共用这个格式）
#[cfg(target_os = "linux")]
fn sync_lines_file(path: &PathBuf, target: &str, label: &str, mounted: bool) {
    let uri = format!("file://{}", encode_path(target));
    let mut lines: Vec<String> = std::fs::read_to_string(path)
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
    let _ = std::fs::write(path, body);
}

#[cfg(target_os = "linux")]
fn sync_lines(t: &str, label: &str, mounted: bool) {
    // GTK 系标准文件：GNOME / XFCE / Cinnamon / MATE / UKUI / 老 LXQt 都读
    if let Some(path) = config_dir().map(|d| d.join("gtk-3.0").join("bookmarks")) {
        sync_lines_file(&path, t, label, mounted);
    }
    // libfm-qt 2.1（LXQt 2.x）起改用自己的文件，格式相同但不再读 GTK 文件；
    // 只在它已存在时写——不存在时创建会让 libfm-qt 跳过 GTK 书签迁移，顶掉用户原有书签。
    if let Some(path) = data_dir().map(|d| d.join("libfm-qt").join("bookmarks")) {
        if path.exists() {
            sync_lines_file(&path, t, label, mounted);
        }
    }
}

#[cfg(target_os = "linux")]
fn sync_deepin(target: &str, label: &str, mounted: bool) {
    use serde_json::Value;

    // deepin / UOS 的配置文件有两个可能位置：老版（UOS / deepin V20）直接放在
    // ~/.config/deepin/ 下，新版（deepin V23 起）挪进了 dde-file-manager 子目录。
    // 都不存在 = 不是 deepin / UOS，或用户还没打开过文件管理器。不主动创建，
    // 免得写出一份结构不全的配置把它顶坏；存在的每个都同步一遍。
    let Some(dir) = config_dir() else {
        return;
    };
    let candidates = [
        dir.join("deepin").join("dde-file-manager.json"),
        dir.join("deepin")
            .join("dde-file-manager")
            .join("dde-file-manager.json"),
    ];
    for path in candidates.into_iter().filter(|p| p.exists()) {
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut root) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if !root.is_object() {
            continue;
        }
        if sync_deepin_file(&path, &mut root, target, label, mounted) {
            // 改的是别人的配置文件，先留一份 .bak
            let _ = std::fs::copy(&path, path.with_extension("json.bak"));
            if let Ok(text) = serde_json::to_string_pretty(&root) {
                let _ = std::fs::write(&path, text);
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn sync_deepin_file(
    path: &std::path::Path,
    root: &mut serde_json::Value,
    target: &str,
    label: &str,
    mounted: bool,
) -> bool {
    use serde_json::{json, Value};
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
        return false;
    }
    true
}

/// KDE Plasma 5 / 6 的 Dolphin「位置」面板：`~/.local/share/user-places.xbel`（XBEL XML）。
/// 只在文件已存在时编辑——Plasma 第一次跑 Dolphin 会把默认位置（文档 / 下载等）写进去，
/// 提前创建一个空壳会把它们顶掉。改前备份 .bak。
#[cfg(target_os = "linux")]
fn sync_kde(target: &str, label: &str, mounted: bool) {
    let Some(path) = data_dir().map(|d| d.join("user-places.xbel")) else {
        return;
    };
    if !path.exists() {
        return;
    }
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    if !raw.contains("<xbel") {
        return; // 格式不认识就不动
    }
    let url = format!("file://{}", encode_path(target));
    let has = raw.contains(&format!("href=\"{url}\""));
    let mut out = raw;
    if mounted && !has {
        // 标题按 XML 转义；图标用 folder-remote（网络存储的常规图标）
        let title = label
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let entry = format!(
            " <bookmark href=\"{url}\">\n  <title>{title}</title>\n  <info>\n   <metadata owner=\"http://www.kde.org\">\n    <bookmark:icon name=\"folder-remote\"/>\n   </metadata>\n  </info>\n </bookmark>\n"
        );
        let Some(pos) = out.rfind("</xbel>") else { return };
        out.insert_str(pos, &entry);
    } else if !mounted && has {
        // 找到 href 所在的 <bookmark ...> ... </bookmark> 块整体删掉
        let Some(hpos) = out.find(&format!("href=\"{url}\"")) else { return };
        let Some(start) = out[..hpos].rfind("<bookmark") else { return };
        let Some(end_rel) = out[hpos..].find("</bookmark>") else { return };
        let end = hpos + end_rel + "</bookmark>".len();
        // 把条目所在行的行首缩进/换行一并吃掉，避免残留空行
        let start = out[..start].rfind('\n').unwrap_or(start);
        out.replace_range(start..end, "");
    } else {
        return; // 状态没变化
    }
    let _ = std::fs::copy(&path, path.with_extension("xbel.bak"));
    let _ = std::fs::write(&path, out);
}
