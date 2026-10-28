mod autostart;
mod crypt;
mod diag;
mod model;
mod rclone;
mod sendto;
mod settings;
mod store;
mod update;

use model::{Connection, ConnectionInput, DiagResult, MountState, Protocol, TrafficDay};
use settings::Settings;
use std::collections::HashMap;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

/// 一个进行中的挂载会话
pub struct Session {
    child: Child,
    started: Instant,
    restarts: u32,
    /// 该挂载进程的 rc 端口，界面靠它读实时吞吐
    rc_port: u16,
}

/// 一次健康探测的结果
#[derive(Clone, Debug)]
pub struct Health {
    /// healthy / slow / error
    pub state: &'static str,
    /// 探测耗时，单位**微秒**（轻量探测的局域网握手常在 1ms 以内，毫秒精度看不出来）
    pub latency_us: Option<u64>,
    pub last_check: i64,
    /// 连续失败次数：1~2 次算「波动」，3 次以上才算「故障」
    pub fails: u32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            state: "healthy",
            latency_us: None,
            last_check: 0,
            fails: 0,
        }
    }
}

/// 一个连接本次挂载期间的统计数据，详情页「节点历史」直接展示这个
#[derive(Clone, Default, serde::Serialize)]
pub struct StatsSnapshot {
    /// 最近一次采样的实时吞吐（字节/秒，读写合计）
    pub speed_bps: u64,
    /// 本次挂载的峰值吞吐
    pub peak_bps: u64,
    /// 本次已传输总量
    pub total_bytes: u64,
    /// 累计传输次数 / rclone 报告的传输错误数
    pub transfers: u64,
    pub errors: u64,
    /// 挂载进程已运行秒数
    pub elapsed_secs: u64,
    /// 探测成功 / 失败次数
    pub probes_ok: u64,
    pub probes_fail: u64,
    /// 最近一次探测耗时（微秒）；「仅进程」模式没有网络往返，这里是 None
    pub latency_us: Option<u64>,
    /// 最近 40 次探测的延迟（微秒，0 表示这次失败），前端画折线用
    pub history: Vec<u32>,
    /// 本次挂载的上行字节（写远端）。方向按「是否有活跃上传」推断，见 stats monitor 注释
    pub up_bytes: u64,
    /// 本次挂载的下行字节（读远端）
    pub down_bytes: u64,
    /// 今天 / 累计的上行、下行（来自落盘的流量账本，跨重启保留）
    pub today_up: u64,
    pub today_down: u64,
    pub total_up: u64,
    pub total_down: u64,
    /// 上一次采样的字节数，用来算瞬时速度（不落前端）
    #[serde(skip)]
    pub last_bytes: u64,
    /// 上一次采样的时间（毫秒时间戳），用真实间隔算速度，避免线程抖动把速度算歪
    #[serde(skip)]
    pub last_sample_ms: u64,
}

impl StatsSnapshot {
    fn push_history(&mut self, us: Option<u64>) {
        self.history.push(us.unwrap_or(0).min(u32::MAX as u64) as u32);
        if self.history.len() > 40 {
            let drop = self.history.len() - 40;
            self.history.drain(0..drop);
        }
    }
}

pub struct AppState {
    sessions: Mutex<HashMap<String, Session>>,
    /// 每个连接最近一次失败原因（挂载日志尾部）
    errors: Mutex<HashMap<String, String>>,
    /// 每个已挂载连接的健康状况（主动探测得来）
    health: Mutex<HashMap<String, Health>>,
    /// 每个已挂载连接的实时统计（rc 采样 + 探测结果汇总）
    stats: Mutex<HashMap<String, StatsSnapshot>>,
    /// 上传/下载账本（按连接 id），每 5 秒采样时累加，定期落盘
    traffic: Mutex<HashMap<String, TrafficDay>>,
    /// 「发送到」待处理请求：资源管理器传进来的文件在这里等着界面来取。
    /// 存一份而不是只发事件，是因为窗口可能还在托盘里、或者界面尚未加载完，
    /// 只发事件会直接丢掉这次请求。
    send_req: Mutex<Option<sendto::SendRequest>>,
    /// 托盘是否创建成功：成功时关闭窗口改为"隐藏到托盘"
    tray_ready: AtomicBool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            errors: Mutex::new(HashMap::new()),
            health: Mutex::new(HashMap::new()),
            stats: Mutex::new(HashMap::new()),
            traffic: Mutex::new(HashMap::new()),
            send_req: Mutex::new(None),
            tray_ready: AtomicBool::new(false),
        }
    }
}

/// 账本日期键（本地时区）：跨天时把「今天」清零，累计值不动。
fn today_key() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// 从 rclone 日志尾部挑一句最适合放进通知的话：
/// 优先最后一条 CRITICAL/ERROR/Failed，没有就退回最后一行非空内容。
fn reason_line(tail: &str) -> String {
    let pick = tail
        .lines()
        .filter(|l| {
            let u = l.to_uppercase();
            u.contains("CRITICAL") || u.contains("ERROR") || u.contains("FAILED")
        })
        .last()
        .or_else(|| tail.lines().filter(|l| !l.trim().is_empty()).last())
        .unwrap_or("");
    // 去掉 rclone 的「日期 时间 级别:」前缀。注意不能按第一个冒号切：
    // 时间本身就带冒号（09:46:21），会切在时间中间。
    let mut out = pick.trim();
    for marker in ["CRITICAL:", "ERROR:", "NOTICE:", "INFO:", "DEBUG:"] {
        if let Some(i) = out.find(marker) {
            out = out[i + marker.len()..].trim();
            break;
        }
    }
    out.chars().take(160).collect()
}

