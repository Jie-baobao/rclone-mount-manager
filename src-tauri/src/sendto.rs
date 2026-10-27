//! Windows「发送到」（SendTo）菜单集成，以及「把本地文件复制/移动到挂载盘」的传输实现。
//!
//! 几个刻意的设计约束（都踩过坑）：
//!
//! 1. `.lnk` **只**放在 `%APPDATA%\Microsoft\Windows\SendTo`，这是菜单项本身；
//!    用户点菜单后资源管理器把**真实文件路径**作为参数传给我们，复制到目标盘的是
//!    字节级的文件，目标盘里不会出现任何快捷方式。
//! 2. 盘符带冒号，不能出现在文件名里，所以菜单文字写成 `[组] 名称 (Z盘)`，
//!    真正的盘符走快捷方式的命令行参数 `--sendto "Z:"`。
//! 3. 生成快捷方式用系统自带的 PowerShell + WScript.Shell（零额外依赖）。
//!    脚本以 UTF-16LE + Base64 的 `-EncodedCommand` 传入，彻底绕开控制台代码页，
//!    中文路径不会被转成问号。
//! 4. 菜单项只在连接**真正挂载**之后出现，卸载/删除/关闭功能时同步清理，
//!    避免用户点到一个已经不存在的盘。
//! 5. 传输本身与平台无关（纯 std::fs），将来做拖拽发送可以直接复用。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------- 数据结构

/// 一次「发送到」请求：目标盘符 + 资源管理器传来的文件路径
#[derive(Serialize, Clone, Debug, Default)]
pub struct SendRequest {
    pub target: String,
    pub files: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
}

/// 传输进度（前端画进度条）
#[derive(Serialize, Clone, Debug, Default)]
pub struct Progress {
    /// planning（正在统计）/ copying / done
    pub phase: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_files: u64,
    pub total_files: u64,
    /// 正在传的文件名，给用户一个「确实在动」的反馈
    pub current: String,
}

/// 传输结果。`ok` 只在「没有任何失败」时为 true；跳过（同名已存在）不算失败。
#[derive(Serialize, Clone, Debug, Default)]
pub struct Report {
    pub ok: bool,
    pub mode: String,
    pub dest: String,
    pub bytes: u64,
    pub copied: u64,
    pub skipped: u64,
    pub failed: u64,
    /// 跳过/失败/其它提示的明细，最多保留 40 条
    pub details: Vec<String>,
    /// 整体性错误（目标不存在、统计失败等）
    pub error: String,
}

/// 传输任务
pub struct Task {
    pub sources: Vec<String>,
    pub dest: String,
    pub target: String,
    /// copy / move
    pub mode: String,
}

/// sendto.json：菜单项对账表 + 每个盘最近用过的目标目录
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct Book {
    /// 菜单文件名（含 .lnk） → 生成时的签名 `label|target`
    menus: HashMap<String, String>,
    /// 盘符 → 最近发送到的目录（新的在前，最多 6 条）
    recent: HashMap<String, Vec<String>>,
}

static RUNNING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------- 书签文件

fn book_path(app: &AppHandle) -> Option<PathBuf> {
    crate::store::config_dir(app).ok().map(|d| d.join("sendto.json"))
}

fn load_book(app: &AppHandle) -> Book {
    let Some(p) = book_path(app) else {
        return Book::default();
    };
    match fs::read_to_string(&p) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Book::default(),
    }
}

fn save_book(app: &AppHandle, book: &Book) {
    let Some(p) = book_path(app) else { return };
    if let Ok(text) = serde_json::to_string_pretty(book) {
        let _ = crate::store::write_atomic(&p, &text);
    }
}

/// 某个盘最近用过的目标目录
pub fn recent(app: &AppHandle, target: &str) -> Vec<String> {
    load_book(app).recent.get(target).cloned().unwrap_or_default()
}

fn remember(app: &AppHandle, target: &str, dest: &str) {
    if target.is_empty() || dest.is_empty() {
        return;
    }
    let mut book = load_book(app);
    let list = book.recent.entry(target.to_string()).or_default();
    list.retain(|p| p != dest);
    list.insert(0, dest.to_string());
    list.truncate(6);
    save_book(app, &book);
}

