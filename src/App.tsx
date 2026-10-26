import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ConnectionForm from "./ConnectionForm";
import {
  api,
  events,
  formatUptime,
  healthOf,
  healthTip,
  isWindows,
  PROTOCOL_META,
  type Connection,
  type EnvInfo,
  type MountState,
  type UpdateReport,
} from "./api";

interface Toast {
  kind: "ok" | "bad";
  text: string;
}

export default function App() {
  const [env, setEnv] = useState<EnvInfo | null>(null);
  const [conns, setConns] = useState<Connection[]>([]);
  const [states, setStates] = useState<Record<string, MountState>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [editing, setEditing] = useState<Connection | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [log, setLog] = useState("");
  const [autostart, setAutostart] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const [busy, setBusy] = useState<string>("");
  const toastTimer = useRef<number | null>(null);

  const notify = useCallback((kind: "ok" | "bad", text: string) => {
    setToast({ kind, text });
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 4500);
  }, []);

  const refreshEnv = useCallback(() => {
    api.env().then(setEnv).catch(() => setEnv(null));
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [list, st] = await Promise.all([api.list(), api.states()]);
      setConns(list);
      setStates(Object.fromEntries(st.map((s) => [s.id, s])));
      setSelected((cur) => (cur && list.some((c) => c.id === cur) ? cur : list[0]?.id ?? null));
    } catch (e) {
      notify("bad", `读取连接失败：${e}`);
    }
  }, [notify]);

  // 初次加载
  useEffect(() => {
    refreshEnv();
    api.getAutostart().then(setAutostart).catch(() => undefined);
    refresh();
  }, [refresh, refreshEnv]);

  // 后端事件驱动的实时刷新（比轮询更跟手），再补一个低频兜底轮询
  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    events.onStateChanged(() => {
      api.states().then((st) => setStates(Object.fromEntries(st.map((s) => [s.id, s]))));
    }).then((un) => unlisteners.push(un));
    events.onConnectionsChanged(() => {
      refresh();
    }).then((un) => unlisteners.push(un));
    events.onTrayAction((action) => {
      if (action === "mount_all") void handleMountAll();
      if (action === "unmount_all") void handleUnmountAll();
    }).then((un) => unlisteners.push(un));

    const timer = window.setInterval(() => {
      api
        .states()
        .then((st) => setStates(Object.fromEntries(st.map((s) => [s.id, s]))))
        .catch(() => undefined);
    }, 8000);

    return () => {
      unlisteners.forEach((un) => un());
      window.clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refresh]);

  const current = useMemo(() => conns.find((c) => c.id === selected) ?? null, [conns, selected]);
  const currentState = current ? states[current.id] : undefined;
  const mountedCount = useMemo(() => Object.values(states).filter((s) => s.mounted).length, [states]);

  const loadLog = useCallback(async (id: string) => {
    try {
      setLog(await api.readLog(id));
    } catch {
      setLog("");
    }
  }, []);

  // 挂载中时自动刷新日志，便于看实时进度
  useEffect(() => {
    if (!current) {
      setLog("");
      return;
    }
    void loadLog(current.id);
    if (!currentState?.mounted) return;
    const timer = window.setInterval(() => void loadLog(current.id), 4000);
    return () => window.clearInterval(timer);
  }, [current, currentState?.mounted, loadLog]);

  // 快捷键：Ctrl+N 新建，Ctrl+R 刷新
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        setEditing(null);
        setFormOpen(true);
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "r") {
        e.preventDefault();
        void refresh();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [refresh]);

  async function handleMount(c: Connection) {
    setBusy(c.id);
    try {
      await api.mount(c.id);
      notify("ok", `${c.name} 已挂载到 ${c.target}`);
    } catch (e) {
      notify("bad", String(e));
    } finally {
      setBusy("");
      await refresh();
    }
  }

  async function handleUnmount(c: Connection) {
    setBusy(c.id);
    try {
      await api.unmount(c.id);
      notify("ok", `${c.name} 已卸载`);
    } catch (e) {
      notify("bad", String(e));
    } finally {
      setBusy("");
      await refresh();
    }
  }

  async function handleMountAll() {
    try {
      const n = await api.mountAll();
      notify("ok", n > 0 ? `已发起 ${n} 个挂载` : "没有需要挂载的连接");
    } catch (e) {
      notify("bad", String(e));
    } finally {
      await refresh();
    }
  }

  async function handleUnmountAll() {
    try {
      const n = await api.unmountAll();
      notify("ok", `已卸载 ${n} 个连接`);
    } catch (e) {
      notify("bad", String(e));
    } finally {
      await refresh();
    }
  }

  async function handleDelete(c: Connection) {
    if (!window.confirm(`确定删除连接「${c.name}」？配置和已保存的密码会一并移除。`)) return;
    setBusy(c.id);
    try {
      await api.remove(c.id);
      notify("ok", "已删除");
      await refresh();
    } catch (e) {
      notify("bad", String(e));
    } finally {
      setBusy("");
    }
  }

  async function toggleAutostart(next: boolean) {
    try {
      await api.setAutostart(next);
      setAutostart(next);
      notify("ok", next ? "已设置开机自启动" : "已关闭开机自启动");
    } catch (e) {
      notify("bad", String(e));
    }
  }

  const win = isWindows(env);
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return conns;
    return conns.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        c.host.toLowerCase().includes(q) ||
        c.target.toLowerCase().includes(q) ||
        c.user.toLowerCase().includes(q)
    );
  }, [conns, query]);

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="logo">◇</span>
          <div>
            <strong>Rclone Mount</strong>
            <small>{mountedCount > 0 ? `${mountedCount} 个已挂载` : "管理器"}</small>
          </div>
        </div>

        <button
          className="btn primary block"
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
        >
          + 新建连接
          <kbd>Ctrl N</kbd>
        </button>

        <input
          className="ctl search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索名称 / 地址 / 盘符 / 用户"
        />

        <ul className="conn-list">
          {filtered.map((c) => {
            const st = states[c.id];
            const h = healthOf(st);
            const busyThis = busy === c.id;
            return (
              <li
                key={c.id}
                className={`conn ${selected === c.id ? "active" : ""}`}
                onClick={() => setSelected(c.id)}
                title={`${c.host}${c.protocol === "s3" ? "" : `:${c.port}`}`}
              >
                <span className={h.cls} title={healthTip(st)} />
                <div className="conn-meta">
                  <span className="conn-name">
                    {c.pinned && <span className="pin">★</span>}
                    {c.name}
                  </span>
                  <span className="conn-sub">
                    <span className="tag">{PROTOCOL_META[c.protocol].short}</span>
                    <span className="tag mono">{c.target}</span>
                    <span className="conn-user" title={c.user || "匿名"}>
                      {c.user || "匿名"}
                    </span>
                  </span>
                </div>
                <button
                  className={`row-toggle ${st?.mounted ? "stop" : "start"}`}
                  disabled={busyThis}
                  title={st?.mounted ? "停止挂载" : "启动挂载"}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (busyThis) return;
                    if (st?.mounted) void handleUnmount(c);
                    else void handleMount(c);
                  }}
                >
                  {busyThis ? "…" : st?.mounted ? "停止" : "启动"}
                </button>
              </li>
            );
          })}
          {filtered.length === 0 && (
            <li className="empty">{conns.length === 0 ? "还没有连接，点上面新建一个" : "没有匹配的连接"}</li>
          )}
        </ul>

        <div className="bulk">
          <button className="btn ghost small" onClick={handleMountAll}>
            全部启动
          </button>
          <button className="btn ghost small" onClick={handleUnmountAll}>
            全部停止
          </button>
        </div>

        <div className="sidebar-foot">
          <label className="check">
            <input type="checkbox" checked={autostart} onChange={(e) => toggleAutostart(e.target.checked)} />
            开机自启动
          </label>
          <button className="env-card" onClick={() => setAboutOpen(true)} title="查看版本与依赖状态">
            <span className={env?.fuse_ready && env?.rclone_version ? "env ok" : "env bad"}>
              {env ? (env.fuse_ready && env.rclone_version ? "● 环境就绪" : "○ 环境缺依赖") : "正在检测环境…"}
            </span>
            <span className="env-ver mono">
              {env
                ? `rclone ${env.rclone_version || "—"} · ${env.fuse_version ? `WinFsp ${env.fuse_version}` : "驱动—"}`
                : ""}
            </span>
          </button>
        </div>
      </aside>

      <main className="main">
        {!current && (
          <div className="placeholder">
            <h2>把 NAS 挂成本地盘</h2>
            <p>三步：填连接 → 测通 → 挂载。不需要账号登录，断网也能用。</p>
            <ol>
              <li>选协议（SMB / WebDAV / FTP / SFTP / S3 兼容），填地址和账号密码</li>
              <li>点「测试连接」，先确认认证通过（WebDAV 会返回真实状态码）</li>
              <li>选盘符保存，点「挂载」，资源管理器里就出现了</li>
            </ol>
            <p className="muted small">提示：按 Ctrl+N 随时新建连接。</p>
          </div>
        )}

        {current && (
          <>
            <header className="detail-head">
              <div className="title">
                <h2>
                  {current.name}
                  {current.pinned && <span className="pin">★</span>}
                </h2>
                <p className="muted mono">
                  {PROTOCOL_META[current.protocol].label} · {current.host}
                  {current.protocol === "s3" ? "" : `:${current.port}`}
                  {current.path ? ` / ${current.path}` : ""}
                </p>
              </div>
              <div className="head-actions">
                <span className={`badge ${currentState?.mounted ? "on" : ""}`}>
                  {currentState?.mounted
                    ? `${healthOf(currentState).label} ${current.target}${
                        currentState.latency_ms != null ? ` · ${currentState.latency_ms}ms` : ""
                      }`
                    : "未挂载"}
                </span>
                <button
                  className="btn ghost small"
                  onClick={() => api.pin(current.id, !current.pinned).then(refresh)}
                  title={current.pinned ? "取消置顶" : "置顶"}
                >
                  {current.pinned ? "取消置顶" : "置顶"}
                </button>
              </div>
            </header>

            {currentState?.last_error && !currentState.mounted && (
              <div className="alert">
                <strong>上次挂载失败</strong>
                <pre>{currentState.last_error}</pre>
              </div>
            )}

            <section className="panel">
              <div className="kv">
                <div>
                  <label>目标位置</label>
                  <span className="mono">{current.target}</span>
                </div>
                <div>
                  <label>{current.protocol === "s3" ? "Access Key" : "用户名"}</label>
                  <span className="ellipsis" title={current.user}>
                    {current.user || "（匿名）"}
                  </span>
                </div>
                <div>
                  <label>已运行</label>
                  <span>{currentState?.mounted ? formatUptime(currentState.since) : "—"}</span>
                </div>
                <div>
                  <label>连接质量</label>
                  <span>
                    {currentState?.mounted
                      ? currentState.latency_ms != null
                        ? `${healthOf(currentState).label}（${currentState.latency_ms} ms）`
                        : healthOf(currentState).label
                      : "—"}
                  </span>
                </div>
              </div>

              <div className="actions">
                {currentState?.mounted ? (
                  <button className="btn" disabled={busy === current.id} onClick={() => handleUnmount(current)}>
                    停止挂载
                  </button>
                ) : (
                  <button className="btn primary" disabled={busy === current.id} onClick={() => handleMount(current)}>
                    {busy === current.id ? "挂载中…" : "启动挂载"}
                  </button>
                )}
                <button
                  className="btn ghost"
                  disabled={!currentState?.mounted}
                  onClick={() => api.openTarget(current.id).catch((e) => notify("bad", String(e)))}
                >
                  在{win ? "资源管理器" : "文件管理器"}中打开
                </button>
                <button
                  className="btn ghost"
                  onClick={() => {
                    setEditing(current);
                    setFormOpen(true);
                  }}
                >
                  编辑
                </button>
                <button className="btn danger" disabled={busy === current.id} onClick={() => handleDelete(current)}>
                  删除
                </button>
              </div>
            </section>

            <details className="panel details">
              <summary>高级设置</summary>
              <div className="kv">
                <div>
                  <label>只读挂载</label>
                  <span>{current.read_only ? "是（不可写）" : "否"}</span>
                </div>
                <div>
                  <label>挂载形态</label>
                  <span>{win ? (current.network_mode ? "网络驱动器" : "本地磁盘") : "挂载点"}</span>
                </div>
                <div>
                  <label>带宽限制</label>
                  <span>{current.bwlimit || "不限"}</span>
                </div>
                <div>
                  <label>缓存上限</label>
                  <span>{current.cache_max_size || "默认"}</span>
                </div>
                <div>
                  <label>代理</label>
                  <span className="mono">{current.proxy || "未设置"}</span>
                </div>
                <div>
                  <label>忽略证书校验</label>
                  <span>{current.ignore_cert ? "是" : "否"}</span>
                </div>
              </div>
            </details>

            <section className="panel grow">
              <div className="panel-head">
                <h3>挂载日志</h3>
                <div className="panel-tools">
                  <span className="muted small">{currentState?.mounted ? "每 4 秒自动刷新" : "未挂载时为空"}</span>
                  <button className="btn link" onClick={() => current && loadLog(current.id)}>
                    刷新
                  </button>
                </div>
              </div>
              <pre className="log">{log || "暂无日志。挂载失败时这里会显示 rclone 的真实报错。"}</pre>
            </section>
          </>
        )}
      </main>

      {formOpen && (
        <ConnectionForm
          initial={editing}
          env={env}
          onClose={() => setFormOpen(false)}
          onSaved={async (c) => {
            setFormOpen(false);
            setSelected(c.id);
            notify("ok", "已保存");
            await refresh();
          }}
        />
      )}

      {aboutOpen && (
        <AboutPanel
          env={env}
          onClose={() => setAboutOpen(false)}
          onRefreshEnv={refreshEnv}
          notify={notify}
        />
      )}

      {toast && <div className={`toast ${toast.kind}`}>{toast.text}</div>}
    </div>
  );
}