fn traffic_entry<'a>(
    map: &'a mut HashMap<String, TrafficDay>,
    id: &str,
) -> &'a mut TrafficDay {
    let today = today_key();
    let entry = map.entry(id.to_string()).or_default();
    if entry.date != today {
        entry.date = today;
        entry.up = 0;
        entry.down = 0;
    }
    entry
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

/// 往连接日志尾部追加一行（探测结果也写进这里，UI 的"挂载日志"能直接看到）
fn append_probe_log(app: &AppHandle, id: &str, cfg: &Settings, line: &str) {
    let Ok(path) = store::mount_log_path(app, id) else {
        return;
    };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
    // 超过大小上限就裁掉前半段，保留最近的内容
    if let Ok(meta) = std::fs::metadata(&path) {
        let max = cfg.log_max_mb * 1024 * 1024;
        if meta.len() > max {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            let keep_from = lines.len() / 2;
            let _ = std::fs::write(&path, lines[keep_from..].join("\n"));
        }
    }
}

/// 按保留天数清理过期日志；每小时跑一次就够
fn cleanup_old_logs(app: &AppHandle, cfg: &Settings) {
    let Ok(dir) = store::log_dir(app) else { return };
    let cutoff = std::time::SystemTime::now()
        - Duration::from_secs(cfg.log_retention_days as u64 * 86_400);
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let expired = meta
            .modified()
            .map(|m| m < cutoff)
            .unwrap_or(false);
        if expired {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// 时间戳，探测日志用
fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// 微秒 → 人能读的延迟文本：0.42ms 写成 "0.42 ms"，42ms 就写整数
fn fmt_ms(us: u64) -> String {
    if us < 1_000 {
        format!("{:.2} ms", us as f64 / 1000.0)
    } else if us < 1_000_000 {
        format!("{:.0} ms", us as f64 / 1000.0)
    } else {
        format!("{:.1} s", us as f64 / 1_000_000.0)
    }
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
        target: normalize_target(&input.target),
        cache_dir: input.cache_dir.trim().to_string(),
        ignore_cert: input.ignore_cert,
        s3_provider: if input.s3_provider.trim().is_empty() {
            "Other".to_string()
        } else {
            input.s3_provider.trim().to_string()
        },
        region: input.region.trim().to_string(),
        s3_path_style: input.s3_path_style,
        read_only: input.read_only,
        network_mode: input.network_mode,
        bwlimit: input.bwlimit.trim().to_string(),
        cache_max_size: input.cache_max_size.trim().to_string(),
        proxy: input.proxy.trim().to_string(),
        reconnect: input.reconnect,
        auto_mount: input.auto_mount,
        pinned: existing.map(|c| c.pinned).unwrap_or(input.pinned),
        group: input.group.trim().to_string(),
        use_system_proxy: input.use_system_proxy,
        probe_mode: match input.probe_mode.as_str() {
            "full" | "light" | "process" => input.probe_mode.clone(),
            _ => "light".into(),
        },
        extra_args: input.extra_args.trim().to_string(),
        created_at: existing.map(|c| c.created_at).unwrap_or_else(now_secs),
    }
}

/// 找一个空闲的本机端口给 rclone 的 rc 用（向系统要一个临时端口）
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(0)
}

/// 用户家目录。unix 一律看 HOME：Tauri 进程由桌面会话拉起，这个变量总是有的。
#[cfg(unix)]
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

/// 新建连接时的默认挂载目标。Windows 给一个固定盘符；unix 放家目录下，
/// 因为 `/mnt` 属于 root，普通用户建不了目录，一挂就报权限错误（deepin 上实测）。
#[tauri::command]
fn default_mount_target() -> String {
    #[cfg(windows)]
    {
        "Z:".to_string()
    }
    #[cfg(unix)]
    {
        match home_dir() {
            Some(h) => h.join("mnt").join("nas").to_string_lossy().to_string(),
            None => "/mnt/nas".to_string(),
        }
    }
}

/// 确保 unix 的挂载点目录存在。FUSE 要求挂载点先建好，缺了 rclone 直接
/// `cannot open: /mnt/nas: No such file or directory`。
/// 家目录这种位置普通权限就能建；`/mnt` 之类的要 root，Linux 上顺手走一次
/// pkexec（polkit 弹系统授权窗），省得用户自己开终端敲 sudo。macOS 没有 pkexec，
/// 建不出来就把补救命令写进错误里。
#[cfg(unix)]
fn ensure_mount_point(target: &str) -> Result<(), String> {
    let t = target.trim();
    if t.is_empty() {
        return Ok(());
    }
    let p = std::path::Path::new(t);
    if p.is_dir() {
        return Ok(());
    }
    if p.exists() {
        return Err(format!("挂载点 {t} 已存在，但不是目录。换一个位置再试。"));
    }
    if std::fs::create_dir_all(p).is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        // sh -c 里一次做完「建目录 + 把属主改回当前用户」：只弹一次授权，
        // 而且挂载点归自己以后 fusermount 挂载、卸载都不会再卡权限。
        // PKEXEC_UID 由 pkexec 自己注入，$1 是下面用 "sh" 占位后传进来的路径。
        let script = "mkdir -p \"$1\" && chown \"$PKEXEC_UID:$PKEXEC_UID\" \"$1\"";
        if let Ok(mut child) = std::process::Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(script)
            .arg("sh")
            .arg(t)
            .spawn()
        {
            // 用户在授权窗里输密码可能慢，给两分钟；超时就杀掉，避免挂载一直转圈
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            loop {
                match child.try_wait() {
                    Ok(Some(st)) if st.success() && p.is_dir() => return Ok(()),
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                    Ok(None) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    Err(_) => break,
                }
            }
        }
    }

    let alt = home_dir()
        .map(|h| {
            format!(
                "，或者把「目标位置」改到家目录下（例如 {}/mnt/nas），那里不需要管理员权限",
                h.display()
            )
        })
        .unwrap_or_default();
    Err(format!(
        "挂载点 {t} 不存在，自动创建被系统拒绝（需要管理员权限）。请在终端执行 `sudo mkdir -p {t}` 后重试{alt}。"
    ))
}

