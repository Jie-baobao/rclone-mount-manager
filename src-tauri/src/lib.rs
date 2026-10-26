mod autostart;
mod diag;
mod model;
mod rclone;
mod store;

use model::{Connection, ConnectionInput, DiagResult, MountState, Protocol};
use std::collections::HashMap;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State};

/// 一个进行中的挂载会话
pub struct Session {
    child: Child,
    started: Instant,
    restarts: u32,
}

pub struct AppState {
    sessions: Mutex<HashMap<String, Session>>,
    /// 每个连接最近一次失败原因（挂载日志尾部）
    errors: Mutex<HashMap<String, String>>,
    /// 托盘是否创建成功：成功时关闭窗口改为"隐藏到托盘"
    tray_ready: AtomicBool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            errors: Mutex::new(HashMap::new()),
            tray_ready: AtomicBool::new(false),
        }
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

/// 表单 → 连接对象。编辑时沿用旧的 id / 创建时间 / 置顶状态。
fn to_connection(input: &ConnectionInput, existing: Option<&Connection>) -> Connection {
    let port = input
        .port
        .filter(|p| *p > 0)
        .or_else(|| existing.map(|c| c.port))
        .unwrap_or_else(|| input.protocol.default_port());

    Connection {
        id: existing
            .map(|c| c.id.clone())
            .or_else(|| input.id.clone())
            .unwrap_or_else(new_id),
        name: input.name.trim().to_string(),
        protocol: input.protocol,
        host: input.host.trim().to_string(),
        port,
        path: input.path.trim().trim_matches('/').to_string(),
        user: input.user.trim().to_string(),
        target: input.target.trim().to_string(),
        cache_dir: input.cache_dir.trim().to_string(),
        ignore_cert: input.ignore_cert,
        read_only: input.read_only,
        network_mode: input.network_mode,
        bwlimit: input.bwlimit.trim().to_string(),
        cache_max_size: input.cache_max_size.trim().to_string(),
        proxy: input.proxy.trim().to_string(),
        reconnect: input.reconnect,
        auto_mount: input.auto_mount,
        pinned: existing.map(|c| c.pinned).unwrap_or(input.pinned),
        created_at: existing.map(|c| c.created_at).unwrap_or_else(now_secs),
    }
}

/// 启动挂载并把会话登记进状态表
fn start_session(app: &AppHandle, state: &AppState, conn: &Connection, restarts: u32) -> Result<u32, String> {
    let log_path = store::mount_log_path(app, &conn.id)?;
    let _ = std::fs::write(&log_path, "");
    let child = rclone::spawn_mount(app, conn, &log_path)?;
    let pid = child.id();
    state.sessions.lock().unwrap().insert(
        conn.id.clone(),
        Session {
            child,
            started: Instant::now(),
            restarts,
        },
    );
    state.errors.lock().unwrap().remove(&conn.id);
    Ok(pid)
}

// ---------------------------------------------------------------- 命令

#[tauri::command]
fn get_env(app: AppHandle) -> diag::EnvInfo {
    diag::env_info(&app)
}

#[tauri::command]
fn list_connections(app: AppHandle) -> Vec<Connection> {
    let mut list = store::load_connections(&app);
    // 置顶的排前面，其余按创建时间倒序
    list.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    list
}

#[tauri::command]
fn save_connection(app: AppHandle, input: ConnectionInput) -> Result<Connection, String> {
    if input.name.trim().is_empty() {
        return Err("请填写连接名称".into());
    }
    if input.host.trim().is_empty() {
        return Err("请填写服务器地址".into());
    }
    if input.target.trim().is_empty() {
        return Err("请选择盘符或挂载点".into());
    }
    if input.protocol == Protocol::Webdav && input.user.is_empty() {
        return Err("WebDAV 需要填写用户名".into());
    }

    let mut list = store::load_connections(&app);
    let idx = input
        .id
        .as_ref()
        .and_then(|id| list.iter().position(|c| &c.id == id));
    let conn = to_connection(&input, idx.map(|i| &list[i]));

    let plain = if input.password.is_empty() {
        None
    } else {
        Some(input.password.as_str())
    };
    rclone::upsert_remote(&app, &conn, plain)?;

    match idx {
        Some(i) => list[i] = conn.clone(),
        None => list.push(conn.clone()),
    }
    store::save_connections(&app, &list)?;
    let _ = app.emit("connections-changed", ());
    Ok(conn)
}

#[tauri::command]
fn set_pinned(app: AppHandle, id: String, pinned: bool) -> Result<(), String> {
    let mut list = store::load_connections(&app);
    if let Some(c) = list.iter_mut().find(|c| c.id == id) {
        c.pinned = pinned;
        store::save_connections(&app, &list)?;
        Ok(())
    } else {
        Err("连接不存在".into())
    }
}