// ---------------------------------------------------------------- 菜单同步

/// 把「当前已挂载的连接」同步成 SendTo 菜单项。
/// `mounted` 每项是 (连接名, 分组, 挂载目标)，只传真正挂着的那些。
pub fn sync(app: &AppHandle, mounted: &[(String, String, String)]) {
    #[cfg(windows)]
    {
        let mut book = load_book(app);
        let cfg = crate::settings::load(app);
        if !cfg.sendto_enabled {
            // 功能关掉：删掉我们创建过的所有菜单项，最近目录留着（重新打开还能用）
            clear_menus(&mut book);
            save_book(app, &book);
            return;
        }

        let Some(dir) = sendto_dir() else {
            return;
        };

        let mut want: HashMap<String, String> = HashMap::new();
        for (name, group, target) in mounted {
            let Some(drive) = drive_label(target) else {
                continue;
            };
            let label = menu_label(name, group, &drive);
            let file = format!("{}.lnk", sanitize(&label));
            // 用不可见分隔符拼签名：连接名里带 '|' 时按 '|' 切会切歪
            want.insert(file, format!("{label}\u{1}{target}"));
        }

        // 1) 删掉不再需要的（连接卸载了、被删了、改名了留下的旧文件）
        let stale: Vec<String> = book
            .menus
            .keys()
            .filter(|f| !want.contains_key(*f))
            .cloned()
            .collect();
        for file in stale {
            let _ = fs::remove_file(dir.join(&file));
            book.menus.remove(&file);
        }

        // 2) 补上缺的，以及内容变了要重建的
        for (file, sig) in want {
            let up_to_date = book.menus.get(&file).map(|s| s.as_str()) == Some(sig.as_str())
                && dir.join(&file).exists();
            if up_to_date {
                continue;
            }
            let Some((label, target)) = sig.split_once('\u{1}') else {
                continue;
            };
            if make_lnk(&dir.join(&file), label, target).is_ok() {
                book.menus.insert(file, sig);
            }
        }

        save_book(app, &book);
    }

    #[cfg(not(windows))]
    {
        // 「发送到」是 Windows 资源管理器独有的，其它平台什么都不做
        let _ = (app, mounted);
    }
}

#[cfg(windows)]
fn clear_menus(book: &mut Book) {
    if let Some(dir) = sendto_dir() {
        for file in book.menus.keys() {
            let _ = fs::remove_file(dir.join(file));
        }
    }
    book.menus.clear();
}

#[cfg(windows)]
fn sendto_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let dir = PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("SendTo");
    if dir.is_dir() {
        Some(dir)
    } else {
        None
    }
}

/// `Z:` → `Z盘`。不是盘符挂载点（Linux 的 /mnt/nas）返回 None，这类不生成菜单项。
#[allow(dead_code)] // 非 Windows 平台下这个函数没有调用点
fn drive_label(target: &str) -> Option<String> {
    let t = target.trim();
    let b = t.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        Some(format!("{}盘", (b[0] as char).to_ascii_uppercase()))
    } else {
        None
    }
}

/// `[工作] NAS-视频 (Z盘)`：一眼看出「哪个组、哪个连接、哪个盘」
#[allow(dead_code)]
fn menu_label(name: &str, group: &str, drive: &str) -> String {
    let g = group.trim();
    let base = if g.is_empty() {
        name.trim().to_string()
    } else {
        format!("[{g}] {}", name.trim())
    };
    let text = format!("{base} ({drive})");
    // 「发送到」菜单宽度有限，太长会被系统截断得没有辨识度，这里自己先截
    if text.chars().count() > 48 {
        let cut: String = text.chars().take(47).collect();
        format!("{cut}…")
    } else {
        text
    }
}

#[allow(dead_code)]
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

#[cfg(windows)]
fn make_lnk(lnk: &Path, label: &str, target: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("取程序路径失败：{e}"))?;
    let exe_s = exe.to_string_lossy().to_string();
    let dir_s = exe
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut({});\
         $s.TargetPath={};$s.Arguments={};$s.WorkingDirectory={};\
         $s.IconLocation={};$s.Description={};$s.Save()",
        ps_quote(&lnk.to_string_lossy()),
        ps_quote(&exe_s),
        ps_quote(&format!("--sendto \"{target}\"")),
        ps_quote(&dir_s),
        ps_quote(&format!("{exe_s},0")),
        ps_quote(label),
    );
    ps_run(&script)
}