/// 挂载目标归一化：`~` 展开成家目录（rclone 不认 `~`，会当相对路径处理），
/// 其余平台去掉首尾空白。
fn normalize_target(raw: &str) -> String {
    #[cfg(unix)]
    {
        let p = raw.trim();
        if p == "~" {
            return home_dir()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|| p.to_string());
        }
        if let Some(rest) = p.strip_prefix("~/") {
            if let Some(h) = home_dir() {
                return h.join(rest).to_string_lossy().to_string();
            }
        }
        p.to_string()
    }
    #[cfg(windows)]
    {
        raw.trim().to_string()
    }
}

/// 启动挂载并把会话登记进状态表
fn start_session(app: &AppHandle, state: &AppState, conn: &Connection, restarts: u32) -> Result<u32, String> {
    let log_path = store::mount_log_path(app, &conn.id)?;
    let _ = std::fs::write(&log_path, "");
    // 每次启动都按连接信息重写一遍 remote：老版本写坏的配置（例如把 WebDAV 地址拼成
    // http://5005//192.168.50.4）不用用户重新编辑连接就能自愈。密码沿用已有值。
    let _ = rclone::upsert_remote(app, conn, None);

    // Unix 的 FUSE 挂载要求挂载点目录已经存在（Windows 盘符由系统管，不需要），
    // 缺了 rclone 会直接 CRITICAL: cannot open: /mnt/nas: no such file or directory。
    // 这里尽力帮用户补一个，需要提权时走 pkexec；真补不出来就把补救方法写进错误里。
    #[cfg(unix)]
    ensure_mount_point(&conn.target)?;

    let rc_port = free_port();
    let child = rclone::spawn_mount(app, conn, &log_path, rc_port)?;
    let pid = child.id();
    state.sessions.lock().unwrap().insert(
        conn.id.clone(),
        Session {
            child,
            started: Instant::now(),
            restarts,
            rc_port,
        },
    );
    // 新会话从零开始统计
    state
        .stats
        .lock()
        .unwrap()
        .insert(conn.id.clone(), StatsSnapshot::default());
    state.errors.lock().unwrap().remove(&conn.id);
    state.health.lock().unwrap().insert(conn.id.clone(), Health::default());
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
fn save_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    input: ConnectionInput,
) -> Result<Connection, String> {
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
    // 自定义参数先过一遍校验：写错了在这里就报出来，而不是等挂载时 rclone 莫名退出
    let _ = rclone::parse_extra_args(&input.extra_args)?;

    let mut list = store::load_connections(&app);
    let idx = input
        .id
        .as_ref()
        .and_then(|id| list.iter().position(|c| &c.id == id));

    // 盘符/挂载点冲突预检：别的连接正占着同一个位置时直接拒绝
    let self_id = idx.map(|i| list[i].id.clone()).unwrap_or_default();
    let target = input.target.trim();
    // 编辑时位置没变就不用查系统占用——那个位置本来就是自己挂着的
    let unchanged = idx
        .map(|i| list[i].target.trim().eq_ignore_ascii_case(target))
        .unwrap_or(false);
    if !unchanged {
        if let Some(other) = list
            .iter()
            .find(|c| c.id != self_id && c.target.trim().eq_ignore_ascii_case(target))
        {
            return Err(format!(
                "{target} 已被连接「{}」占用，请换一个盘符或先停用它",
                other.name
            ));
        }
        if rclone::target_taken(target) {
            return Err(format!("{target} 已被系统或其他程序占用，请换一个盘符"));
        }
    }

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

    // 改完已经在跑：直接重启一次，让地址 / 盘符 / 密码的改动马上生效，
    // 而不是让用户自己先停再启（改错时也能立刻看到错误）。
    let was_mounted = state.sessions.lock().unwrap().contains_key(&conn.id);
    if was_mounted {
        if let Some(mut session) = state.sessions.lock().unwrap().remove(&conn.id) {
            let _ = rclone::unmount(&mut session.child, "");
        }
        state.health.lock().unwrap().remove(&conn.id);
        let _ = start_session(&app, state.inner(), &conn, 0);
        let _ = app.emit("state-changed", ());
    }
    // 名字/分组/盘符都可能改了，菜单文字要跟着变
    sync_sendto_menu(&app);
    Ok(conn)
}

/// 详情页「节点历史」的数据源：本次挂载的吞吐、峰值、探测成功率、延迟曲线
#[tauri::command]
fn get_stats(state: State<'_, AppState>, id: String) -> StatsSnapshot {
    let mut snap = state
        .stats
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .unwrap_or_default();
    // 今日/累计来自落盘账本，单独取（不在采样快照里，重启后也还在）
    if let Some(t) = state.traffic.lock().unwrap().get(&id) {
        let fresh = t.date == today_key();
        snap.today_up = if fresh { t.up } else { 0 };
        snap.today_down = if fresh { t.down } else { 0 };
        snap.total_up = t.total_up;
        snap.total_down = t.total_down;
    }
    snap
}