/** 版本与依赖面板：管理器 / rclone / WinFsp 的当前版本、检查更新、一键安装 */
function AboutPanel({
  env,
  onClose,
  onRefreshEnv,
  notify,
}: {
  env: EnvInfo | null;
  onClose: () => void;
  onRefreshEnv: () => void;
  notify: (kind: "ok" | "bad", text: string) => void;
}) {
  const [report, setReport] = useState<UpdateReport | null>(null);
  const [checking, setChecking] = useState(false);
  const [working, setWorking] = useState("");

  async function check() {
    setChecking(true);
    try {
      setReport(await api.checkUpdates());
    } catch (e) {
      notify("bad", `检查更新失败：${e}`);
    } finally {
      setChecking(false);
    }
  }

  async function doInstallRclone(version: string) {
    setWorking("rclone");
    try {
      const v = await api.installRclone(version);
      notify("ok", `rclone 已更新到 ${v}`);
      onRefreshEnv();
      await check();
    } catch (e) {
      notify("bad", String(e));
    } finally {
      setWorking("");
    }
  }

  async function doInstallWinfsp() {
    setWorking("winfsp");
    try {
      const msg = await api.installWinfsp();
      notify("ok", msg);
      onRefreshEnv();
      await check();
    } catch (e) {
      notify("bad", String(e));
    } finally {
      setWorking("");
    }
  }

  const sourceLabel: Record<string, string> = {
    bundled: "安装包内置",
    updated: "已手动更新",
    sidecar: "同目录文件",
    env: "环境变量指定",
    path: "系统 PATH",
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal narrow" onClick={(e) => e.stopPropagation()}>
        <header className="modal-head">
          <div>
            <h2>版本与依赖</h2>
            <p className="muted small">rclone 与挂载驱动都可以在这里查看和一键更新</p>
          </div>
          <button className="icon-btn" onClick={onClose} title="关闭">
            ✕
          </button>
        </header>

        <div className="modal-body">
          <div className="ver-list">
            <div className="ver-row">
              <div>
                <strong>管理器</strong>
                <div className="muted small mono">{env?.app_version ? `v${env.app_version}` : "—"}</div>
              </div>
              <button className="btn ghost small" onClick={() => api.openUrl("https://github.com/Jie-baobao/rclone-mount-manager/releases")}>
                发布页
              </button>
            </div>

            <div className="ver-row">
              <div className="ver-main">
                <strong>rclone</strong>
                <div className="muted small mono">
                  {env?.rclone_version ? `v${env.rclone_version}` : "未找到"} ·{" "}
                  {sourceLabel[env?.rclone_source ?? ""] ?? "未知来源"}
                </div>
                {env?.rclone_path && <div className="path mono" title={env.rclone_path}>{env.rclone_path}</div>}
              </div>
              <button className="btn ghost small" onClick={() => api.openUrl("https://rclone.org/downloads/")}>
                官网
              </button>
            </div>

            <div className="ver-row">
              <div>
                <strong>WinFsp（挂载驱动）</strong>
                <div className={env?.fuse_ready ? "muted small mono" : "bad small mono"}>
                  {env?.fuse_ready ? (env.fuse_version ? `v${env.fuse_version}` : "已安装") : "未安装"}
                </div>
              </div>
              <button className="btn small" disabled={working === "winfsp"} onClick={doInstallWinfsp}>
                {working === "winfsp" ? "安装中…" : env?.fuse_ready ? "重新安装" : "一键安装"}
              </button>
            </div>
          </div>

          <div className="about-actions">
            <button className="btn" onClick={check} disabled={checking}>
              {checking ? "检查中…" : "检查更新"}
            </button>
            {report?.items.map((it) => (
              <div key={it.name} className="upd-row">
                <span>
                  {it.name}：当前 {it.current || "—"}
                  {it.latest ? ` / 最新 ${it.latest}` : ""}
                  {it.note ? `（${it.note}）` : ""}
                </span>
                {it.has_update && it.name === "rclone" && (
                  <button className="btn primary small" disabled={!!working} onClick={() => doInstallRclone(it.latest)}>
                    {working === "rclone" ? "更新中…" : "一键更新"}
                  </button>
                )}
                {it.has_update && it.name !== "rclone" && (
                  <button className="btn ghost small" onClick={() => api.openUrl(it.url)}>
                    去下载
                  </button>
                )}
                {!it.has_update && it.latest && <span className="muted small">已是最新</span>}
              </div>
            ))}
          </div>

          <p className="tips">
            rclone 更新会下载官方发布包并解压到应用配置目录，下次挂载自动生效（优先于内置版本）。
            WinFsp 安装需要管理员权限，会弹出一次系统授权窗口。
          </p>
        </div>

        <footer className="modal-foot">
          <button className="btn ghost" onClick={onClose}>
            关闭
          </button>
        </footer>
      </div>
    </div>
  );
}