#[cfg(windows)]
fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

#[cfg(windows)]
fn ps_run(script: &str) -> Result<(), String> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    use std::os::windows::process::CommandExt;

    // -EncodedCommand 收的是 UTF-16LE 的 Base64：中文路径不经控制台代码页，不会变问号
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let encoded = B64.encode(bytes);

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            encoded.as_str(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("调用 PowerShell 失败：{e}"))?;
    if out.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&out.stderr)
        .trim()
        .chars()
        .take(400)
        .collect())
}

// ---------------------------------------------------------------- 目录浏览

/// 只列子目录：这一步是「选往哪儿放」，文件只会干扰。
pub fn list_dirs(path: &str) -> Result<Vec<DirEntry>, String> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(format!("目录不存在：{path}"));
    }
    let rd = fs::read_dir(p).map_err(|e| format!("无法读取目录：{e}"))?;
    let mut out = Vec::new();
    for item in rd.flatten() {
        // 符号链接不列：跟随链接可能绕回源目录，复制时变成无限递归
        let Ok(ft) = item.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        out.push(DirEntry {
            name: item.file_name().to_string_lossy().to_string(),
            path: item.path().to_string_lossy().to_string(),
        });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

// ---------------------------------------------------------------- 传输

struct Plan {
    /// (源文件, 目标文件)
    files: Vec<(PathBuf, PathBuf)>,
    /// 需要创建的目录（含空目录）
    dirs: Vec<PathBuf>,
    total_bytes: u64,
    skipped: u64,
    details: Vec<String>,
}

fn build_plan(sources: &[String], dest: &Path) -> Plan {
    let mut plan = Plan {
        files: Vec::new(),
        dirs: Vec::new(),
        total_bytes: 0,
        skipped: 0,
        details: Vec::new(),
    };
    for src in sources {
        let src_path = PathBuf::from(src);
        let Some(name) = src_path.file_name().map(|s| s.to_os_string()) else {
            plan.details.push(format!("跳过（无法识别的路径）：{src}"));
            continue;
        };
        let target = dest.join(&name);
        let Ok(meta) = fs::symlink_metadata(&src_path) else {
            plan.details.push(format!("跳过（源文件已不存在）：{src}"));
            continue;
        };
        if meta.is_dir() {
            plan.dirs.push(target.clone());
            walk_dir(&src_path, &target, &mut plan, 0);
        } else {
            if target.exists() {
                plan.skipped += 1;
                plan.details
                    .push(format!("跳过（目标已有同名文件）：{}", target.display()));
                continue;
            }
            plan.total_bytes += meta.len();
            plan.files.push((src_path, target));
        }
    }
    plan
}

fn walk_dir(src: &Path, dst: &Path, plan: &mut Plan, depth: usize) {
    if depth > 64 {
        plan.details
            .push(format!("跳过（目录层级过深）：{}", src.display()));
        return;
    }
    let Ok(rd) = fs::read_dir(src) else {
        plan.details
            .push(format!("跳过（无法读取目录）：{}", src.display()));
        return;
    };
    for item in rd.flatten() {
        let Ok(ft) = item.file_type() else { continue };
        let s = item.path();
        let d = dst.join(item.file_name());
        if ft.is_dir() {
            plan.dirs.push(d.clone());
            walk_dir(&s, &d, plan, depth + 1);
        } else if ft.is_file() {
            if d.exists() {
                plan.skipped += 1;
                if plan.details.len() < 40 {
                    plan.details
                        .push(format!("跳过（目标已有同名文件）：{}", d.display()));
                }
                continue;
            }
            if let Ok(meta) = item.metadata() {
                plan.total_bytes += meta.len();
            }
            plan.files.push((s, d));
        }
        // 符号链接直接忽略：跨盘链接指回源目录时会无限递归
    }
}

/// 分块复制并回调进度。不用 `fs::copy` 是因为它没法报进度，
/// 大文件会看起来像卡死了。
fn copy_file<F: FnMut(u64)>(src: &Path, dst: &Path, on_chunk: &mut F) -> std::io::Result<()> {
    let mut input = fs::File::open(src)?;
    let mut output = fs::File::create(dst)?;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if CANCEL.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "已取消",
            ));
        }
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
        on_chunk(n as u64);
    }
    output.flush()?;
    // 网络盘上 flush 之后数据才算真的过去；sync_all 失败不影响结果判断
    let _ = output.sync_all();
    Ok(())
}