#[tauri::command]
fn delete_connection(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    if let Some(mut session) = state.sessions.lock().unwrap().remove(&id) {
        let _ = rclone::unmount(&mut session.child, "");
    }
    state.errors.lock().unwrap().remove(&id);
    let mut list = store::load_connections(&app);
    list.retain(|c| c.id != id);
    store::save_connections(&app, &list)?;
    rclone::remove_remote(&app, &id)?;
    let _ = app.emit("connections-changed", ());
    Ok(())
}

#[tauri::command]
fn mount(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<MountState, String> {
    let conn = store::load_connections(&app)
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| "连接不存在".to_string())?;

    // 已挂载就直接返回，避免重复起进程
    if state.sessions.lock().unwrap().contains_key(&id) {
        return Ok(state_snapshot(&app, &state, &conn));
    }

    start_session(&app, &state, &conn, 0)?;

    // 等一小会儿：认证失败 / 驱动缺失会让 rclone 立刻退出，
    // 这里当场把真实报错抛给界面，而不是给一个"假成功"。
    std::thread::sleep(Duration::from_millis(900));
    let alive = {
        let mut map = state.sessions.lock().unwrap();
        match map.get_mut(&id) {
            Some(s) => !matches!(s.child.try_wait(), Ok(Some(_))),
            None => false,
        }
    };
    if !alive {
        state.sessions.lock().unwrap().remove(&id);
        let tail = store::mount_log_path(&app, &id)
            .map(|p| rclone::tail_log(&p, 15))
            .unwrap_or_default();
        state.errors.lock().unwrap().insert(id.clone(), tail.clone());
        let _ = app.emit("state-changed", ());
        return Err(format!("挂载进程已退出。\n\n{tail}"));
    }

    let _ = app.emit("state-changed", ());
    Ok(state_snapshot(&app, &state, &conn))
}

#[tauri::command]
fn unmount(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut session = state
        .sessions
        .lock()
        .unwrap()
        .remove(&id)
        .ok_or_else(|| "该连接当前未挂载".to_string())?;
    let target = store::load_connections(&app)
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.target)
        .unwrap_or_default();
    rclone::unmount(&mut session.child, &target)?;
    state.errors.lock().unwrap().remove(&id);
    let _ = app.emit("state-changed", ());
    Ok(())
}

/// 一键挂载全部未挂载的连接（不阻塞界面，逐个起、结果通过事件回报）
#[tauri::command]
fn mount_all(app: AppHandle, state: State<'_, AppState>) -> Result<usize, String> {
    let list = store::load_connections(&app);
    let mut started = 0;
    for conn in list {
        if state.sessions.lock().unwrap().contains_key(&conn.id) {
            continue;
        }
        if start_session(&app, &state, &conn, 0).is_ok() {
            started += 1;
        }
    }
    let _ = app.emit("state-changed", ());
    Ok(started)
}

#[tauri::command]
fn unmount_all(app: AppHandle, state: State<'_, AppState>) -> Result<usize, String> {
    let drained: Vec<(String, Session)> = state.sessions.lock().unwrap().drain().collect();
    let targets: HashMap<String, String> = store::load_connections(&app)
        .into_iter()
        .map(|c| (c.id, c.target))
        .collect();
    let mut count = 0;
    for (id, mut s) in drained {
        let target = targets.get(&id).cloned().unwrap_or_default();
        let _ = rclone::unmount(&mut s.child, &target);
        count += 1;
    }
    let _ = app.emit("state-changed", ());
    Ok(count)
}

fn state_snapshot(app: &AppHandle, state: &AppState, conn: &Connection) -> MountState {
    let map = state.sessions.lock().unwrap();
    let session = map.get(&conn.id);
    MountState {
        id: conn.id.clone(),
        mounted: session.is_some(),
        pid: session.map(|s| s.child.id()),
        since: session.map(|s| s.started.elapsed().as_secs()),
        restarts: session.map(|s| s.restarts).unwrap_or(0),
        last_error: state.errors.lock().unwrap().get(&conn.id).cloned(),
        log_path: store::mount_log_path(app, &conn.id)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    }
}

#[tauri::command]
fn mount_states(app: AppHandle, state: State<'_, AppState>) -> Vec<MountState> {
    let list = store::load_connections(&app);
    list.iter()
        .map(|c| state_snapshot(&app, &state, c))
        .collect()
}

#[tauri::command]
fn read_log(app: AppHandle, id: String) -> String {
    match store::mount_log_path(&app, &id) {
        Ok(p) => rclone::tail_log(&p, 80),
        Err(_) => String::new(),
    }
}

#[tauri::command]
fn reveal_password(app: AppHandle, id: String) -> Option<String> {
    rclone::reveal_password(&app, &id)
}

