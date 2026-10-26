import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isMock, mockInvoke, mockListen } from "./mock";

/** 在 Tauri 里走真实后端；直接用浏览器打开时走 mock，方便调界面 */
function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return isMock() ? mockInvoke<T>(cmd, args) : invoke<T>(cmd, args);
}

export type Protocol = "smb" | "webdav" | "ftp" | "sftp";

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
  rclone_path: string;
  rclone_version: string;
  fuse_ready: boolean;
  fuse_hint: string;
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
};

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
