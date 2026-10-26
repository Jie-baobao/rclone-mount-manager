import { useMemo, useState } from "react";
import Modal from "./Modal";
import {
  api,
  isWindows,
  PROTOCOL_META,
  S3_PRESETS,
  type Connection,
  type ConnectionInput,
  type DiagResult,
  type EnvInfo,
  type Protocol,
} from "./api";
import type { T } from "./i18n";

interface Props {
  initial: Connection | null;
  env: EnvInfo | null;
  t: T;
  onClose: () => void;
  onSaved: (c: Connection) => void;
}

const LETTERS = "DEFGHIJKLMNOPQRSTUVWXYZ".split("");

export default function ConnectionForm({ initial, env, t, onClose, onSaved }: Props) {
  const win = isWindows(env);
  const editing = !!initial;

  const [name, setName] = useState(initial?.name ?? "");
  const [protocol, setProtocol] = useState<Protocol>(initial?.protocol ?? "smb");
  const [host, setHost] = useState(initial?.host ?? "");
  const [port, setPort] = useState<number>(initial?.port ?? PROTOCOL_META.smb.port);
  const [path, setPath] = useState(initial?.path ?? "");
  const [user, setUser] = useState(initial?.user ?? "");
  const [password, setPassword] = useState("");
  const [showPass, setShowPass] = useState(false);
  const [target, setTarget] = useState(initial?.target ?? (win ? "Z:" : "/mnt/nas"));
  const [cacheDir, setCacheDir] = useState(initial?.cache_dir ?? "");

  const [ignoreCert, setIgnoreCert] = useState(initial?.ignore_cert ?? false);
  const [readOnly, setReadOnly] = useState(initial?.read_only ?? false);
  const [networkMode, setNetworkMode] = useState(initial?.network_mode ?? false);
  const [bwlimit, setBwlimit] = useState(initial?.bwlimit ?? "");
  const [cacheMax, setCacheMax] = useState(initial?.cache_max_size ?? "10G");
  const [proxy, setProxy] = useState(initial?.proxy ?? "");
  const [reconnect, setReconnect] = useState(initial?.reconnect ?? true);
  const [autoMount, setAutoMount] = useState(initial?.auto_mount ?? false);

  const [s3Provider, setS3Provider] = useState(initial?.s3_provider ?? "Other");
  const [region, setRegion] = useState(initial?.region ?? "");
  const [pathStyle, setPathStyle] = useState(initial?.s3_path_style ?? false);
  const [presetKey, setPresetKey] = useState(
    S3_PRESETS.find((p) => p.provider === (initial?.s3_provider ?? "Other"))?.key ?? "other"
  );

  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<DiagResult | null>(null);
  const [error, setError] = useState("");

  const meta = PROTOCOL_META[protocol];
  const isS3 = protocol === "s3";

  const payload = useMemo<ConnectionInput>(
    () => ({
      id: initial?.id ?? null,
      name,
      protocol,
      host,
      port,
      path,
      user,
      password,
      target,
      cache_dir: cacheDir,
      ignore_cert: ignoreCert,
      s3_provider: s3Provider,
      region,
      s3_path_style: pathStyle,
      read_only: readOnly,
      network_mode: networkMode,
      bwlimit,
      cache_max_size: cacheMax,
      proxy,
      reconnect,
      auto_mount: autoMount,
      pinned: initial?.pinned ?? false,
    }),
    [
      initial?.id,
      initial?.pinned,
      name,
      protocol,
      host,
      port,
      path,
      user,
      password,
      target,
      cacheDir,
      ignoreCert,
      s3Provider,
      region,
      pathStyle,
      readOnly,
      networkMode,
      bwlimit,
      cacheMax,
      proxy,
      reconnect,
      autoMount,
    ]
  );

  function changeProtocol(p: Protocol) {
    setProtocol(p);
    setPort(PROTOCOL_META[p].port);
    setResult(null);
    if (!editing && win) setTarget(target || "Z:");
  }

  function applyPreset(key: string) {
    setPresetKey(key);
    const preset = S3_PRESETS.find((p) => p.key === key);
    if (!preset) return;
    setS3Provider(preset.provider);
    setPathStyle(preset.pathStyle);
    if (preset.endpoint) setHost(preset.endpoint);
    setRegion(preset.region);
    setResult(null);
  }

  async function doTest() {
    setTesting(true);
    setError("");
    setResult(null);
    try {
      setResult(await api.test(payload));
    } catch (e) {
      setError(String(e));
    } finally {
      setTesting(false);
    }
  }

  async function doSave() {
    setSaving(true);
    setError("");
    try {
      const saved = await api.save(payload);
      onSaved(saved);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal
      title={editing ? t("cf.editTitle") : t("cf.newTitle")}
      subtitle={t("cf.required")}
      onClose={onClose}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            {t("cf.cancel")}
          </button>
          <button className="btn" onClick={doTest} disabled={testing || !host}>
            {testing ? t("cf.testing") : t("cf.test")}
          </button>
          <button className="btn primary" onClick={doSave} disabled={saving || !name || !host}>
            {saving ? t("cf.saving") : t("cf.save")}
          </button>
        </>
      }
    >
      <div className="form-grid">
        <label className="field">
          <span>{t("cf.name")}</span>
          <input className="ctl" value={name} onChange={(e) => setName(e.target.value)} placeholder={t("cf.namePh")} />
        </label>

        <label className="field">
          <span>{t("cf.protocol")}</span>
          <select className="ctl" value={protocol} onChange={(e) => changeProtocol(e.target.value as Protocol)}>
            {(Object.keys(PROTOCOL_META) as Protocol[]).map((p) => (
              <option key={p} value={p}>
                {PROTOCOL_META[p].label}
              </option>
            ))}
          </select>
        </label>

        {isS3 && (
          <label className="field span2">
            <span>{t("cf.preset")}</span>
            <select className="ctl" value={presetKey} onChange={(e) => applyPreset(e.target.value)}>
              {S3_PRESETS.map((p) => (
                <option key={p.key} value={p.key}>
                  {t(`ps.${p.key}`)}
                </option>
              ))}
            </select>
          </label>
        )}

        <label className="field span2">
          <span>{isS3 ? t("cf.hostS3") : protocol === "webdav" ? t("cf.hostWebdav") : t("cf.host")}</span>
          <input className="ctl" value={host} onChange={(e) => setHost(e.target.value)} placeholder={t(`pm.${protocol}.hint`)} />
        </label>

        {!isS3 && (
          <label className="field">
            <span>{t("cf.port")}</span>
            <input
              className="ctl"
              type="number"
              value={port}
              onChange={(e) => setPort(Number(e.target.value) || meta.port)}
              placeholder={String(meta.port)}
            />
          </label>
        )}

        <label className="field">
          <span>{t(`pm.${protocol}.pathLabel`)}</span>
          <input className="ctl" value={path} onChange={(e) => setPath(e.target.value)} placeholder={t(`pm.${protocol}.pathPlaceholder`)} />
        </label>

        {isS3 && (
          <label className="field">
            <span>{t("cf.region")}</span>
            <input className="ctl" value={region} onChange={(e) => setRegion(e.target.value)} placeholder={t("cf.regionPh")} />
          </label>
        )}

        <label className="field">
          <span>{isS3 ? t("cf.accessKey") : t("cf.user")}</span>
          <input className="ctl" value={user} onChange={(e) => setUser(e.target.value)} autoComplete="off" />
        </label>

        <label className="field">
          <span>{isS3 ? t("cf.secret") : t("cf.pass")}</span>
          <div className="pass-wrap">
            <input
              className="ctl"
              type={showPass ? "text" : "password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={editing ? t("cf.passKeep") : ""}
              autoComplete="new-password"
            />
            <button type="button" className="pass-toggle" onClick={() => setShowPass((v) => !v)}>
              {showPass ? t("cf.hide") : t("cf.show")}
            </button>
          </div>
        </label>

        <label className="field">
          <span>{win ? t("cf.drive") : t("cf.mountpoint")}</span>
          {win ? (
            <select className="ctl" value={target} onChange={(e) => setTarget(e.target.value)}>
              {LETTERS.map((l) => (
                <option key={l} value={`${l}:`}>
                  {l}:
                </option>
              ))}
            </select>
          ) : (
            <input className="ctl" value={target} onChange={(e) => setTarget(e.target.value)} placeholder={t("cf.mountpointPh")} />
          )}
        </label>

        <label className="field">
          <span>{t("cf.cacheDir")}</span>
          <input className="ctl" value={cacheDir} onChange={(e) => setCacheDir(e.target.value)} placeholder={t("cf.cacheDirPh")} />
        </label>
      </div>

      {isS3 && <p className="tips">{t("cf.s3Tip")}</p>}

      {protocol === "webdav" && <p className="tips">{t("cf.davTip")}</p>}

      <div className="checks">
        {protocol === "webdav" && (
          <label className="check">
            <input type="checkbox" checked={ignoreCert} onChange={(e) => setIgnoreCert(e.target.checked)} />
            {t("cf.ignoreCert")}
          </label>
        )}
        {isS3 && (
          <label className="check">
            <input type="checkbox" checked={pathStyle} onChange={(e) => setPathStyle(e.target.checked)} />
            {t("cf.pathStyle")}
          </label>
        )}
        <label className="check">
          <input type="checkbox" checked={autoMount} onChange={(e) => setAutoMount(e.target.checked)} />
          {t("cf.autoMount")}
        </label>
        <label className="check">
          <input type="checkbox" checked={reconnect} onChange={(e) => setReconnect(e.target.checked)} />
          {t("cf.reconnect")}
        </label>
      </div>

      <details className="adv">
        <summary>{t("cf.adv")}</summary>
        <div className="form-grid">
          {win && (
            <label className="field">
              <span>{t("app.mountMode")}</span>
              <select className="ctl" value={networkMode ? "net" : "local"} onChange={(e) => setNetworkMode(e.target.value === "net")}>
                <option value="local">{t("app.localDisk")}</option>
                <option value="net">{t("app.netDrive")}</option>
              </select>
            </label>
          )}
          <label className="field">
            <span>{t("app.bwlimit")}</span>
            <input className="ctl" value={bwlimit} onChange={(e) => setBwlimit(e.target.value)} placeholder={t("cf.bwPh")} />
          </label>
          <label className="field">
            <span>{t("app.cacheMax")}</span>
            <input className="ctl" value={cacheMax} onChange={(e) => setCacheMax(e.target.value)} placeholder={t("cf.cachePh")} />
          </label>
          <label className="field">
            <span>{t("app.proxy")}</span>
            <input className="ctl" value={proxy} onChange={(e) => setProxy(e.target.value)} placeholder={t("cf.proxyPh")} />
          </label>
          <label className="check wide">
            <input type="checkbox" checked={readOnly} onChange={(e) => setReadOnly(e.target.checked)} />
            {t("cf.readOnly")}
          </label>
        </div>
      </details>

      {result && (
        <div className={`diag ${result.ok ? "ok" : "bad"}`}>
          <strong>
            {result.ok ? "✓ " : "✗ "}
            {result.message}
            {result.code != null && <span className="code">HTTP {result.code}</span>}
          </strong>
          <div className="diag-url">
            {t("cf.reqUrl")}
            {result.url}
          </div>
          {result.detail && <pre className="diag-detail">{result.detail}</pre>}
        </div>
      )}

      {error && <div className="diag bad">{error}</div>}
    </Modal>
  );
}