/// 把这次挂载的流量计数清零（峰值、累计、次数都归零）。
///
/// 用途：想单独量「复制这一个文件到底花了多少流量」时先点清零再操作。
/// 只动内存里的统计和 rclone 自己的计数器，不影响挂载本身。
#[tauri::command]
fn reset_stats(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let port = state
        .sessions
        .lock()
        .unwrap()
        .get(&id)
        .map(|s| s.rc_port)
        .unwrap_or(0);
    if port != 0 {
        // rclone 侧的累计量也要清，否则下一次采样又会被读回来
        let _ = rclone::rc_call(port, "core/stats-reset");
    }
    let mut guard = state.stats.lock().unwrap();
    let entry = guard.entry(id).or_default();
    entry.total_bytes = 0;
    entry.last_bytes = 0;
    entry.speed_bps = 0;
    entry.peak_bps = 0;
    entry.transfers = 0;
    entry.errors = 0;
    entry.up_bytes = 0;
    entry.down_bytes = 0;
    entry.last_sample_ms = 0;
    Ok(())
}

/// 手动做一次「完整探测」，返回微秒。
///
/// 轻量探测只测 TCP 握手（不到 1ms，不代表读写延迟），用户想看真实数字时点一下这个。
/// 由用户主动触发、只跑一次，不会像定时探测那样把休眠硬盘反复唤醒。
#[tauri::command]
fn probe_now(app: AppHandle, id: String) -> Result<u64, String> {
    let list = store::load_connections(&app);
    let conn = list
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| "连接不存在".to_string())?;
    let cfg = settings::load(&app);
    let started = std::time::Instant::now();
    let result = rclone::probe_latency(&app, conn, 8);
    match &result {
        Ok(us) => append_probe_log(
            &app,
            &id,
            &cfg,
            &format!("[手动探测 {}] 完整探测 · {}", timestamp(), fmt_ms(*us)),
        ),
        Err(e) => append_probe_log(
            &app,
            &id,
            &cfg,
            &format!(
                "[手动探测 {}] 失败（{:.1}s）· {}",
                timestamp(),
                started.elapsed().as_secs_f64(),
                e
            ),
        ),
    }
    result
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
    state.health.lock().unwrap().remove(&id);
    state.stats.lock().unwrap().remove(&id);
    // 连接删了，它的流量账本也一起清掉：首页合计是按连接求和的，别留孤儿数据
    state.traffic.lock().unwrap().remove(&id);
    let mut list = store::load_connections(&app);
    list.retain(|c| c.id != id);
    store::save_connections(&app, &list)?;
    rclone::remove_remote(&app, &id)?;
    let _ = app.emit("connections-changed", ());
    sync_sendto_menu(&app);
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
        // 界面可能被收在托盘里，只有日志是看不见的 —— 让前端弹一条系统通知
        let _ = app.emit(
            "notify",
            serde_json::json!({
                "kind": "mount-failed",
                "id": id,
                "name": conn.name,
                "reason": reason_line(&tail),
            }),
        );
        return Err(format!("挂载进程已退出。\n\n{tail}"));
    }

    let _ = app.emit("state-changed", ());
    // 挂上之后才在「发送到」里露出这一项
    sync_sendto_menu(&app);
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
    state.health.lock().unwrap().remove(&id);
    state.stats.lock().unwrap().remove(&id);
    // 卸载时把账本落盘，避免下次启动丢掉最后这一分钟的流量
    let snapshot = state.traffic.lock().unwrap().clone();
    let _ = store::save_traffic(&app, &snapshot);
    let _ = app.emit("state-changed", ());
    // 盘已经不在了，菜单项必须跟着消失，否则点了会复制失败
    sync_sendto_menu(&app);
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
    sync_sendto_menu(&app);
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
    sync_sendto_menu(&app);
    Ok(count)
}

fn state_snapshot(app: &AppHandle, state: &AppState, conn: &Connection) -> MountState {
    let map = state.sessions.lock().unwrap();
    let session = map.get(&conn.id);
    let health = state.health.lock().unwrap().get(&conn.id).cloned();
    let mounted = session.is_some();
    let health_state = health
        .as_ref()
        .map(|h| h.state.to_string())
        .unwrap_or_else(|| "healthy".into());
    let latency_us = if mounted {
        health.as_ref().and_then(|h| h.latency_us)
    } else {
        None
    };
    let last_check = health.as_ref().map(|h| h.last_check);

    MountState {
        id: conn.id.clone(),
        mounted,
        pid: session.map(|s| s.child.id()),
        since: session.map(|s| s.started.elapsed().as_secs()),
        restarts: session.map(|s| s.restarts).unwrap_or(0),
        last_error: state.errors.lock().unwrap().get(&conn.id).cloned(),
        log_path: store::mount_log_path(app, &conn.id)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        health: if mounted { health_state } else { "stopped".into() },
        latency_us,
        last_check,
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
    reveal_path(&target)
}

/// 在系统文件管理器里打开任意目录（「发送到」完成之后用它直接跳到目标位置）
#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    reveal_path(&path)
}

fn reveal_path(path: &str) -> Result<(), String> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
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

#[tauri::command]
async fn check_updates(app: AppHandle) -> Result<update::UpdateReport, String> {
    tauri::async_runtime::spawn_blocking(move || update::check(&app))
        .await
        .map_err(|e| format!("检查更新失败：{e}"))
}

#[tauri::command]
async fn install_rclone(app: AppHandle, version: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || update::install_rclone(&app, &version))
        .await
        .map_err(|e| format!("安装失败：{e}"))?
}

#[tauri::command]
async fn install_winfsp(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || update::install_winfsp(&app))
        .await
        .map_err(|e| format!("安装失败：{e}"))?
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    update::open_url(&url)
}

/// Ctrl+W / ⌘W：关窗不退出。托盘可用就隐藏到托盘（和点 X 的行为一致），
/// 托盘没建成就退化为最小化，避免窗口藏起来再也找不回。
#[tauri::command]
fn hide_main_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if app.state::<AppState>().tray_ready.load(Ordering::SeqCst) {
            let _ = w.hide();
        } else {
            let _ = w.minimize();
        }
    }
}

