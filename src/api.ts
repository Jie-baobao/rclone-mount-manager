import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isMock, mockInvoke, mockListen } from "./mock";
import type { T } from "./i18n";

/** 在 Tauri 里走真实后端；直接用浏览器打开时走 mock，方便调界面 */
function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return isMock() ? mockInvoke<T>(cmd, args) : invoke<T>(cmd, args);
}

export type Protocol = "smb" | "webdav" | "ftp" | "sftp" | "s3";

/** 圆点颜色语义 */
export type Health = "healthy" | "slow" | "error" | "stopped";

export interface Connection {
  id: string;
  name: string;
  protocol: Protocol;
  host: string;
  port: number;
  path: string;
  user: string;
  target: string;
  cache_dir: string;
  ignore_cert: boolean;
  s3_provider: string;
  region: string;
  s3_path_style: boolean;
  read_only: boolean;
  network_mode: boolean;
  bwlimit: string;
  cache_max_size: string;
  proxy: string;
  reconnect: boolean;
  auto_mount: boolean;
  pinned: boolean;
  /** 分组名（可空）；挂载时拼进卷标：[组名] 名称 */
  group: string;
  /** 挂载与探测走系统代理 */
  use_system_proxy: boolean;
  /** 探测方式：full / light（默认，不碰盘）/ process */
  probe_mode: ProbeMode;
  /** 追加给 rclone mount 的自定义参数（命令行原文，例如 `--transfers 4`） */
  extra_args: string;
  created_at: number;
}

export type ProbeMode = "full" | "light" | "process";

export interface ConnectionInput {
  id?: string | null;
  name: string;
  protocol: Protocol;
  host: string;
  port?: number | null;
  path: string;
  user: string;
  password: string;
  target: string;
  cache_dir: string;
  ignore_cert: boolean;
  s3_provider: string;
  region: string;
  s3_path_style: boolean;
  read_only: boolean;
  network_mode: boolean;
  bwlimit: string;
  cache_max_size: string;
  proxy: string;
  reconnect: boolean;
  auto_mount: boolean;
  pinned: boolean;
  group: string;
  use_system_proxy: boolean;
  probe_mode: ProbeMode;
  extra_args: string;
}

export interface MountState {
  id: string;
  mounted: boolean;
  pid: number | null;
  since: number | null;
  restarts: number;
  last_error: string | null;
  log_path: string;
  health: Health;
  /** 最近一次探测耗时，单位微秒（轻量探测的局域网握手常在 1ms 以内） */
  latency_us: number | null;
  last_check: number | null;
}

/**
 * 延迟数字的「口径」：轻量探测量的是 TCP 握手往返，完整探测量的是列一次目录的耗时，
 * 两者差一个数量级，展示时必须标清楚，不然用户会以为探测量错了。
 */
export function probeKindLabel(mode: ProbeMode | undefined, t: T): string {
  if (mode === "full") return t("probe.kindFull");
  if (mode === "process") return t("probe.kindProcess");
  return t("probe.kindTcp");
}

/**
 * 把微秒格式化成好读的延迟文本。
 * 亚毫秒保留两位（0.42 ms），10ms 以上取整，超过 1 秒换成秒。
 */
export function fmtLatency(us: number | null | undefined): string {
  if (us == null) return "—";
  if (us < 1_000) return `${(us / 1000).toFixed(2)} ms`;
  if (us < 10_000) return `${(us / 1000).toFixed(1)} ms`;
  if (us < 1_000_000) return `${Math.round(us / 1000)} ms`;
  return `${(us / 1_000_000).toFixed(1)} s`;
}

/**
 * 把字节数格式化成好读的文本：512 B / 1.2 MB / 3.4 GB。
 * 统计卡、流量条、日志都用同一套口径，免得同一张界面上出现两种换行规则。
 */
