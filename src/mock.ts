/**
 * 浏览器预览用的假数据。
 *
 * 只在「不在 Tauri 运行时里」时生效——`npm run dev` 直接用浏览器打开也能看界面，
 * 打包后的应用永远走真实的 Rust 后端。
 */
import type { Connection, ConnectionInput, DiagResult, EnvInfo, MountState } from "./api";

const now = Math.floor(Date.now() / 1000);

const conns: Connection[] = [
  {
    id: "a1",
    name: "家里 NAS（WebDAV）",
    protocol: "webdav",
    host: "nas.local",
    port: 8080,
    path: "",
    user: "alice",
    target: "Z:",
    cache_dir: "",
    ignore_cert: false,
    read_only: false,
    network_mode: false,
    bwlimit: "",
    cache_max_size: "10G",
    proxy: "",
    reconnect: true,
    auto_mount: true,
    pinned: true,
    created_at: now,
  },
  {
    id: "a2",
    name: "办公室共享盘",
    protocol: "smb",
    host: "fileserver.local",
    port: 445,
    path: "public",
    user: "work",
    target: "Y:",
    cache_dir: "",
    ignore_cert: false,
    read_only: true,
    network_mode: true,
    bwlimit: "20M",
    cache_max_size: "5G",
    proxy: "",
    reconnect: true,
    auto_mount: false,
    pinned: false,
    created_at: now - 100,
  },
  {
    id: "a3",
    name: "备份服务器",
    protocol: "sftp",
    host: "backup.example.com",
    port: 22,
    path: "data",
    user: "root",
    target: "X:",
    cache_dir: "",
    ignore_cert: false,
    read_only: false,
    network_mode: false,
    bwlimit: "",
    cache_max_size: "10G",
    proxy: "http://127.0.0.1:7890",
    reconnect: false,
    auto_mount: false,
    pinned: false,
    created_at: now - 200,
  },
];

const states: Record<string, MountState> = {
  a1: {
    id: "a1",
    mounted: true,
    pid: 4242,
    since: 3725,
    restarts: 0,
    last_error: null,
    log_path: "",
  },
  a2: {
    id: "a2",
    mounted: false,
    pid: null,
    since: null,
    restarts: 1,
    last_error: "2026/10/26 15:41:02 ERROR : SMB: login failed: STATUS_LOGON_FAILURE",
    log_path: "",
  },
  a3: { id: "a3", mounted: false, pid: null, since: null, restarts: 0, last_error: null, log_path: "" },
};

const LOG = `2026/10/26 15:32:11 INFO  : Using config file from config/rclone.conf
2026/10/26 15:32:11 INFO  : webdav root '': --vfs-cache-mode writes
2026/10/26 15:32:11 NOTICE: Serving remote control on http://127.0.0.1:5572/
2026/10/26 15:32:11 INFO  : vfs cache: cleaned: objects 0 (was 0) in use 0, to upload 0, uploading 0, total size 0 (was 0)
2026/10/26 15:32:12 INFO  : webdav root '': poll-interval is not supported by this remote
2026/10/26 15:32:12 NOTICE: webdav root '': poll-interval is not supported by this remote
2026/10/26 15:33:40 INFO  : 示例文件.xlsx: vfs cache: downloader: queued
2026/10/26 15:33:41 INFO  : 示例文件.xlsx: Copied (new)
2026/10/26 15:34:02 INFO  : vfs cache: cleaned: objects 1 in use 0, to upload 0, uploading 0, total size 128.4Ki`;

let list = [...conns];

function reply(cmd: string, args?: Record<string, unknown>): unknown {
  switch (cmd) {
    case "get_env":
      return {
        platform: "windows x86_64",
        rclone_path: "C:\\Program Files\\Rclone\\rclone.exe",
        rclone_version: "rclone v1.75.2",
        fuse_ready: true,
        fuse_hint: "WinFsp 已就绪",
      } satisfies EnvInfo;
    case "list_connections":
      return list;
    case "mount_states":
      return list.map((c) => states[c.id] ?? { id: c.id, mounted: false, pid: null, since: null, restarts: 0, last_error: null, log_path: "" });
    case "read_log":
      return LOG;
    case "get_autostart":
      return true;
    case "set_autostart":
    case "set_pinned":
    case "unmount":
    case "open_target":
    case "delete_connection":
      return null;
    case "mount":
      return { id: (args?.id as string) ?? "", mounted: true, pid: 1, since: 0, restarts: 0, last_error: null, log_path: "" };
    case "mount_all":
      return 1;
    case "unmount_all":
      return 2;
    case "reveal_password":
      return "";
    case "save_connection": {
      const input = args?.input as ConnectionInput;
      const saved: Connection = {
        ...(list.find((c) => c.id === input.id) ?? ({
          id: String(Date.now()),
          created_at: Math.floor(Date.now() / 1000),
        } as Connection)),
        ...input,
        id: input.id ?? String(Date.now()),
        password: undefined,
      } as unknown as Connection;
      const idx = list.findIndex((c) => c.id === saved.id);
      if (idx >= 0) list[idx] = saved;
      else list = [saved, ...list];
      return saved;
    }
    case "test_connection": {
      const input = args?.input as ConnectionInput;
      const ok = input.protocol === "webdav";
      return {
        ok,
        code: ok ? 207 : null,
        message: ok ? "认证通过，WebDAV 服务正常" : "端口连接失败：拒绝连接 (os error 10061)",
        url: `http://${input.host}:${input.port}/`,
        detail: ok ? "WWW-Authenticate: Basic realm=\"Restricted\"" : "",
      } satisfies DiagResult;
    }
    default:
      return null;
  }
}

export function isMock(): boolean {
  return !("__TAURI_INTERNALS__" in window);
}

export async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  await new Promise((r) => setTimeout(r, 120));
  return reply(cmd, args) as T;
}

export const mockListen = async () => () => undefined;