// ---------------------------------------------------------------- 设置 / 导入导出

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, s: Settings) -> Result<Settings, String> {
    let valid = s.validate()?;
    settings::save(&app, &valid)?;
    // 「发送到」开关可能刚被拨动：开着就补齐菜单项，关掉就整个清掉
    sync_sendto_menu(&app);
    Ok(valid)
}

/// 导出连接配置为 JSON 字符串。密码以 rclone obscure 形式附带（非明文），
/// 没有密码的连接导出空值。由前端负责落盘（浏览器下载）。
///
/// 传了 `password` 就加密：返回的是带信封的 JSON（`encrypted: true`），
/// 因为 `password_obscured` 是可逆的混淆串，明文导出等于把口令一起给出去。
#[tauri::command]
fn export_connections(app: AppHandle, password: Option<String>) -> Result<String, String> {
    let list = store::load_connections(&app);
    let items: Vec<serde_json::Value> = list
        .iter()
        .map(|c| {
            let mut v = serde_json::to_value(c).unwrap_or_default();
            if let Some(obj) = v.as_object_mut() {
                obj.insert(
                    "password_obscured".into(),
                    serde_json::Value::String(rclone::raw_password(&app, &c.id).unwrap_or_default()),
                );
            }
            v
        })
        .collect();
    let doc = serde_json::json!({
        "app": "rclone-mount-manager",
        "format": 1,
        "exported_at": now_secs(),
        "connections": items,
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| format!("序列化失败：{e}"))?;
    match password.filter(|p| !p.is_empty()) {
        Some(p) => crypt::encrypt(&text, &p),
        None => Ok(text),
    }
}

/// 导入连接配置：跳过与现有连接重名的条目，其余按合并方式追加，
/// id 一律重新生成避免冲突。返回实际导入的条数。
///
/// 若文件是口令加密的信封，必须带 `password` 才能解开，否则报错让前端去问口令。
#[tauri::command]
fn import_connections(
    app: AppHandle,
    data: String,
    password: Option<String>,
) -> Result<usize, String> {
    let mut doc: serde_json::Value =
        serde_json::from_str(&data).map_err(|_| "文件不是有效的 JSON 配置".to_string())?;

    if crypt::is_encrypted(&doc) {
        let p = password.unwrap_or_default();
        if p.is_empty() {
            return Err("ENCRYPTED_NEED_PASSWORD".into());
        }
        let plain = crypt::decrypt(&doc, &p)?;
        doc = serde_json::from_str(&plain).map_err(|_| "解密后的内容不是有效的配置".to_string())?;
    }

    let items = doc
        .get("connections")
        .and_then(|v| v.as_array())
        .ok_or("文件里找不到 connections 数组，请确认导出的是本应用的配置文件")?;

    let mut list = store::load_connections(&app);
    let mut imported = 0;
    for item in items {
        let mut conn: Connection = serde_json::from_value(item.clone())
            .map_err(|e| format!("连接条目解析失败：{e}"))?;
        let obscured = item
            .get("password_obscured")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // 名称冲突的条目跳过，避免把现有配置搅乱
        if list.iter().any(|c| c.name == conn.name) {
            continue;
        }
        conn.id = new_id();
        conn.created_at = now_secs();
        conn.pinned = false;
        rclone::upsert_remote_obscured(&app, &conn, obscured.as_deref())?;
        list.push(conn);
        imported += 1;
    }
    if imported > 0 {
        store::save_connections(&app, &list)?;
        let _ = app.emit("connections-changed", ());
    }
    Ok(imported)
}

/// 首页概览用的全部连接流量合计
#[derive(serde::Serialize, Default)]
pub struct TrafficSummary {
    pub today_up: u64,
    pub today_down: u64,
    pub total_up: u64,
    pub total_down: u64,
}

#[tauri::command]
fn traffic_summary(state: State<'_, AppState>) -> TrafficSummary {
    let today = today_key();
    let map = state.traffic.lock().unwrap();
    let mut out = TrafficSummary::default();
    for day in map.values() {
        // 跨天但还没重新采样过的条目，今天的部分不算
        if day.date == today {
            out.today_up += day.up;
            out.today_down += day.down;
        }
        out.total_up += day.total_up;
        out.total_down += day.total_down;
    }
    out
}

/// 盘符/挂载点能不能用。前端在表单里实时调用，早点把冲突说清楚。
#[derive(serde::Serialize)]
pub struct TargetCheck {
    pub available: bool,
    /// "" / "other-connection" / "occupied"
    pub reason: String,
    /// 占用它的连接名（reason=other-connection 时有用）
    pub owner: String,
}

#[tauri::command]
fn check_target(app: AppHandle, target: String, exclude_id: String) -> TargetCheck {
    let t = target.trim();
    let ok = TargetCheck {
        available: true,
        reason: String::new(),
        owner: String::new(),
    };
    if t.is_empty() {
        return ok;
    }
    if let Some(other) = store::load_connections(&app)
        .into_iter()
        .find(|c| c.id != exclude_id && c.target.trim().eq_ignore_ascii_case(t))
    {
        return TargetCheck {
            available: false,
            reason: "other-connection".into(),
            owner: other.name,
        };
    }
    // 自己正挂在这个位置上，不算冲突
    let mine = store::load_connections(&app)
        .into_iter()
        .find(|c| c.id == exclude_id)
        .map(|c| c.target)
        .unwrap_or_default();
    if !mine.trim().eq_ignore_ascii_case(t) && rclone::target_taken(t) {
        return TargetCheck {
            available: false,
            reason: "occupied".into(),
            owner: String::new(),
        };
    }
    ok
}

/// 弹一条系统通知。文案由前端按当前语言拼好再传进来，
/// 这样通知和界面语言永远一致（后端不必再维护一套翻译表）。
#[tauri::command]
fn notify_user(app: AppHandle, title: String, body: String) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| format!("系统通知发送失败：{e}"))
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