/// 起一个后台线程执行传输。立刻返回，进度和结果都走事件。
pub fn run(app: AppHandle, task: Task) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        let _ = app.emit(
            "sendto-done",
            Report {
                ok: false,
                error: "已有一次传输在进行中".into(),
                ..Default::default()
            },
        );
        return;
    }
    CANCEL.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let report = execute(&app, &task);
        RUNNING.store(false, Ordering::SeqCst);
        let _ = app.emit("sendto-done", report);
    });
}

fn execute(app: &AppHandle, task: &Task) -> Report {
    let dest = PathBuf::from(&task.dest);
    let mut report = Report {
        mode: task.mode.clone(),
        dest: task.dest.clone(),
        ..Default::default()
    };

    if !dest.is_dir() {
        report.error = format!("目标目录不存在：{}", task.dest);
        return report;
    }

    let _ = app.emit(
        "sendto-progress",
        Progress {
            phase: "planning".into(),
            ..Default::default()
        },
    );

    let plan = build_plan(&task.sources, &dest);
    report.skipped = plan.skipped;
    report.bytes = plan.total_bytes;
    report.details = plan.details.clone();
    let total_files = plan.files.len() as u64;

    for d in &plan.dirs {
        if let Err(e) = fs::create_dir_all(d) {
            report.error = format!("创建目录失败：{}（{e}）", d.display());
            return report;
        }
    }

    let mut done_bytes = 0u64;
    let mut done_files = 0u64;
    let mut cancelled = false;

    for (src, dst) in &plan.files {
        if CANCEL.load(Ordering::SeqCst) {
            cancelled = true;
            break;
        }
        if let Some(parent) = dst.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut last_emit = std::time::Instant::now();
        let res = copy_file(src, dst, &mut |n| {
            done_bytes += n;
            if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
                last_emit = std::time::Instant::now();
                let _ = app.emit(
                    "sendto-progress",
                    Progress {
                        phase: "copying".into(),
                        done_bytes,
                        total_bytes: plan.total_bytes,
                        done_files,
                        total_files,
                        current: src
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    },
                );
            }
        });

        match res {
            Ok(()) => {
                done_files += 1;
                if task.mode == "move" {
                    if let Err(e) = fs::remove_file(src) {
                        report
                            .details
                            .push(format!("已复制但删不掉源文件：{}（{e}）", src.display()));
                    }
                }
            }
            Err(e) => {
                // 目标盘上别留半截文件
                let _ = fs::remove_file(dst);
                report.failed += 1;
                if report.details.len() < 40 {
                    report
                        .details
                        .push(format!("失败：{}（{e}）", src.display()));
                }
                // 失败的文件也算「走过了」，否则进度条永远差一截
                done_bytes += fs::metadata(src).map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    // 移动模式：只有「一个都没跳过、一个都没失败」时才删源目录。
    // 只要有一个同名文件被跳过，删源就等于丢数据。
    if task.mode == "move" && !cancelled && report.failed == 0 && report.skipped == 0 {
        for src in &task.sources {
            let p = PathBuf::from(src);
            if p.is_dir() {
                let _ = fs::remove_dir_all(&p);
            }
        }
    }

    report.copied = done_files;
    report.ok = report.failed == 0 && !cancelled && report.error.is_empty();
    if cancelled {
        report.error = "已取消".into();
    }

    let _ = app.emit(
        "sendto-progress",
        Progress {
            phase: "done".into(),
            done_bytes: plan.total_bytes,
            total_bytes: plan.total_bytes,
            done_files,
            total_files,
            current: String::new(),
        },
    );

    if report.ok {
        remember(app, &task.target, &task.dest);
    }
    report
}
