/**
 * 浏览器预览用的假数据。
 *
 * 只在「不在 Tauri 运行时里」时生效——`npm run dev` 直接用浏览器打开也能看界面，
 * 打包后的应用永远走真实的 Rust 后端。
 */
import type {
  Connection,
  ConnectionInput,
  DiagResult,
  EnvInfo,
  MountState,
  Stats,
} from "./api";

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
    s3_provider: "Other",
    region: "",
    s3_path_style: false,
    read_only: false,
    network_mode: false,
    bwlimit: "",
    cache_max_size: "10G",
    proxy: "",
    reconnect: true,
    auto_mount: true,
    pinned: true,
    group: "家庭",
    use_system_proxy: false,
    probe_mode: "light",
    extra_args: "",
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
    s3_provider: "Other",
    region: "",
    s3_path_style: false,
    read_only: true,
    network_mode: true,
    bwlimit: "20M",
    cache_max_size: "5G",
    proxy: "",
    reconnect: true,
    auto_mount: false,
    pinned: false,
    group: "办公",
    use_system_proxy: true,
    probe_mode: "light",
    extra_args: "",
    created_at: now - 100,
  },
  {
    id: "a3",
    name: "对象存储备份",
    protocol: "s3",
    host: "s3.example.com",
    port: 443,
    path: "backup",
    user: "AKIAEXAMPLE",
    target: "X:",
    cache_dir: "",
    ignore_cert: false,
    s3_provider: "Other",
    region: "us-east-1",
    s3_path_style: true,
    read_only: false,
    network_mode: false,
    bwlimit: "",
    cache_max_size: "10G",
    proxy: "",
    reconnect: false,
    auto_mount: false,
    pinned: false,
    group: "",
    use_system_proxy: false,
    probe_mode: "light",
    extra_args: "",
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
    health: "healthy",
    latency_us: 380,
    last_check: now - 6,
  },
  a2: {
    id: "a2",
    mounted: false,
    pid: null,
    since: null,
    restarts: 1,
    last_error: "2026/10/26 15:41:02 ERROR : SMB: login failed: STATUS_LOGON_FAILURE",
    log_path: "",
    health: "stopped",
    latency_us: null,
    last_check: null,
  },
  a3: {
    id: "a3",
    mounted: true,
    pid: 4780,
    since: 900,
    restarts: 2,
    last_error: null,
    log_path: "",
    health: "error",
    latency_us: null,
    last_check: now - 15,
  },
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
        app_version: "0.3.8",
        rclone_path: "C:\\Program Files\\Rclone Mount Manager\\resources\\rclone\\rclone.exe",
        rclone_version: "1.75.2",
        rclone_source: "bundled",
        rclone_bundled: true,
        fuse_ready: true,
        fuse_hint: "WinFsp 已就绪",
        fuse_version: "2.0.23075",
      } satisfies EnvInfo;
    case "list_connections":
      return list;
    case "mount_states":
      return list.map(
        (c) =>
          states[c.id] ?? {
            id: c.id,
            mounted: false,
            pid: null,
            since: null,
            restarts: 0,
            last_error: null,
            log_path: "",
            health: "stopped",
            latency_us: null,
            last_check: null,
          }
      );
    case "read_log":
      return LOG;
    case "probe_now":
      // 浏览器预览下假装跑了一次完整探测：42ms（真实环境是列一次目录的耗时）
      return 42_000;
    case "reset_stats":
      return null;
    case "get_stats":
      return {
        speed_bps: 1_843_200,
        peak_bps: 5_242_880,
        total_bytes: 734_003_200,
        transfers: 128,
        errors: 0,
        elapsed_secs: 1620,
        probes_ok: 96,
        probes_fail: 4,
        latency_us: 380,
        // 20 个点刚好是图表的窗口长度；单位微秒，两个 0 表示探测失败，一次 1.8ms 的尖峰
        history: [
          360, 420, 380, 0, 310, 460, 390, 1800, 340, 410, 350, 300, 390, 370, 0, 440, 360, 400, 380,
          350, 330, 410, 390, 380,
        ],
        up_bytes: 118_111_232,
        down_bytes: 615_891_968,
        today_up: 268_435_456,
        today_down: 1_342_177_280,
        total_up: 8_589_934_592,
        total_down: 42_949_672_960,
      } satisfies Stats;
    case "traffic_summary":
      return {
        today_up: 402_653_184,
        today_down: 2_254_857_830,
        total_up: 12_884_901_888,
        total_down: 68_719_476_736,
      };
    case "check_target": {
      // 预览环境里也做一次真实冲突检查：换了别的连接占用的盘符就能看到提示
      const t = String(args?.target ?? "").trim().toUpperCase();
      const exclude = String(args?.excludeId ?? "");
      const other = list.find((c) => c.id !== exclude && c.target.toUpperCase() === t);
      return other
        ? { available: false, reason: "other-connection", owner: other.name }
        : { available: true, reason: "", owner: "" };
    }
    case "notify_user":
      return null;
    case "get_autostart":
      return true;
    case "get_settings":
      return {
        language: "system",
        silent_start: true,
        probe_interval_secs: 15,
        latency_threshold_ms: 1200,
        warn_fails: 2,
        error_fails: 3,
        log_max_mb: 5,
        log_retention_days: 30,
        sort_by: "default",
        sort_dir: "asc",
        sendto_enabled: true,
      };
    case "save_settings":
      return args?.s;
    case "export_connections":
      // 浏览器预览下带口令就返回一个假的加密信封，方便看导入弹口令的交互
      return args?.password
        ? JSON.stringify({ app: "rclone-mount-manager", format: 2, encrypted: true, kdf: "pbkdf2-sha256", iterations: 120000, salt: "", nonce: "", ciphertext: "" })
        : JSON.stringify({ app: "rclone-mount-manager", format: 1, connections: list.map((c) => ({ ...c, password_obscured: "" })) });
    case "import_connections":
      return 2;
    case "check_updates":
      return {
        items: [
          {
            name: "管理器",
            current: "0.2.0",
            latest: "0.2.0",
            has_update: false,
            url: "https://github.com/Jie-baobao/rclone-mount-manager/releases",
            note: "",
          },
          {
            name: "rclone",
            current: "1.75.2",
            latest: "1.75.2",
            has_update: false,
            url: "https://rclone.org/downloads/",
            note: "",
          },
          {
            name: "WinFsp",
            current: "2.0.23075",
            latest: "2.1",
            has_update: true,
            url: "https://winfsp.dev/rel/",
            note: "",
          },
        ],
      };
    case "install_rclone":
      return "1.75.2";
    case "install_winfsp":
      return "WinFsp 安装完成";
    case "set_autostart":
    case "set_pinned":
    case "unmount":
    case "open_target":
    case "open_url":
    case "open_path":
    case "hide_main_window":
    case "sendto_run":
    case "sendto_cancel":
    case "sendto_window_ready":
    case "sendto_window_close":
    case "delete_connection":
      return null;
    // 浏览器预览里没有资源管理器入口，只有直接访问 ?sendto=1 时才给一份示例数据
    case "sendto_take":
      return new URLSearchParams(location.search).get("sendto") === "1"
        ? {
            target: "Z:",
            files: [
              "C:\\Users\\alice\\Videos\\旅行记录.mp4",
              "C:\\Users\\alice\\Videos\\封面.jpg",
            ],
          }
        : null;
    case "sendto_recent":
      return ["Z:\\电影", "Z:\\备份\\2024"];
    case "sendto_dirs": {
      const p = String(args?.path ?? "Z:\\");
      return [
        { name: "电影", path: `${p}电影` },
        { name: "剧集", path: `${p}剧集` },
        { name: "备份", path: `${p}备份` },
      ];
    }
    case "mount":
      return {
        id: (args?.id as string) ?? "",
        mounted: true,
        pid: 1,
        since: 0,
        restarts: 0,
        last_error: null,
        log_path: "",
        health: "healthy",
        latency_us: 380,
        last_check: now,
      } satisfies MountState;
    case "mount_all":
      return 1;
    case "unmount_all":
      return 2;
    case "reveal_password":
      return "";
    case "save_connection": {
      const input = args?.input as ConnectionInput;
      const existing = list.find((c) => c.id === input.id);
      const saved: Connection = {
        ...(existing ?? ({ id: String(Date.now()), created_at: Math.floor(Date.now() / 1000) } as Connection)),
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
      const ok = input.protocol === "webdav" || input.protocol === "s3";
      return {
        ok,
        code: input.protocol === "webdav" ? 207 : null,
        message: ok
          ? input.protocol === "s3"
            ? "服务地址可达。凭据与存储桶的校验会在挂载时完成（Bucket：backup）"
            : "认证通过，WebDAV 服务正常"
          : "端口连接失败：拒绝连接 (os error 10061)",
        url:
          input.protocol === "s3"
            ? `https://${input.host}`
            : `http://${input.host}:${input.port}/`,
        detail: ok ? 'WWW-Authenticate: Basic realm="Restricted"' : "",
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