// ---------------------------------------------------------------- 「发送到」菜单

/// 把「当前真正挂载着」的连接同步进 SendTo 菜单。
/// 挂载、卸载、删除连接、改设置之后都要调一次；看护线程发现进程掉了也会调。
fn sync_sendto_menu(app: &AppHandle) {
    let mounted: Vec<(String, String, String)> = {
        let state = app.state::<AppState>();
        let ids: Vec<String> = state.sessions.lock().unwrap().keys().cloned().collect();
        store::load_connections(app)
            .into_iter()
            .filter(|c| ids.iter().any(|id| id == &c.id))
            .map(|c| (c.name, c.group, c.target))
            .collect()
    };
    sendto::sync(app, &mounted);
}

/// 解析命令行里的「发送到」参数。
/// 资源管理器的规则是：把快捷方式的参数和自己拼起来，再把选中的文件路径追加在后面，
/// 所以形态固定为 `--sendto Z: "C:\a.txt" "C:\b.txt"`。
fn parse_sendto_args(args: &[String]) -> Option<sendto::SendRequest> {
    let mut target: Option<String> = None;
    let mut files: Vec<String> = Vec::new();
    // 第一个参数是可执行文件自己
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a.as_str() == "--sendto" {
            target = it.next().map(|s| s.trim().trim_matches('"').to_string());
            continue;
        }
        if a.starts_with("--") {
            continue;
        }
        files.push(a.trim_matches('"').to_string());
    }
    let target = target?;
    if files.is_empty() {
        return None;
    }
    Some(sendto::SendRequest { target, files })
}

/// 收到一次发送请求：存起来（等确认窗来取）+ 只把「发送到」小窗叫出来。
///
/// 这里**刻意不碰主窗口**：主界面常常收在托盘里，用户只是想把文件丢到挂载盘，
/// 不应该为此把整个管理器拽到前台。只有小窗建不起来（极少见）时，
/// 才退回「显示主窗口 + 主界面内弹确认框」，保证请求不会石沉大海。
fn queue_sendto(app: &AppHandle, req: sendto::SendRequest) {
    *app.state::<AppState>().send_req.lock().unwrap() = Some(req.clone());

    match sendto::show_window(app) {
        // 新建：页面加载完会自己 sendto_take，不用再推事件
        Ok(true) => {}
        // 已经开着：直接把新请求推给它，前面那次传输的结果面板会被替换
        Ok(false) => {
            let _ = app.emit_to(sendto::WINDOW_LABEL, "sendto-request", req);
        }
        // 退路：小窗建不起来，至少别让用户以为没反应
        Err(_) => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            let _ = app.emit_to("main", "sendto-request", req);
        }
    }
}

/// 界面启动后主动来取一次：处理「本应用还没运行时就点了发送到」的情况
#[tauri::command]
fn sendto_take(state: State<'_, AppState>) -> Option<sendto::SendRequest> {
    state.send_req.lock().unwrap().take()
}

#[tauri::command]
fn sendto_dirs(path: String) -> Result<Vec<sendto::DirEntry>, String> {
    sendto::list_dirs(&path)
}

#[tauri::command]
fn sendto_recent(app: AppHandle, target: String) -> Vec<String> {
    sendto::recent(&app, &target)
}

#[tauri::command]
fn sendto_run(
    app: AppHandle,
    sources: Vec<String>,
    dest: String,
    target: String,
    mode: String,
) -> Result<(), String> {
    if sources.is_empty() {
        return Err("没有要发送的文件".into());
    }
    if sendto::running() {
        return Err("已有一次传输在进行中".into());
    }
    sendto::run(
        app,
        sendto::Task {
            sources,
            dest,
            target,
            mode,
        },
    );
    Ok(())
}

#[tauri::command]
fn sendto_cancel() {
    sendto::cancel();
}

/// 「发送到」小窗首帧画完了：这时才显示，避免先白屏再出现内容
#[tauri::command]
fn sendto_window_ready(window: tauri::Window) {
    let _ = window.show();
    let _ = window.set_focus();
}

/// 关掉「发送到」小窗自己（不走系统关闭按钮，免得和托盘的「关闭=隐藏」混在一起）
#[tauri::command]
fn sendto_window_close(window: tauri::Window) {
    let _ = window.close();
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
            // 开机自启动挂上的盘，也要在「发送到」里就位
            sync_sendto_menu(&app);
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
                state.errors.lock().unwrap().insert(id.clone(), tail.clone());

                if conn.reconnect && restarts < 5 {
                    // 退避：2s, 5s, 8s, 11s, 14s
                    std::thread::sleep(Duration::from_secs(2 + restarts as u64 * 3));
                    let _ = start_session(&app, &state, &conn, restarts + 1);
                } else if conn.reconnect {
                    // 连退 5 次都没起来，不再自动重试 —— 这时必须让人知道
                    let _ = app.emit(
                        "notify",
                        serde_json::json!({
                            "kind": "gave-up",
                            "id": id,
                            "name": conn.name,
                            "reason": reason_line(&tail),
                        }),
                    );
                }
            }
            let _ = app.emit("state-changed", ());
            // 进程掉了（还没重连上）这一项就不该再出现在「发送到」里
            sync_sendto_menu(&app);
        }
    });
}

