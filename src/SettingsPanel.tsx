import { useState } from "react";
import Modal from "./Modal";
import {
  api,
  fmtLatency,
  isWindows,
  type Connection,
  type EnvInfo,
  type MountState,
  type Settings,
  type UpdateReport,
} from "./api";
import { copyText } from "./clipboard";
import { LANGUAGE_OPTIONS, makeT, resolveLanguage } from "./i18n";
import type { T } from "./i18n";

const DEFAULTS: Settings = {
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

/**
 * 设置页（弹窗式）：通用 / 启动 / 探测规则 / 日志 / 依赖与更新 / 数据。
 * rclone、WinFsp 的版本查看与更新检测从独立面板挪到了这里。
 */
export default function SettingsPanel({
  initial,
  env,
  conns,
  states,
  t,
  onClose,
  onSaved,
  onRefreshEnv,
  notify,
}: {
  initial: Settings;
  env: EnvInfo | null;
  /** 诊断报告要列出每条连接的状态，所以把列表和挂载状态一起传进来 */
  conns: Connection[];
  states: Record<string, MountState>;
  t: T;
  onClose: () => void;
  onSaved: (s: Settings) => void;
  onRefreshEnv: () => void;
  notify: (kind: "ok" | "bad", text: string) => void;
}) {
  const [cfg, setCfg] = useState<Settings>(initial);
  const [saving, setSaving] = useState(false);
  const [report, setReport] = useState<UpdateReport | null>(null);
  const [checking, setChecking] = useState(false);
  const [working, setWorking] = useState("");
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  /** 导出：先问一句要不要加密（密码是可选口令，留空即明文） */
  const [expOpen, setExpOpen] = useState(false);
  const [expPass, setExpPass] = useState("");
  /** 导入：加密文件先挂起，等用户给口令 */
  const [impPending, setImpPending] = useState<string | null>(null);
  const [impPass, setImpPass] = useState("");
  const [diagCopied, setDiagCopied] = useState(false);

  const win = isWindows(env);
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setCfg((c) => ({ ...c, [key]: value }));

  async function save() {
    setSaving(true);
    try {
      const saved = await api.saveSettings(cfg);
      setCfg(saved);
      onSaved(saved);
      // 提示语必须用「刚保存的语言」：此时父组件还没重渲染，直接用闭包里的 t 会显示旧语言
      notify("ok", makeT(resolveLanguage(saved.language))("st.saved"));
    } catch (e) {
      notify("bad", String(e));
    } finally {
      setSaving(false);
    }
  }

  async function check() {
    setChecking(true);
    try {
      setReport(await api.checkUpdates());
    } catch (e) {
      notify("bad", t("st.checkFail", { e: String(e) }));
    } finally {
      setChecking(false);
    }
  }

  async function doInstallRclone(version: string) {
    setWorking("rclone");
    try {
      const v = await api.installRclone(version);
      notify("ok", t("st.rcloneUpdated", { v }));
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

  async function doExport(password?: string) {
    setExporting(true);
    try {
      const json = await api.exportConnections(password);
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `rclone-mount-manager-connections${password ? "-encrypted" : ""}.json`;
      a.click();
      URL.revokeObjectURL(url);
      notify("ok", password ? t("st.exportedEnc") : t("st.exported", { n: conns.length }));
      setExpOpen(false);
      setExpPass("");
    } catch (e) {
      notify("bad", String(e));
    } finally {
      setExporting(false);
    }
  }

  async function runImport(text: string, password?: string) {
    setImporting(true);
    try {
      const n = await api.importConnections(text, password);
      // 明确告诉用户这次是「解密后导入」还是「明文导入」，避免以为文件已受保护
      notify("ok", password ? t("st.importedEncOk", { n }) : t("st.importedPlain", { n }));
      setImpPending(null);
      setImpPass("");
    } catch (e) {
      notify("bad", t("st.importFail", { e: String(e) }));
    } finally {
      setImporting(false);
    }
  }

  async function doImport(file: File) {
    const text = await file.text();
    // 先在前端看一眼是不是加密信封：是的话先要口令，别把「缺口令」当失败弹给用户
    let encrypted = false;
    try {
      encrypted = JSON.parse(text)?.encrypted === true;
    } catch {
      /* 不是 JSON 就交给后端报错，文案更准确 */
    }
    if (encrypted) {
      setImpPending(text);
      setImpPass("");
      return;
    }
    await runImport(text);
  }

  /** 诊断报告：把排查问题需要的信息一次复制出来，方便发群/发 issue */
  function diagText(): string {
    const mountedCount = conns.filter((c) => states[c.id]?.mounted).length;
    const rows = conns.map((c) => {
      const st = states[c.id];
      const err = (st?.last_error ?? "").split("\n").filter((l) => l.trim()).slice(-1)[0] ?? "";
      const state = st?.mounted ? `${st.health}${st.latency_us != null ? ` ${fmtLatency(st.latency_us)}` : ""}` : "stopped";
      return `- ${c.name} | ${c.protocol.toUpperCase()} | ${c.target} | ${c.host}:${c.port} | ${state}${
        err ? ` | ${err.slice(0, 120)}` : ""
      }`;
    });
    return [
      `Rclone Mount Manager — ${t("st.diagTitle")}`,
      `Time: ${new Date().toLocaleString()}`,
      `Manager: v${env?.app_version ?? "?"}`,
      `Platform: ${env?.platform ?? "?"}`,
      `rclone: ${env?.rclone_version || "-"} (${env?.rclone_source || "-"}) ${env?.rclone_path || ""}`,
      `FUSE: ${env?.fuse_ready ? env?.fuse_version || "ready" : "missing"}`,
      `Settings: lang=${cfg.language} probe=${cfg.probe_interval_secs}s threshold=${cfg.latency_threshold_ms}ms warn=${cfg.warn_fails} error=${cfg.error_fails} log=${cfg.log_max_mb}MB/${cfg.log_retention_days}d sort=${cfg.sort_by}/${cfg.sort_dir}`,
      `Connections: ${conns.length} (mounted ${mountedCount})`,
      ...rows,
    ].join("\n");
  }

  async function copyDiag() {
    const ok = await copyText(diagText());
    if (!ok) {
      notify("bad", t("st.diagFail"));
      return;
    }
    setDiagCopied(true);
    window.setTimeout(() => setDiagCopied(false), 1800);
  }

  const srcLabel: Record<string, string> = {
    bundled: t("st.src.bundled"),
    updated: t("st.src.updated"),
    sidecar: t("st.src.sidecar"),
    env: t("st.src.env"),
    path: t("st.src.path"),
  };

  const NUM_KEYS = [
    "probe_interval_secs",
    "latency_threshold_ms",
    "warn_fails",
    "error_fails",
    "log_max_mb",
    "log_retention_days",
  ] as const;
  type NumKey = (typeof NUM_KEYS)[number];

  const num = (key: NumKey, label: string, min: number, max: number, hint?: string) => (
    <label className="field" title={hint}>
      <span>
        {label}
        {hint ? <i className="hint-dot" aria-hidden="true">?</i> : null}
      </span>
      <input
        className="ctl"
        type="number"
        min={min}
        max={max}
        value={String(cfg[key])}
        onChange={(e) => set(key, Number(e.target.value) || 0)}
      />
    </label>
  );

  return (
    <Modal
      title={t("st.title")}
      subtitle={t("st.subtitle")}
      onClose={onClose}
      closeLabel={t("cf.close")}
      size="settings"
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            {t("cf.cancel")}
          </button>
          <button className="btn primary" onClick={save} disabled={saving}>
            {saving ? "…" : t("st.save")}
          </button>
        </>
      }
    >
      {/* 通用 */}
      <h3 className="set-section">{t("st.sectionGeneral")}</h3>
      <div className="form-grid">
        <label className="field span2">
          <span>{t("st.language")}</span>
          <select
            className="ctl"
            value={cfg.language}
            onChange={(e) => set("language", e.target.value)}
          >
            {LANGUAGE_OPTIONS.map((o) => (
              <option key={o.code} value={o.code}>
                {o.code === "system" ? t("st.langSystem") : o.native}
              </option>
            ))}
          </select>
          <small className="muted">{t("st.langNote")}</small>
        </label>
      </div>

      {/* 启动 */}
      <h3 className="set-section">{t("st.sectionStartup")}</h3>
      <div className="checks">
        <label className="check">
          <input
            type="checkbox"
            checked={cfg.autostart ?? false}
            onChange={async (e) => {
              // 必须在 await 之前取值：这个复选框是受控组件，await 期间 React 会把 DOM 的
              // checked 刷回 false，等 setAutostart 返回后再读就永远是 false（表现为“勾不上”）。
              const next = e.target.checked;
              setCfg((c) => ({ ...c, autostart: next }));
              try {
                await api.setAutostart(next);
                notify("ok", next ? t("toast.autostartOn") : t("toast.autostartOff"));
              } catch (err) {
                setCfg((c) => ({ ...c, autostart: !next }));
                notify("bad", String(err));
              }
            }}
          />
          {t("st.autostart")}
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={cfg.silent_start}
            onChange={(e) => set("silent_start", e.target.checked)}
          />
          {t("st.silent")}
        </label>
      </div>
      <p className="tips">{t("st.silentNote")}</p>
      <p className="tips">{t("st.hotkeyNote")}</p>

      {/* 资源管理器集成：只有 Windows 有「发送到」菜单 */}
      {win && (
        <>
          <h3 className="set-section">{t("st.sectionShell")}</h3>
          <div className="checks">
            <label className="check">
              <input
                type="checkbox"
                checked={cfg.sendto_enabled}
                onChange={(e) => set("sendto_enabled", e.target.checked)}
              />
              {t("st.sendto")}
            </label>
          </div>
          <p className="tips">{t("st.sendtoNote")}</p>
        </>
      )}

      {/* 探测规则 */}
      <h3 className="set-section">{t("st.sectionProbe")}</h3>
      <div className="form-grid">
        {num("probe_interval_secs", t("st.probeInterval"), 5, 3600, t("st.hintInterval"))}
        {num("latency_threshold_ms", t("st.latencyThreshold"), 100, 60000, t("st.hintThreshold"))}
        {num("warn_fails", t("st.warnFails"), 1, 99, t("st.hintWarn"))}
        {num("error_fails", t("st.errorFails"), 1, 99, t("st.hintError"))}
      </div>
      <p className="tips">{t("st.latencyNote")}</p>
      <p className="tips">{t("st.probeNote")}</p>
      <div className="row-actions">
        <button className="btn ghost small" onClick={() => setCfg({ ...DEFAULTS })}>
          {t("st.resetDefaults")}
        </button>
      </div>

      {/* 日志 */}
      <h3 className="set-section">{t("st.sectionLog")}</h3>
      <div className="form-grid">
        {num("log_max_mb", t("st.logMax"), 1, 500, t("st.hintLogSize"))}
        {num("log_retention_days", t("st.logRetention"), 1, 3650, t("st.hintLogDays"))}
      </div>
      <p className="tips">{t("st.logNote")}</p>

      {/* 依赖与更新 */}
      <h3 className="set-section">{t("st.sectionDeps")}</h3>
      <p className="tips">{t("st.updateNote")}</p>
      <p className="tips">{t("st.bundleNote")}</p>
      <div className="ver-list">
        <div className="ver-row">
          <div>
            <strong>{t("st.manager")}</strong>
            <div className="muted small mono">{env?.app_version ? `v${env.app_version}` : "—"}</div>
          </div>
          <button
            className="btn ghost small"
            onClick={() => api.openUrl("https://github.com/Jie-baobao/rclone-mount-manager/releases")}
          >
            {t("st.releasePage")}
          </button>
        </div>

        <div className="ver-row">
          <div className="ver-main">
            <strong>rclone</strong>
            <div className="muted small mono">
              {env?.rclone_version ? `v${env.rclone_version}` : t("st.notFound")} ·{" "}
              {srcLabel[env?.rclone_source ?? ""] ?? t("st.src.unknown")}
            </div>
            {env?.rclone_path && (
              <div className="path mono" title={env.rclone_path}>
                {env.rclone_path}
              </div>
            )}
          </div>
          <button className="btn ghost small" onClick={() => api.openUrl("https://rclone.org/downloads/")}>
            {t("st.officialSite")}
          </button>
        </div>

        <div className="ver-row">
          <div>
            <strong>{t("st.winfsp")}</strong>
            <div className={env?.fuse_ready ? "muted small mono" : "bad small mono"}>
              {env?.fuse_ready
                ? env.fuse_version
                  ? `v${env.fuse_version}`
                  : t("st.installed")
                : t("st.notInstalled")}
            </div>
          </div>
          <button className="btn small" disabled={working === "winfsp" || !win} onClick={doInstallWinfsp}>
            {working === "winfsp" ? t("st.installing") : env?.fuse_ready ? t("st.reinstall") : t("st.install")}
          </button>
        </div>
      </div>

      <div className="about-actions">
        <button className="btn" onClick={check} disabled={checking}>
          {checking ? t("st.checking") : t("st.checkUpdates")}
        </button>
        {report?.items.map((it) => (
          <div key={it.name} className="upd-row">
            <span>
              {it.name} · {t("st.current")} {it.current || "—"}
              {it.latest ? ` / ${t("st.newest")} ${it.latest}` : ""}
              {it.note ? ` (${it.note})` : ""}
            </span>
            {it.has_update && it.name === "rclone" && (
              <button className="btn primary small" disabled={!!working} onClick={() => doInstallRclone(it.latest)}>
                {working === "rclone" ? t("st.updating") : t("st.updateNow")}
              </button>
            )}
            {it.has_update && it.name !== "rclone" && (
              <button className="btn ghost small" onClick={() => api.openUrl(it.url)}>
                {t("st.download")}
              </button>
            )}
            {!it.has_update && it.latest && <span className="muted small">{t("st.latest")}</span>}
          </div>
        ))}
      </div>
      <p className="tips">{t("st.depsTip")}</p>

      {/* 数据 */}
      <h3 className="set-section">{t("st.sectionData")}</h3>
      <div className="data-actions">
        <button className="btn" onClick={() => setExpOpen((v) => !v)} disabled={exporting}>
          {exporting ? "…" : t("st.export")}
        </button>
        <label className={`btn ${importing ? "disabled" : ""}`}>
          {importing ? "…" : t("st.import")}
          <input
            type="file"
            accept="application/json,.json"
            style={{ display: "none" }}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void doImport(f);
              e.target.value = "";
            }}
          />
        </label>
        <button className="btn ghost" onClick={() => void copyDiag()}>
          {diagCopied ? t("st.diagCopied") : t("st.diagCopy")}
        </button>
      </div>

      {/* 导出：明说这份文件里带着可还原的口令，让用户自己决定要不要加密 */}
      {expOpen && (
        <div className="pass-box">
          <p className="tips">{t("st.exportWarn")}</p>
          <div className="pass-row">
            <input
              className="ctl"
              type="password"
              value={expPass}
              onChange={(e) => setExpPass(e.target.value)}
              placeholder={t("st.passPh")}
              autoComplete="new-password"
            />
            <button className="btn primary small" disabled={!expPass || exporting} onClick={() => void doExport(expPass)}>
              {t("st.expEncrypted")}
            </button>
            <button className="btn small" disabled={exporting} onClick={() => void doExport(undefined)}>
              {t("st.expPlain")}
            </button>
            <button
              className="btn ghost small"
              onClick={() => {
                setExpOpen(false);
                setExpPass("");
              }}
            >
              {t("cf.cancel")}
            </button>
          </div>
        </div>
      )}

      {/* 导入：加密文件先要口令，避免把「缺口令」当成导入失败 */}
      {impPending && (
        <div className="pass-box">
          <p className="tips">{t("st.importEncNote")}</p>
          <div className="pass-row">
            <input
              className="ctl"
              type="password"
              value={impPass}
              onChange={(e) => setImpPass(e.target.value)}
              placeholder={t("st.passPh")}
              autoComplete="off"
            />
            <button
              className="btn primary small"
              disabled={!impPass || importing}
              onClick={() => void runImport(impPending, impPass)}
            >
              {t("st.impGo")}
            </button>
            <button
              className="btn ghost small"
              onClick={() => {
                setImpPending(null);
                setImpPass("");
              }}
            >
              {t("cf.cancel")}
            </button>
          </div>
        </div>
      )}

      <p className="tips">{t("st.exportNote")}</p>
      <p className="tips">{t("st.importNote")}</p>
      <p className="tips">{t("st.diagNote")}</p>
    </Modal>
  );
}
