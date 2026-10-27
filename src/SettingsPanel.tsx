import { useState } from "react";
import Modal from "./Modal";
import {
  api,
  isWindows,
  type EnvInfo,
  type Settings,
  type UpdateReport,
} from "./api";
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
};

/**
 * 设置页（弹窗式）：通用 / 启动 / 探测规则 / 日志 / 依赖与更新 / 数据。
 * rclone、WinFsp 的版本查看与更新检测从独立面板挪到了这里。
 */
export default function SettingsPanel({
  initial,
  env,
  t,
  onClose,
  onSaved,
  onRefreshEnv,
  notify,
}: {
  initial: Settings;
  env: EnvInfo | null;
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

  async function doExport() {
    setExporting(true);
    try {
      const json = await api.exportConnections();
      const n = (JSON.parse(json)?.connections ?? []).length;
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `rclone-mount-manager-connections.json`;
      a.click();
      URL.revokeObjectURL(url);
      notify("ok", t("st.exported", { n }));
    } catch (e) {
      notify("bad", String(e));
    } finally {
      setExporting(false);
    }
  }

  async function doImport(file: File) {
    setImporting(true);
    try {
      const text = await file.text();
      const n = await api.importConnections(text);
      notify("ok", t("st.imported", { n }));
    } catch (e) {
      notify("bad", t("st.importFail", { e: String(e) }));
    } finally {
      setImporting(false);
    }
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

  const num = (key: NumKey, label: string, min: number, max: number) => (
    <label className="field">
      <span>{label}</span>
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

      {/* 探测规则 */}
      <h3 className="set-section">{t("st.sectionProbe")}</h3>
      <div className="form-grid">
        {num("probe_interval_secs", t("st.probeInterval"), 5, 3600)}
        {num("latency_threshold_ms", t("st.latencyThreshold"), 100, 60000)}
        {num("warn_fails", t("st.warnFails"), 1, 99)}
        {num("error_fails", t("st.errorFails"), 1, 99)}
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
        {num("log_max_mb", t("st.logMax"), 1, 500)}
        {num("log_retention_days", t("st.logRetention"), 1, 3650)}
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
        <button className="btn" onClick={doExport} disabled={exporting}>
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
      </div>
      <p className="tips">{t("st.exportNote")}</p>
      <p className="tips">{t("st.importNote")}</p>
    </Modal>
  );
}