export function fmtBytes(n: number): string {
  if (!n || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  const digits = i === 0 ? 0 : v >= 100 ? 0 : 1;
  return `${v.toFixed(digits)} ${units[i]}`;
}

/**
 * 本次挂载的统计数据（后端每 5 秒从 rclone 的本机 rc 端口采一次，只在内存里）。
 *
 * 口径提醒：rclone 的 core/stats 只有「收 + 发」合计的字节数，不区分上下行；
 * 而且它是**挂载开始以来的累计值**，含重复读取、重试与预取，所以 total_bytes
 * 不等于「复制了多少文件」。想单独量一次拷贝，先点「清零重计」再操作。
 */
export interface Stats {
  speed_bps: number;
  peak_bps: number;
  total_bytes: number;
  transfers: number;
  errors: number;
  elapsed_secs: number;
  probes_ok: number;
  probes_fail: number;
  latency_us: number | null;
  /** 最近 40 次探测延迟（微秒），0 表示这次失败 */
  history: number[];
  /** 本次挂载的上下行字节（方向按「是否有活跃上传」推断，属近似口径） */
  up_bytes: number;
  down_bytes: number;
  /** 今天 / 累计的上下行（落盘账本，重启后仍在） */
  today_up: number;
  today_down: number;
  total_up: number;
  total_down: number;
}

/** 首页概览用的全部连接流量合计 */
export interface TrafficSummary {
  today_up: number;
  today_down: number;
  total_up: number;
  total_down: number;
}

/** 盘符/挂载点占用检查结果 */
export interface TargetCheck {
  available: boolean;
  /** "" / "other-connection" / "occupied" */
  reason: string;
  /** 占用它的连接名 */
  owner: string;
}

export interface DiagResult {
  ok: boolean;
  code: number | null;
  message: string;
  url: string;
  detail: string;
}

export interface MountDriver {
  /** 平台实际的挂载驱动名：WinFsp / macFUSE / FUSE-T / FUSE 3 / FUSE */
  name: string;
  /** 驱动版本（Linux 上是 libfuse 版本），探测不到时为空串 */
  version: string;
  ready: boolean;
  /** 未就绪时的安装指引（后端给的，可能只有中文） */
  hint: string;
  /** 能否在应用内一键安装（只有 Windows 的 WinFsp 可以） */
  installable: boolean;
  url: string;
}

export interface EnvInfo {
  platform: string;
  app_version: string;
  rclone_path: string;
  rclone_version: string;
  rclone_source: string;
  rclone_bundled: boolean;
  driver: MountDriver;
}

export interface VersionInfo {
  name: string;
  current: string;
  latest: string;
  has_update: boolean;
  url: string;
  note: string;
}

export interface UpdateReport {
  items: VersionInfo[];
}

export interface Settings {
  language: string;
  silent_start: boolean;
  probe_interval_secs: number;
  latency_threshold_ms: number;
  warn_fails: number;
  error_fails: number;
  log_max_mb: number;
  log_retention_days: number;
  /** 连接列表排序：default / name / status / group */
  sort_by: string;
  /** asc / desc */
  sort_dir: string;
  /** 在资源管理器「发送到」菜单里为已挂载的连接生成入口（Windows，默认开） */
  sendto_enabled: boolean;
}

/** 「发送到」请求：资源管理器点菜单时传进来的目标盘符和文件 */
export interface SendRequest {
  target: string;
  files: string[];
}

export interface DirEntry {
  name: string;
  path: string;
}

/** 传输进度事件 */
export interface SendProgress {
  /** planning / copying / done */
  phase: string;
  done_bytes: number;
  total_bytes: number;
  done_files: number;
  total_files: number;
  current: string;
}

/** 传输结果事件 */
export interface SendReport {
  ok: boolean;
  /** copy / move */
  mode: string;
  dest: string;
  bytes: number;
  copied: number;
  skipped: number;
  failed: number;
  details: string[];
  error: string;
}

export const api = {
  env: () => call<EnvInfo>("get_env"),
  /** 新建连接时的默认挂载目标：Windows 是盘符，unix 是家目录下的路径（不需要管理员权限） */
  defaultMountTarget: () => call<string>("default_mount_target"),
  list: () => call<Connection[]>("list_connections"),
  save: (input: ConnectionInput) => call<Connection>("save_connection", { input }),
  remove: (id: string) => call<void>("delete_connection", { id }),
  pin: (id: string, pinned: boolean) => call<void>("set_pinned", { id, pinned }),
  mount: (id: string) => call<MountState>("mount", { id }),
  unmount: (id: string) => call<void>("unmount", { id }),
  mountAll: () => call<number>("mount_all"),
  unmountAll: () => call<number>("unmount_all"),
  states: () => call<MountState[]>("mount_states"),
  readLog: (id: string) => call<string>("read_log", { id }),
  getStats: (id: string) => call<Stats>("get_stats", { id }),
  /** 全部连接的今日/累计上传下载合计（首页概览） */
  trafficSummary: () => call<TrafficSummary>("traffic_summary"),
  /** 盘符/挂载点预检：别的连接或系统占用了就返回 available=false */
  checkTarget: (target: string, excludeId: string) =>
    call<TargetCheck>("check_target", { target, excludeId }),
  /** 把本次挂载的流量计数清零（峰值、累计、次数），便于单独量一次拷贝 */
  resetStats: (id: string) => call<void>("reset_stats", { id }),
  /** 手动做一次完整探测（真实列目录），返回微秒；会让 NAS 读一次目录 */
  probeNow: (id: string) => call<number>("probe_now", { id }),
  revealPassword: (id: string) => call<string | null>("reveal_password", { id }),
  openTarget: (id: string) => call<void>("open_target", { id }),
  /** 直接在系统文件管理器里打开任意目录 */
  openPath: (path: string) => call<void>("open_path", { path }),
  getAutostart: () => call<boolean>("get_autostart"),
  setAutostart: (enable: boolean) => call<void>("set_autostart", { enable }),
  test: (input: ConnectionInput) => call<DiagResult>("test_connection", { input }),
  checkUpdates: () => call<UpdateReport>("check_updates"),
  installRclone: (version: string) => call<string>("install_rclone", { version }),
  installWinfsp: () => call<string>("install_winfsp"),
  openUrl: (url: string) => call<void>("open_url", { url }),
  /** 弹系统通知（标题/正文由前端按当前语言拼好） */
  notifyUser: (title: string, body: string) => call<void>("notify_user", { title, body }),
  /** Ctrl+W：关闭主窗口到后台（托盘可用时隐藏，否则最小化），挂载进程不受影响 */
  hideWindow: () => call<void>("hide_main_window"),
  getSettings: () => call<Settings>("get_settings"),
  saveSettings: (s: Settings) => call<Settings>("save_settings", { s }),
  exportConnections: (password?: string) =>
    call<string>("export_connections", { password: password ?? null }),
  importConnections: (data: string, password?: string) =>
    call<number>("import_connections", { data, password: password ?? null }),

  // ---- 「发送到」菜单 ----
  /** 取走待处理的发送请求（应用刚启动、界面还没挂好时可能错过事件） */
  sendtoTake: () => call<SendRequest | null>("sendto_take"),
  /** 列某个目录下的子目录，给「选目标位置」用 */
  sendtoDirs: (path: string) => call<DirEntry[]>("sendto_dirs", { path }),
  /** 某个盘最近发送到过的目录 */
  sendtoRecent: (target: string) => call<string[]>("sendto_recent", { target }),
  /** 开始复制/移动（后台跑，进度和结果走事件） */
  sendtoRun: (sources: string[], dest: string, target: string, mode: "copy" | "move") =>
    call<void>("sendto_run", { sources, dest, target, mode }),
  sendtoCancel: () => call<void>("sendto_cancel"),
  /** 「发送到」小窗首帧画好，可以显示了（后端先隐藏建窗，避免白闪） */
  sendtoWindowReady: () => call<void>("sendto_window_ready"),
  /** 关掉「发送到」小窗本身（区别于主窗口的「关闭 = 收进托盘」） */
  sendtoWindowClose: () => call<void>("sendto_window_close"),
};

/** 后端主动推送的事件 */
export const events = {
  onStateChanged: (cb: () => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen("state-changed", () => cb()),
  onConnectionsChanged: (cb: () => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen("connections-changed", () => cb()),
  onTrayAction: (cb: (action: string) => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen<string>("tray-action", (e) => cb(e.payload)),
  /** 后端要求弹系统通知（挂载失败 / 重连放弃），文案由前端本地化 */
  onNotify: (cb: (payload: NotifyPayload) => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen<NotifyPayload>("notify", (e) => cb(e.payload)),
  /** 收到「发送到」请求（资源管理器点了菜单项） */
  onSendtoRequest: (cb: (req: SendRequest) => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen<SendRequest>("sendto-request", (e) => cb(e.payload)),
  onSendtoProgress: (cb: (p: SendProgress) => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen<SendProgress>("sendto-progress", (e) => cb(e.payload)),
  onSendtoDone: (cb: (r: SendReport) => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen<SendReport>("sendto-done", (e) => cb(e.payload)),
};

/** 后端推送的通知请求 */
export interface NotifyPayload {
  /** mount-failed（手动挂载失败）/ gave-up（自动重连放弃） */
  kind: "mount-failed" | "gave-up";
  id: string;
  name: string;
  reason: string;
}

/** 协议元信息：label/short 各语言通用；hint/pathLabel/pathPlaceholder 走 i18n（pm.<code>.*） */
export const PROTOCOL_META: Record<Protocol, { label: string; short: string; port: number }> = {
  smb: { label: "SMB / CIFS", short: "SMB", port: 445 },
  webdav: { label: "WebDAV", short: "DAV", port: 80 },
  ftp: { label: "FTP", short: "FTP", port: 21 },
  sftp: { label: "SFTP", short: "SFTP", port: 22 },
  s3: { label: "S3", short: "S3", port: 443 },
};

/** S3 兼容服务商预设：provider 是 rclone 的取值，endpoint 是官方地址示例 */
export interface S3Preset {
  key: string;
  label: string;
  provider: string;
  endpoint: string;
  region: string;
  pathStyle: boolean;
}

export const S3_PRESETS: S3Preset[] = [
  { key: "aws", label: "Amazon S3", provider: "AWS", endpoint: "s3.amazonaws.com", region: "us-east-1", pathStyle: false },
  { key: "aliyun", label: "阿里云 OSS", provider: "Alibaba", endpoint: "oss-cn-hangzhou.aliyuncs.com", region: "cn-hangzhou", pathStyle: false },
  { key: "tencent", label: "腾讯云 COS", provider: "TencentCOS", endpoint: "cos.ap-guangzhou.myqcloud.com", region: "ap-guangzhou", pathStyle: false },
  { key: "r2", label: "Cloudflare R2", provider: "Cloudflare", endpoint: "https://<账号ID>.r2.cloudflarestorage.com", region: "auto", pathStyle: false },
  { key: "b2", label: "Backblaze B2", provider: "B2", endpoint: "s3.us-west-004.backblazeb2.com", region: "us-west-004", pathStyle: false },
  { key: "wasabi", label: "Wasabi", provider: "Wasabi", endpoint: "s3.us-east-1.wasabisys.com", region: "us-east-1", pathStyle: false },
  { key: "do", label: "DigitalOcean Spaces", provider: "DigitalOcean", endpoint: "nyc3.digitaloceanspaces.com", region: "nyc3", pathStyle: false },
  { key: "minio", label: "MinIO / 自建", provider: "Minio", endpoint: "minio.example.com:9000", region: "us-east-1", pathStyle: true },
  { key: "other", label: "其它 S3 兼容", provider: "Other", endpoint: "", region: "", pathStyle: true },
];

export function isWindows(env: EnvInfo | null): boolean {
  return !!env && env.platform.startsWith("windows");
}

export function formatUptimeT(seconds: number | null, t: T): string {
  if (!seconds && seconds !== 0) return "—";
  if (seconds < 60) return t("u.sec", { n: seconds });
  if (seconds < 3600) return t("u.min", { n: Math.floor(seconds / 60) });
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return t("u.hour", { h, m });
}

/** 四色状态：返回样式类名与本地化说明 */
export function healthOf(state: MountState | undefined, t: T): { cls: string; label: string } {
  if (!state || !state.mounted) {
    return state?.last_error
      ? { cls: "dot bad", label: t("h.lastFail") }
      : { cls: "dot off", label: t("h.stopped") };
  }
  if (state.health === "error") return { cls: "dot bad", label: t("h.error") };
  if (state.health === "slow") return { cls: "dot warn", label: t("h.slow") };
  return { cls: "dot ok", label: t("h.ok") };
}

/** 状态点的悬停提示，带上延迟数字更好判断 */
export function healthTip(state: MountState | undefined, t: T): string {
  if (!state) return "";
  if (!state.mounted) return state.last_error ? `${t("h.lastFail")} · ${state.last_error.split("\n")[0]}` : t("h.stopped");
  if (state.latency_us != null) return `${healthOf(state, t).label} · ${fmtLatency(state.latency_us)}`;
  return healthOf(state, t).label;
}