/// 每 5 秒向各挂载进程的本机 rc 端口要一次统计，算出瞬时吞吐。
/// 只是本机回环上一次很小的 HTTP 请求 + 内存里几个数字，对 CPU / 磁盘基本没有影响。
fn spawn_stats_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(12));
        // 账本每 12 个采样（约 60 秒）落一次盘，避免每次采样都写文件
        let mut ticks: u32 = 0;
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let state = app.state::<AppState>();
            let targets: Vec<(String, u16)> = state
                .sessions
                .lock()
                .unwrap()
                .iter()
                .map(|(id, s)| (id.clone(), s.rc_port))
                .collect();
            if targets.is_empty() {
                continue;
            }
            for (id, port) in targets {
                let sample = rclone::rc_call(port, "core/stats");
                let mut guard = state.stats.lock().unwrap();
                let Some(entry) = guard.get_mut(&id) else { continue };
                // 这一轮移动了多少字节、算上行还是下行（出了这段再更新账本，避免嵌套锁）
                let mut moved: Option<(u64, bool)> = None;
                match sample {
                    Some(v) => {
                        let bytes = v.get("bytes").and_then(|x| x.as_u64()).unwrap_or(0);
                        let delta = bytes.saturating_sub(entry.last_bytes);
                        // 速度按**两次采样的真实间隔**算：线程睡 5 秒实际可能是 5.3 秒，
                        // 用固定 5 除会把速度算高，短时突发也会失真
                        let now_ms = now_millis();
                        let dt = if entry.last_sample_ms == 0 {
                            5.0
                        } else {
                            ((now_ms.saturating_sub(entry.last_sample_ms)) as f64 / 1000.0).clamp(0.5, 60.0)
                        };
                        entry.speed_bps = (delta as f64 / dt).round() as u64;
                        entry.last_sample_ms = now_ms;
                        if entry.speed_bps > entry.peak_bps {
                            entry.peak_bps = entry.speed_bps;
                        }
                        entry.last_bytes = bytes;
                        entry.total_bytes = bytes;
                        if let Some(n) = v.get("totalTransfers").and_then(|x| x.as_u64()) {
                            entry.transfers = n;
                        }
                        if let Some(n) = v.get("errors").and_then(|x| x.as_u64()) {
                            entry.errors = n;
                        }
                        entry.elapsed_secs = v
                            .get("elapsedTime")
                            .and_then(|x| x.as_f64())
                            .unwrap_or(0.0) as u64;

                        // 方向判定：rclone 的 core/stats 只有「收发合计」，没有分向字段
                        // （core/stats 与 vfs/stats 都实测过）。但挂载读远端走的是 VFS 读句柄，
                        // 不会出现在 transferring 里；出现在里面的都是「写远端」的上传。
                        // 所以：有活跃上传时这段增量记上行，否则记下行。
                        // 同时读写的极端情况下会偏向一端，属于已知近似。
                        let uploading = v
                            .get("transferring")
                            .and_then(|x| x.as_array())
                            .map(|a| !a.is_empty())
                            .unwrap_or(false);
                        if delta > 0 {
                            if uploading {
                                entry.up_bytes = entry.up_bytes.saturating_add(delta);
                            } else {
                                entry.down_bytes = entry.down_bytes.saturating_add(delta);
                            }
                            moved = Some((delta, uploading));
                        }
                    }
                    // 进程刚起或已退出：速度归零，别把上一次的数字挂着骗人
                    None => entry.speed_bps = 0,
                }
                drop(guard);

                if let Some((delta, uploading)) = moved {
                    let mut ledger = state.traffic.lock().unwrap();
                    let day = traffic_entry(&mut ledger, &id);
                    if uploading {
                        day.up = day.up.saturating_add(delta);
                        day.total_up = day.total_up.saturating_add(delta);
                    } else {
                        day.down = day.down.saturating_add(delta);
                        day.total_down = day.total_down.saturating_add(delta);
                    }
                }
            }

            ticks += 1;
            if ticks % 12 == 0 {
                let snapshot = state.traffic.lock().unwrap().clone();
                let _ = store::save_traffic(&app, &snapshot);
            }
        }
    });
}