#[tauri::command]
fn open_target(app: AppHandle, id: String) -> Result<(), String> {
    let target = store::load_connections(&app)
        .into_iter()
        .find(|c| c.id == id)
        .map(|c| c.target)
        .ok_or_else(|| "连接不存在".to_string())?;

    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(&target);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(&target);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&target);
        c
    };

    cmd.spawn().map_err(|e| format!("打开失败：{e}"))?;
    Ok(())
}

#[tauri::command]
fn get_autostart() -> bool {
    autostart::is_enabled()
}

#[tauri::command]
fn set_autostart(enable: bool) -> Result<(), String> {
    autostart::set_enabled(enable)
}

/// 连接测试放到阻塞线程池里跑，避免 15 秒超时把界面卡住
#[tauri::command]
async fn test_connection(app: AppHandle, input: ConnectionInput) -> Result<DiagResult, String> {
    let conn = to_connection(&input, None);
    let password = if input.password.is_empty() {
        input
            .id
            .clone()
            .and_then(|id| rclone::reveal_password(&app, &id))
    } else {
        Some(input.password.clone())
    };

    tauri::async_runtime::spawn_blocking(move || diag::test_connection(&app, &conn, password))
        .await
        .map_err(|e| format!("测试任务失败：{e}"))
}

// ---------------------------------------------------------------- 后台看护

/// 启动时自动挂载 + 意外退出的自动重连，都在这条线程里做。
fn spawn_supervisor(app: AppHandle) {
    std::thread::spawn(move || {
        // 启动阶段：等界面准备好再挂，避免和窗口初始化抢资源
        std::thread::sleep(Duration::from_secs(2));
        {
            let state = app.state::<AppState>();
            for conn in store::load_connections(&app)
                .into_iter()
                .filter(|c| c.auto_mount)
            {
                let _ = start_session(&app, &state, &conn, 0);
            }
            let _ = app.emit("state-changed", ());
        }

        // 看护循环：进程意外退出时按退避策略重连
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let state = app.state::<AppState>();

            let exited: Vec<(String, u32)> = {
                let mut map = state.sessions.lock().unwrap();
                let ids: Vec<String> = map.keys().cloned().collect();
                let mut out = Vec::new();
                for id in ids {
                    if let Some(s) = map.get_mut(&id) {
                        if matches!(s.child.try_wait(), Ok(Some(_))) {
                            out.push((id.clone(), s.restarts));
                        }
                    }
                }
                for (id, _) in &out {
                    map.remove(id);
                }
                out
            };

            if exited.is_empty() {
                continue;
            }

            for (id, restarts) in exited {
                let conns = store::load_connections(&app);
                let Some(conn) = conns.into_iter().find(|c| c.id == id) else {
                    continue;
                };
                let tail = store::mount_log_path(&app, &id)
                    .map(|p| rclone::tail_log(&p, 8))
                    .unwrap_or_default();
                state.errors.lock().unwrap().insert(id.clone(), tail);

                if conn.reconnect && restarts < 5 {
                    // 退避：2s, 5s, 8s, 11s, 14s
                    std::thread::sleep(Duration::from_secs(2 + restarts as u64 * 3));
                    let _ = start_session(&app, &state, &conn, restarts + 1);
                }
            }
            let _ = app.emit("state-changed", ());
        }
    });
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let mount_all = MenuItem::with_id(app, "mount_all", "全部挂载", true, None::<&str>)?;
    let unmount_all = MenuItem::with_id(app, "unmount_all", "全部卸载", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &mount_all, &unmount_all, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Rclone Mount 管理器")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            "mount_all" => {
                let _ = app.emit("tray-action", "mount_all");
            }
            "unmount_all" => {
                let _ = app.emit("tray-action", "unmount_all");
            }
            "quit" => app.exit(0),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    app.state::<AppState>()
        .tray_ready
        .store(true, Ordering::SeqCst);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            // 托盘失败不应阻止应用启动，只是失去"最小化到托盘"能力
            let _ = build_tray(app);

            let handle = app.handle().clone();
            spawn_supervisor(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.app_handle().state::<AppState>();
                if state.tray_ready.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_env,
            list_connections,
            save_connection,
            set_pinned,
            delete_connection,
            mount,
            unmount,
            mount_all,
            unmount_all,
            mount_states,
            read_log,
            reveal_password,
            open_target,
            get_autostart,
            set_autostart,
            test_connection
        ])
        .build(tauri::generate_context!())
        .expect("Tauri 应用构建失败")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                let state = app_handle.state::<AppState>();
                let drained: Vec<(String, Session)> = state.sessions.lock().unwrap().drain().collect();
                for (_, mut s) in drained {
                    let _ = s.child.kill();
                    let _ = s.child.wait();
                }
            }
        });
}
