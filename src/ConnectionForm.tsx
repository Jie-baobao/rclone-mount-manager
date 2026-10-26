import { useMemo, useState } from "react";
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

interface Props {
  initial: Connection | null;
  env: EnvInfo | null;
  onClose: () => void;
  onSaved: (c: Connection) => void;
}

const LETTERS = "DEFGHIJKLMNOPQRSTUVWXYZ".split("");

export default function ConnectionForm({ initial, env, onClose, onSaved }: Props) {
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
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <header className="modal-head">
          <div>
            <h2>{editing ? "编辑连接" : "新建连接"}</h2>
            <p className="muted small">带 * 的为必填项</p>
          </div>
          <button className="icon-btn" onClick={onClose} title="关闭">
            ✕
          </button>
        </header>

        <div className="modal-body">
          <div className="form-grid">
            <label className="field">
              <span>连接名称 *</span>
              <input className="ctl" value={name} onChange={(e) => setName(e.target.value)} placeholder="例如：家里 NAS" />
            </label>

            <label className="field">
              <span>协议</span>
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
                <span>服务商预设</span>
                <select className="ctl" value={presetKey} onChange={(e) => applyPreset(e.target.value)}>
                  {S3_PRESETS.map((p) => (
                    <option key={p.key} value={p.key}>
                      {p.label}
                    </option>
                  ))}
                </select>
              </label>
            )}

            <label className={isS3 ? "field span2" : "field span2"}>
              <span>{isS3 ? "服务地址（Endpoint）*" : protocol === "webdav" ? "服务器地址 *" : "主机 / IP *"}</span>
              <input className="ctl" value={host} onChange={(e) => setHost(e.target.value)} placeholder={meta.hint} />
            </label>

            {!isS3 && (
              <label className="field">
                <span>端口</span>
                <input
                  className="ctl"
                  type="number"
                  value={port}
                  onChange={(e) => setPort(Number(e.target.value) || meta.port)}
                  placeholder={String(meta.port)}
                />
              </label>
            )}

            <label className={isS3 ? "field" : "field"}>
              <span>{meta.pathLabel}</span>
              <input className="ctl" value={path} onChange={(e) => setPath(e.target.value)} placeholder={meta.pathPlaceholder} />
            </label>

            {isS3 && (
              <label className="field">
                <span>区域（Region）</span>
                <input className="ctl" value={region} onChange={(e) => setRegion(e.target.value)} placeholder="例如 us-east-1" />
              </label>
            )}

            <label className="field">
              <span>{isS3 ? "Access Key ID" : "用户名"}</span>
              <input className="ctl" value={user} onChange={(e) => setUser(e.target.value)} autoComplete="off" />
            </label>

            <label className="field">
              <span>{isS3 ? "Secret Access Key" : "密码"}</span>
              <div className="pass-wrap">
                <input
                  className="ctl"
                  type={showPass ? "text" : "password"}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={editing ? "留空沿用已保存密码" : ""}
                  autoComplete="new-password"
                />
                <button type="button" className="pass-toggle" onClick={() => setShowPass((v) => !v)}>
                  {showPass ? "隐藏" : "显示"}
                </button>
              </div>
            </label>

            <label className="field">
              <span>{win ? "盘符 *" : "挂载点 *"}</span>
              {win ? (
                <select className="ctl" value={target} onChange={(e) => setTarget(e.target.value)}>
                  {LETTERS.map((l) => (
                    <option key={l} value={`${l}:`}>
                      {l}:
                    </option>
                  ))}
                </select>
              ) : (
                <input className="ctl" value={target} onChange={(e) => setTarget(e.target.value)} placeholder="/mnt/nas" />
              )}
            </label>

            <label className="field">
              <span>缓存目录</span>
              <input className="ctl" value={cacheDir} onChange={(e) => setCacheDir(e.target.value)} placeholder="留空用默认" />
            </label>
          </div>

          {isS3 && (
            <p className="tips">
              Bucket 可留空（挂载后再进目录选），也可以直接填。MinIO 与多数自建网关需要勾选「强制 path-style」，
              选对应预设会自动设置。
            </p>
          )}

          {protocol === "webdav" && (
            <p className="tips">
              密码里有 <code>!</code> 时注意别被输入法打成全角 <code>！</code>——看起来一样，但服务端一定返回 401。可用上面的「显示」核对。
            </p>
          )}

          <div className="checks">
            {protocol === "webdav" && (
              <label className="check">
                <input type="checkbox" checked={ignoreCert} onChange={(e) => setIgnoreCert(e.target.checked)} />
                忽略证书校验（自签证书必选）
              </label>
            )}
            {isS3 && (
              <label className="check">
                <input type="checkbox" checked={pathStyle} onChange={(e) => setPathStyle(e.target.checked)} />
                强制 path-style（MinIO / 自建网关）
              </label>
            )}
            <label className="check">
              <input type="checkbox" checked={autoMount} onChange={(e) => setAutoMount(e.target.checked)} />
              启动时自动挂载
            </label>
            <label className="check">
              <input type="checkbox" checked={reconnect} onChange={(e) => setReconnect(e.target.checked)} />
              断线自动重连
            </label>
          </div>

          <details className="adv">
            <summary>高级选项</summary>
            <div className="form-grid">
              {win && (
                <label className="field">
                  <span>挂载形态</span>
                  <select className="ctl" value={networkMode ? "net" : "local"} onChange={(e) => setNetworkMode(e.target.value === "net")}>
                    <option value="local">本地磁盘</option>
                    <option value="net">网络驱动器</option>
                  </select>
                </label>
              )}
              <label className="field">
                <span>带宽限制</span>
                <input className="ctl" value={bwlimit} onChange={(e) => setBwlimit(e.target.value)} placeholder="如 10M，留空不限" />
              </label>
              <label className="field">
                <span>缓存上限</span>
                <input className="ctl" value={cacheMax} onChange={(e) => setCacheMax(e.target.value)} placeholder="如 10G" />
              </label>
              <label className="field">
                <span>HTTP 代理</span>
                <input className="ctl" value={proxy} onChange={(e) => setProxy(e.target.value)} placeholder="http://127.0.0.1:7890" />
              </label>
              <label className="check wide">
                <input type="checkbox" checked={readOnly} onChange={(e) => setReadOnly(e.target.checked)} />
                只读挂载（防止误删）
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
              <div className="diag-url">请求地址：{result.url}</div>
              {result.detail && <pre className="diag-detail">{result.detail}</pre>}
            </div>
          )}

          {error && <div className="diag bad">{error}</div>}
        </div>

        <footer className="modal-foot">
          <button className="btn ghost" onClick={onClose}>
            取消
          </button>
          <button className="btn" onClick={doTest} disabled={testing || !host}>
            {testing ? "测试中…" : "测试连接"}
          </button>
          <button className="btn primary" onClick={doSave} disabled={saving || !name || !host}>
            {saving ? "保存中…" : "保存"}
          </button>
        </footer>
      </div>
    </div>
  );
}
