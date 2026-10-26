import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isMock, mockInvoke, mockListen } from "./mock";

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
  created_at: number;
}

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
  latency_ms: number | null;
  last_check: number | null;
}

export interface DiagResult {
  ok: boolean;
  code: number | null;
  message: string;
  url: string;
  detail: string;
}

export interface EnvInfo {
  platform: string;
  app_version: string;
  rclone_path: string;
  rclone_version: string;
  rclone_source: string;
  rclone_bundled: boolean;
  fuse_ready: boolean;
  fuse_hint: string;
  fuse_version: string;
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

export const api = {
  env: () => call<EnvInfo>("get_env"),
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
  revealPassword: (id: string) => call<string | null>("reveal_password", { id }),
  openTarget: (id: string) => call<void>("open_target", { id }),
  getAutostart: () => call<boolean>("get_autostart"),
  setAutostart: (enable: boolean) => call<void>("set_autostart", { enable }),
  test: (input: ConnectionInput) => call<DiagResult>("test_connection", { input }),
  checkUpdates: () => call<UpdateReport>("check_updates"),
  installRclone: (version: string) => call<string>("install_rclone", { version }),
  installWinfsp: () => call<string>("install_winfsp"),
  openUrl: (url: string) => call<void>("open_url", { url }),
};

/** 后端主动推送的事件 */
export const events = {
  onStateChanged: (cb: () => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen("state-changed", () => cb()),
  onConnectionsChanged: (cb: () => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen("connections-changed", () => cb()),
  onTrayAction: (cb: (action: string) => void): Promise<UnlistenFn> =>
    isMock() ? mockListen() : listen<string>("tray-action", (e) => cb(e.payload)),
};

export const PROTOCOL_META: Record<
  Protocol,
  { label: string; short: string; port: number; hint: string; pathLabel: string; pathPlaceholder: string }
> = {
  smb: {
    label: "SMB / CIFS",
    short: "SMB",
    port: 445,
    hint: "主机名或 IP，例如 nas.local",
    pathLabel: "共享名",
    pathPlaceholder: "例如 home、video",
  },
  webdav: {
    label: "WebDAV",
    short: "DAV",
    port: 80,
    hint: "完整地址，例如 https://dav.example.com 或 nas.local:8080",
    pathLabel: "子路径",
    pathPlaceholder: "通常留空",
  },
  ftp: {
    label: "FTP",
    short: "FTP",
    port: 21,
    hint: "FTP 服务器地址",
    pathLabel: "子路径",
    pathPlaceholder: "通常留空",
  },
  sftp: {
    label: "SFTP",
    short: "SFTP",
    port: 22,
    hint: "SSH 服务器地址",
    pathLabel: "子路径",
    pathPlaceholder: "通常留空",
  },
  s3: {
    label: "S3 兼容对象存储",
    short: "S3",
    port: 443,
    hint: "服务地址，例如 s3.amazonaws.com 或 minio.example.com:9000",
    pathLabel: "Bucket（可选）",
    pathPlaceholder: "例如 photos",
  },
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

export function formatUptime(seconds: number | null): string {
  if (!seconds && seconds !== 0) return "—";
  if (seconds < 60) return `${seconds} 秒`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分钟`;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h} 小时 ${m} 分`;
}

/** 四色状态：返回样式类名与中文说明 */
export function healthOf(state: MountState | undefined): { cls: string; label: string } {
  if (!state || !state.mounted) {
    return state?.last_error
      ? { cls: "dot bad", label: "上次挂载失败" }
      : { cls: "dot off", label: "已停止" };
  }
  if (state.health === "error") return { cls: "dot bad", label: "连接异常" };
  if (state.health === "slow") return { cls: "dot warn", label: "网络波动" };
  return { cls: "dot ok", label: "运行中" };
}

/** 状态点的悬停提示，带上延迟数字更好判断 */
export function healthTip(state: MountState | undefined): string {
  if (!state) return "";
  if (!state.mounted) return state.last_error ? `上次挂载失败：${state.last_error.split("\n")[0]}` : "已停止";
  if (state.latency_ms != null) return `${healthOf(state).label} · 延迟 ${state.latency_ms} ms`;
  return healthOf(state).label;
}
