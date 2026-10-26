import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import ConnectionForm from "./ConnectionForm";
import Dashboard from "./Dashboard";
import SettingsPanel from "./SettingsPanel";
import SortBar, { type SortBy, type SortDir } from "./SortBar";
import StatsPanel from "./StatsPanel";
import {
  api,
  events,
  healthOf,
  healthTip,
  isWindows,
  PROTOCOL_META,
  formatUptimeT,
  type Connection,
  type EnvInfo,
  type MountState,
  type Settings,
  type Stats,
} from "./api";
import { makeT, resolveLanguage, type T } from "./i18n";

interface Toast {
  kind: "ok" | "bad";
  text: string;
}

export default function App() {
  const [env, setEnv] = useState<EnvInfo | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [conns, setConns] = useState<Connection[]>([]);
  const [states, setStates] = useState<Record<string, MountState>>({});
  const [selected, setSelected] = useState<string | null>(null);
  const [editing, setEditing] = useState<Connection | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [log, setLog] = useState("");
  const [stats, setStats] = useState<Stats | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [busy, setBusy] = useState<string>("");
  const toastTimer = useRef<number | null>(null);

  const t: T = useMemo(() => makeT(resolveLanguage(settings?.language ?? "system")), [settings?.language]);

  // 用 ref 持有最新的翻译函数：refresh 不能依赖 t，否则切换语言会触发重新拉取设置，
  // 刚保存的语言又被磁盘上的旧值覆盖。
  const tRef = useRef(t);
  tRef.current = t;

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
      // 不做默认选中：启动和刷新后都停在首页（概览），选中只由用户点击产生。
      // 选中的连接被删掉时才回落成 null。
      setSelected((cur) => (cur && list.some((c) => c.id === cur) ? cur : null));
    } catch (e) {
      notify("bad", tRef.current("toast.readFail", { e: String(e) }));
    }
  }, [notify]);

  // 初次加载：设置（含开机自启动开关）+ 环境 + 连接
  useEffect(() => {
    refreshEnv();
    Promise.all([api.getSettings(), api.getAutostart().catch(() => false)])
      .then(([s, autostart]) => setSettings({ ...s, autostart }))
      .catch(() => setSettings(null));
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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

  const loadStats = useCallback(async (id: string) => {
    try {
      setStats(await api.getStats(id));
    } catch {
      setStats(null);
    }
  }, []);

  // 挂载中时自动刷新日志与统计，便于看实时进度（统计后端 5 秒采一次，这里 4 秒读一次足够）
  useEffect(() => {
    if (!current) {
      setLog("");
      setStats(null);
      return;
    }
    void loadLog(current.id);
    void loadStats(current.id);
    if (!currentState?.mounted) return;
    const timer = window.setInterval(() => {
      void loadLog(current.id);
      void loadStats(current.id);
    }, 4000);
    return () => window.clearInterval(timer);
  }, [current, currentState?.mounted, loadLog, loadStats]);

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
      notify("ok", t("toast.mounted", { name: c.name, target: c.target }));
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
      notify("ok", t("toast.unmounted", { name: c.name }));
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
      notify("ok", n > 0 ? t("toast.mountAll", { n }) : t("toast.mountAllNone"));
    } catch (e) {
      notify("bad", String(e));
    } finally {
      await refresh();
    }
  }

  async function handleUnmountAll() {
    try {
      const n = await api.unmountAll();
      notify("ok", t("toast.unmountAll", { n }));
    } catch (e) {
      notify("bad", String(e));
    } finally {
      await refresh();
    }
  }

  async function handleDelete(c: Connection) {
    if (!window.confirm(t("app.deleteConfirm", { name: c.name }))) return;
    setBusy(c.id);
    try {
      await api.remove(c.id);
      notify("ok", t("toast.deleted"));
      await refresh();
    } catch (e) {
      notify("bad", String(e));
    } finally {
      setBusy("");
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

  const sortBy = (settings?.sort_by ?? "default") as SortBy;
  const sortDir = (settings?.sort_dir ?? "asc") as SortDir;

  // 排序：名称按中文拼音（Intl.Collator），状态绿→黄→红→灰，分组按组名 A-Z 且未分组置底；
  // 置顶连接始终排最前。默认排序 = 后端返回顺序（添加顺序）。
  const sorted = useMemo(() => {
    const collator = new Intl.Collator("zh-Hans-CN", { numeric: true });
    const byName = (a: Connection, b: Connection) => collator.compare(a.name, b.name);
    const dir = sortDir === "desc" ? -1 : 1;
    const list = [...filtered];
    if (sortBy === "name") {
      list.sort((a, b) => dir * byName(a, b));
    } else if (sortBy === "status") {
      const rank: Record<string, number> = { healthy: 0, slow: 1, error: 2, stopped: 3 };
      list.sort((a, b) => {
        const sa = states[a.id];
        const sb = states[b.id];
        const ra = sa?.mounted ? (rank[sa.health] ?? 3) : 3;
        const rb = sb?.mounted ? (rank[sb.health] ?? 3) : 3;
        // 倒序只反转绿黄红，灰（已关闭）始终垫底
        const ka = ra === 3 ? 3 : dir > 0 ? ra : 2 - ra;
        const kb = rb === 3 ? 3 : dir > 0 ? rb : 2 - rb;
        return ka - kb || byName(a, b);
      });
    } else if (sortBy === "group") {
      list.sort((a, b) => {
        const ga = a.group.trim();
        const gb = b.group.trim();
        // 未分组无论正序倒序都放最后
        if (!ga && gb) return 1;
        if (ga && !gb) return -1;
        if (!ga && !gb) return byName(a, b);
        const c = collator.compare(ga, gb);
        return (dir > 0 ? c : -c) || byName(a, b);
      });
    }
    // 置顶永远在最前面（JS 的 sort 是稳定的，不会打乱上面的次序）
    list.sort((a, b) => Number(b.pinned) - Number(a.pinned));
    return list;
  }, [filtered, states, sortBy, sortDir]);

  // 排序状态写进 settings.json，下次开机保持上次的排序
  const changeSort = useCallback(
    (by: SortBy, dir: SortDir) => {
      setSettings((cur) => (cur ? { ...cur, sort_by: by, sort_dir: dir } : cur));
      api
        .saveSettings({ ...(settings ?? ({} as Settings)), sort_by: by, sort_dir: dir })
        .then((s) => setSettings((cur) => (cur ? { ...cur, ...s } : cur)))
        .catch(() => undefined);
    },
    [settings]
  );

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand" onClick={() => setSelected(null)} title={t("home.title")}>
          <span className="logo">◇</span>
          <div>
            <strong>Rclone Mount</strong>
            <small>{mountedCount > 0 ? `${mountedCount} · OK` : "Manager"}</small>
          </div>
        </div>

        <button
          className="btn primary block"
          onClick={() => {
            setEditing(null);
            setFormOpen(true);
          }}
        >
          {t("app.newConn")}
          <kbd>Ctrl N</kbd>
        </button>

        <input
          className="ctl search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("app.searchPh")}
        />

        <SortBar sortBy={sortBy} sortDir={sortDir} onChange={changeSort} t={t} />

        {/* 点列表的空白处（条目之间的空隙）取消选中，回到首页概览 */}
        <ul
          className="conn-list"
          onClick={(e) => {
            if (e.target === e.currentTarget) setSelected(null);
          }}
        >
          {sorted.map((c, i) => {
            const st = states[c.id];
            const h = healthOf(st, t);
            const busyThis = busy === c.id;
            // 按分组排序时，每个组的第一条上方压一行组名标题
            const showGroup =
              sortBy === "group" &&
              c.group.trim() !== "" &&
              sorted.findIndex((p) => p.group.trim() === c.group.trim()) === i;
            return (
              <Fragment key={c.id}>
                {showGroup && <li className="group-head">{c.group}</li>}
                <li
                  className={`conn ${selected === c.id ? "active" : ""}`}
                  // 再点一次已选中的连接就取消选中，回到首页概览
                  onClick={() => setSelected((prev) => (prev === c.id ? null : c.id))}
                  title={`${c.host}${c.protocol === "s3" ? "" : `:${c.port}`}`}
                >
                  <span className={h.cls} title={healthTip(st, t)} />
                  <div className="conn-meta">
                    <span className="conn-name">
                      {c.pinned && <span className="pin">★</span>}
                      {c.name}
                    </span>
                    <span className="conn-sub">
                      <span className="tag">{PROTOCOL_META[c.protocol].short}</span>
                      <span className="tag mono">{c.target}</span>
                      <span className="conn-user" title={c.user || t("app.anon")}>
                        {c.user || t("app.anon")}
                      </span>
                    </span>
                  </div>
                  <button
                    className={`row-toggle ${st?.mounted ? "stop" : "start"}`}
                    disabled={busyThis}
                    title={st?.mounted ? t("app.stopMount") : t("app.startMount")}
                    onClick={(e) => {
                      e.stopPropagation();
                      if (busyThis) return;
                      if (st?.mounted) void handleUnmount(c);
                      else void handleMount(c);
                    }}
                  >
                    {busyThis ? "…" : st?.mounted ? t("app.stop") : t("app.start")}
                  </button>
                </li>
              </Fragment>
            );
          })}
          {filtered.length === 0 && (
            <li className="empty">{conns.length === 0 ? t("app.emptyNone") : t("app.emptyNoMatch")}</li>
          )}
        </ul>

        <div className="bulk">
          <button className="btn ghost small" onClick={handleMountAll}>
            {t("app.mountAll")}
          </button>
          <button className="btn ghost small" onClick={handleUnmountAll}>
            {t("app.unmountAll")}
          </button>
        </div>

        <div className="sidebar-foot">
          <button className="env-card" onClick={() => setSettingsOpen(true)} title={t("app.settingsTip")}>
            <span className={env?.fuse_ready && env?.rclone_version ? "env ok" : "env bad"}>
              {env
                ? env.fuse_ready && env.rclone_version
                  ? t("app.envReady")
                  : t("app.envMissing")
                : t("app.envChecking")}
            </span>
            <span className="env-ver mono">
              {env
                ? `rclone ${env.rclone_version || "—"} · ${env.fuse_version ? `WinFsp ${env.fuse_version}` : t("app.noDriver")}`
                : ""}
            </span>
          </button>
          <button className="btn ghost small block" onClick={() => setSettingsOpen(true)}>
            ⚙ {t("app.settings")}
          </button>
        </div>
      </aside>

      {/* 点主区真正空白的背景（面板之间的留白）也取消选中回到首页；
          面板内部的空白（日志区、卡片、图表）有自己的元素，target 不会等于 main，不会误触 */}
      <main
        className="main"
        onClick={(e) => {
          if (e.target === e.currentTarget) setSelected(null);
        }}
      >
        {!current && <Dashboard conns={conns} states={states} t={t} onSelect={setSelected} />}

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
                    ? `${healthOf(currentState, t).label} ${current.target}${
                        currentState.latency_ms != null ? ` · ${currentState.latency_ms}ms` : ""
                      }`
                    : t("app.unmounted")}
                </span>
                <button
                  className="btn ghost small"
                  onClick={() => api.pin(current.id, !current.pinned).then(refresh)}
                  title={current.pinned ? t("app.unpin") : t("app.pin")}
                >
                  {current.pinned ? t("app.unpin") : t("app.pin")}
                </button>
              </div>
            </header>

            {currentState?.last_error && !currentState.mounted && (
              <div className="alert">
                <strong>{t("app.lastFail")}</strong>
                <pre>{currentState.last_error}</pre>
              </div>
            )}

            <section className="panel">
              <div className="kv">
                <div>
                  <label>{t("app.kvTarget")}</label>
                  <span className="mono">{current.target}</span>
                </div>
                <div>
                  <label>{t("cf.group")}</label>
                  <span>{current.group || t("home.ungrouped")}</span>
                </div>
                <div>
                  <label>{current.protocol === "s3" ? t("app.kvAccessKey") : t("app.kvUser")}</label>
                  <span className="ellipsis" title={current.user}>
                    {current.user || t("app.anon")}
                  </span>
                </div>
                <div>
                  <label>{t("app.kvUptime")}</label>
                  <span>{currentState?.mounted ? formatUptimeT(currentState.since, t) : "—"}</span>
                </div>
                <div>
                  <label>{t("app.kvQuality")}</label>
                  <span>
                    {currentState?.mounted
                      ? currentState.latency_ms != null
                        ? `${healthOf(currentState, t).label}（${currentState.latency_ms} ms）`
                        : healthOf(currentState, t).label
                      : "—"}
                  </span>
                </div>
              </div>

              <div className="actions">
                {currentState?.mounted ? (
                  <button className="btn" disabled={busy === current.id} onClick={() => handleUnmount(current)}>
                    {t("app.stopMount")}
                  </button>
                ) : (
                  <button className="btn primary" disabled={busy === current.id} onClick={() => handleMount(current)}>
                    {busy === current.id ? t("app.mounting") : t("app.startMount")}
                  </button>
                )}
                <button
                  className="btn ghost"
                  disabled={!currentState?.mounted}
                  onClick={() => api.openTarget(current.id).catch((e) => notify("bad", String(e)))}
                >
                  {t("app.openIn", { where: win ? t("app.explorer") : t("app.fileManager") })}
                </button>
                <button
                  className="btn ghost"
                  onClick={() => {
                    setEditing(current);
                    setFormOpen(true);
                  }}
                >
                  {t("app.edit")}
                </button>
                <button className="btn danger" disabled={busy === current.id} onClick={() => handleDelete(current)}>
                  {t("app.delete")}
                </button>
              </div>
            </section>

            <details className="panel details">
              <summary>{t("app.advSettings")}</summary>
              <div className="kv">
                <div>
                  <label>{t("app.readOnly")}</label>
                  <span>{current.read_only ? t("app.roYes") : t("app.roNo")}</span>
                </div>
                <div>
                  <label>{t("app.mountMode")}</label>
                  <span>{win ? (current.network_mode ? t("app.netDrive") : t("app.localDisk")) : t("app.mountPoint")}</span>
                </div>
                <div>
                  <label>{t("app.bwlimit")}</label>
                  <span>{current.bwlimit || t("app.unlimited")}</span>
                </div>
                <div>
                  <label>{t("app.cacheMax")}</label>
                  <span>{current.cache_max_size || t("app.cacheDefault")}</span>
                </div>
                <div>
                  <label>{t("app.proxy")}</label>
                  <span className="mono">{current.proxy || (current.use_system_proxy ? t("cf.sysProxy") : t("app.notSet"))}</span>
                </div>
                <div>
                  <label>{t("cf.probeMode")}</label>
                  <span>{t(`pm.probe.${current.probe_mode}`)}</span>
                </div>
                <div>
                  <label>{t("app.ignoreCert")}</label>
                  <span>{current.ignore_cert ? t("app.yes") : t("app.roNo")}</span>
                </div>
              </div>
            </details>

            <StatsPanel
              stats={stats}
              state={currentState}
              threshold={settings?.latency_threshold_ms ?? 1200}
              intervalSecs={settings?.probe_interval_secs ?? 15}
              t={t}
            />

            <section className="panel grow">
              <div className="panel-head">
                <h3>{t("app.logTitle")}</h3>
                <div className="panel-tools">
                  <span className="muted small">{currentState?.mounted ? t("app.logAuto") : t("app.logIdle")}</span>
                  <button className="btn link" onClick={() => current && loadLog(current.id)}>
                    {t("app.refresh")}
                  </button>
                </div>
              </div>
              <pre className="log">{log || t("app.logEmpty")}</pre>
            </section>
          </>
        )}
      </main>

      {formOpen && (
        <ConnectionForm
          initial={editing}
          env={env}
          t={t}
          onClose={() => setFormOpen(false)}
          onSaved={async (c) => {
            // 新建连接保存后直接挂载，省掉“再点一次启动”；
            // 编辑时后端会自动重启挂载，这里只补一句提示
            const isNew = !editing;
            const wasMounted = !!states[c.id]?.mounted;
            setFormOpen(false);
            setEditing(null);
            setSelected(c.id);
            await refresh();
            if (!isNew) {
              notify("ok", wasMounted ? t("toast.savedReconnect") : t("toast.saved"));
              return;
            }
            setBusy(c.id);
            try {
              await api.mount(c.id);
              notify("ok", t("toast.savedMounted", { target: c.target }));
            } catch (e) {
              notify("bad", t("toast.savedMountFail", { e: String(e) }));
            } finally {
              setBusy("");
              await refresh();
            }
          }}
        />
      )}

      {settingsOpen && settings && (
        <SettingsPanel
          initial={settings}
          env={env}
          t={t}
          onClose={() => setSettingsOpen(false)}
          onSaved={(s) => {
            setSettings({ ...settings, ...s });
          }}
          onRefreshEnv={refreshEnv}
          notify={notify}
        />
      )}

      {toast && <div className={`toast ${toast.kind}`}>{toast.text}</div>}
    </div>
  );
}