/// 周期性主动探测已挂载连接：把「网络波动」变成看得见的黄灯。
/// 间隔与阈值都能在设置页里调，改动保存后下一轮立即生效。
fn spawn_health_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(8));
        let mut last_cleanup = Instant::now();
        loop {
            let cfg = settings::load(&app);
            std::thread::sleep(Duration::from_secs(cfg.probe_interval_secs.max(3)));

            let state = app.state::<AppState>();
            let ids: Vec<String> = state.sessions.lock().unwrap().keys().cloned().collect();
            if !ids.is_empty() {
                let conns = store::load_connections(&app);
                let mut changed = false;

                for id in ids {
                    let Some(conn) = conns.iter().find(|c| c.id == id) else {
                        continue;
                    };
                    // 探测方式（连接表单里可选，默认轻量）：
                    // full=完整探测（真实列目录，会唤醒休眠硬盘）
                    // light=轻量（TCP 握手，不碰盘）
                    // process=仅看挂载进程存活（零网络流量）
                    let result: Result<u64, String> = match conn.probe_mode.as_str() {
                        "process" => {
                            let alive = {
                                let mut sessions = state.sessions.lock().unwrap();
                                match sessions.get_mut(&id) {
                                    Some(s) => s.child.try_wait().map(|st| st.is_none()).unwrap_or(false),
                                    None => false,
                                }
                            };
                            if alive {
                                Ok(0)
                            } else {
                                Err("挂载进程已退出".into())
                            }
                        }
                        "full" => rclone::probe_latency(&app, conn, 8),
                        _ => rclone::tcp_probe(conn, 8),
                    };
                    let ok_ms = result.as_ref().ok().copied();
                    // 「仅进程」模式只看进程存活，没有网络往返，不产生延迟样本
                    let tracks_latency = conn.probe_mode != "process";
                    let now = now_secs();

                    let mut guard = state.health.lock().unwrap();
                    let entry = guard.entry(id.clone()).or_default();
                    let before = (entry.state, entry.latency_us);

                    match result {
                        Ok(us) => {
                            entry.fails = 0;
                            entry.latency_us = if tracks_latency { Some(us) } else { None };
                            let ms = us as f64 / 1000.0;
                            // 单次探测超过阈值 → 黄（网络波动）
                            entry.state = if ms > cfg.latency_threshold_ms as f64 {
                                "slow"
                            } else {
                                "healthy"
                            };
                            let ok_text = if tracks_latency {
                                format!("[探测 {}] 正常 · {}", timestamp(), fmt_ms(us))
                            } else {
                                format!("[探测 {}] 挂载进程存活", timestamp())
                            };
                            append_probe_log(&app, &id, &cfg, &ok_text);
                        }
                        Err(e) => {
                            entry.fails = entry.fails.saturating_add(1);
                            entry.latency_us = None;
                            // 连续失败次数决定颜色：≥error_fails 红，≥warn_fails 黄
                            if entry.fails >= cfg.error_fails {
                                entry.state = "error";
                                state
                                    .errors
                                    .lock()
                                    .unwrap()
                                    .insert(id.clone(), format!("连接探测失败：{e}"));
                                append_probe_log(
                                    &app,
                                    &id,
                                    &cfg,
                                    &format!(
                                        "[探测 {}] 失败 · 连续第 {} 次 → 连接异常：{e}",
                                        timestamp(),
                                        entry.fails
                                    ),
                                );
                            } else if entry.fails >= cfg.warn_fails {
                                entry.state = "slow";
                                append_probe_log(
                                    &app,
                                    &id,
                                    &cfg,
                                    &format!(
                                        "[探测 {}] 超时 · 连续第 {} 次 → 网络波动",
                                        timestamp(),
                                        entry.fails
                                    ),
                                );
                            } else {
                                // 偶发一次失败先保持绿灯，写进日志供排查
                                append_probe_log(
                                    &app,
                                    &id,
                                    &cfg,
                                    &format!("[探测 {}] 偶发失败（{}），继续观察", timestamp(), e),
                                );
                            }
                        }
                    }
                    entry.last_check = now;
                    // 顺手把这次探测记进统计：成功率、延迟曲线都从这里来
                    {
                        let mut sg = state.stats.lock().unwrap();
                        let st = sg.entry(id.clone()).or_default();
                        match ok_ms {
                            Some(us) => {
                                st.probes_ok += 1;
                                st.latency_us = if tracks_latency { Some(us) } else { None };
                                if tracks_latency {
                                    st.push_history(Some(us));
                                }
                            }
                            None => {
                                st.probes_fail += 1;
                                st.latency_us = None;
                                st.push_history(None);
                            }
                        }
                    }
                    if before != (entry.state, entry.latency_us) {
                        changed = true;
                    }
                }

                if changed {
                    let _ = app.emit("state-changed", ());
                }
            }

            if last_cleanup.elapsed() > Duration::from_secs(3600) {
                cleanup_old_logs(&app, &cfg);
                last_cleanup = Instant::now();
            }
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
        // 左键留给"弹主界面"，菜单只在右键出现
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
        })
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
        // 必须是第一个注册的插件：第二次启动实例要把命令行参数转交给已在运行的那个。
        // 「发送到」里一次选十个文件时，资源管理器可能拉起十个进程，
        // 没有这一层就会各拷一份、互相抢同一个目标文件。
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            match parse_sendto_args(&argv) {
                Some(req) => queue_sendto(app, req),
                // 普通的重复启动：把收在托盘里的窗口叫出来
                None => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.unminimize();
                        let _ = w.set_focus();
                    }
                }
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .setup(|app| {
            // 托盘失败不应阻止应用启动，只是失去"最小化到托盘"能力
            let _ = build_tray(app);

            // 载入落盘的流量账本（今日/累计都在里面）
            let ledger = store::load_traffic(app.handle());
            *app.state::<AppState>().traffic.lock().unwrap() = ledger;

            // 清掉上次被强杀残留的「发送到」菜单项（此时还没有任何挂载）
            sync_sendto_menu(app.handle());

            let argv: Vec<String> = std::env::args().collect();
            let pending = parse_sendto_args(&argv);

            // 静默启动：仅当「通过开机自启动拉起（--silent）」且设置勾选了静默时，
            // 才不弹主界面；手动双击图标永远显示窗口。
            // 由「发送到」拉起的进程也不弹主界面——用户要的是那个小确认窗，不是一个管理器。
            let silent_arg = argv.iter().any(|a| a.as_str() == "--silent");
            let silent = silent_arg && settings::load(app.handle()).silent_start;
            if !silent && pending.is_none() {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }

            // 本应用没在跑的时候点了「发送到」：文件和目标盘就在自己的命令行里
            if let Some(req) = pending {
                queue_sendto(app.handle(), req);
            }

            let handle = app.handle().clone();
            spawn_supervisor(handle.clone());
            spawn_health_monitor(handle.clone());
            spawn_stats_monitor(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 只有主窗口「关闭 = 收进托盘」；「发送到」小窗关掉就该销毁
                if window.label() != "main" {
                    return;
                }
                let state = window.app_handle().state::<AppState>();
                if state.tray_ready.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_env,
            default_mount_target,
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
            test_connection,
            check_updates,
            install_rclone,
            install_winfsp,
            open_url,
            hide_main_window,
            get_settings,
            save_settings,
            get_stats,
            reset_stats,
            probe_now,
            traffic_summary,
            check_target,
            notify_user,
            export_connections,
            import_connections,
            sendto_take,
            sendto_dirs,
            sendto_recent,
            sendto_run,
            sendto_cancel,
            sendto_window_ready,
            sendto_window_close,
            open_path
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
