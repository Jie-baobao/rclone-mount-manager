import { useEffect, useMemo, useState } from "react";
import Modal from "./Modal";
import {
  api,
  isWindows,
  PROTOCOL_META,
  S3_PRESETS,
  type Connection,
  type ConnectionInput,
  type CryptFilename,
  type DiagResult,
  type EnvInfo,
  type Protocol,
  type ProbeMode,
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

/**
 * 常用端口清单：下拉里按协议分组，标注「端口 · 协议 · 用途」。
 * 只做建议，用户随时切到「自定义…」手填（NAS 上改过端口的很常见）。
 */
const PORT_PRESETS: { port: number; proto: Protocol; note?: string }[] = [
  { port: 445, proto: "smb" },
  { port: 139, proto: "smb", note: "NetBIOS" },
  { port: 80, proto: "webdav" },
  { port: 443, proto: "webdav" },
  { port: 5005, proto: "webdav", note: "Synology" },
  { port: 5006, proto: "webdav", note: "Synology" },
  { port: 21, proto: "ftp" },
  { port: 2121, proto: "ftp" },
  { port: 22, proto: "sftp" },
  { port: 2222, proto: "sftp" },
];

export default function ConnectionForm({ initial, env, t, onClose, onSaved }: Props) {
  const win = isWindows(env);
  const editing = !!initial;

  const [name, setName] = useState(initial?.name ?? "");
  const [protocol, setProtocol] = useState<Protocol>(initial?.protocol ?? "smb");
  const [host, setHost] = useState(initial?.host ?? "");
  /** 端口存字符串：自定义模式下用户可能正在清空重填，中间态不能强转成数字 */
  const [portText, setPortText] = useState<string>(
    String(initial?.port ?? PROTOCOL_META.smb.port)
  );
  /** 下拉选了「自定义…」：额外显示一个数字输入框 */
  const [portCustom, setPortCustom] = useState<boolean>(
    () =>
      !PORT_PRESETS.some((p) => p.port === (initial?.port ?? PROTOCOL_META.smb.port))
  );
  const [path, setPath] = useState(initial?.path ?? "");
  const [user, setUser] = useState(initial?.user ?? "");
  /** SMB 域 / 工作组：微软账户填 MicrosoftAccount，域环境填 NetBIOS 域名 */
  const [domain, setDomain] = useState(initial?.domain ?? "");
  const [password, setPassword] = useState("");
  const [showPass, setShowPass] = useState(false);
  /** SMB / FTP 的匿名访问：勾了就不要求账号密码（rclone 不带凭据，服务端按 guest 放行） */
  const [anon, setAnon] = useState(false);
  const [target, setTarget] = useState(initial?.target ?? (win ? "Z:" : ""));
  const [cacheDir, setCacheDir] = useState(initial?.cache_dir ?? "");

  const [ignoreCert, setIgnoreCert] = useState(initial?.ignore_cert ?? false);
  const [readOnly, setReadOnly] = useState(initial?.read_only ?? false);
  const [networkMode, setNetworkMode] = useState(initial?.network_mode ?? false);
  const [bwlimit, setBwlimit] = useState(initial?.bwlimit ?? "");
  const [cacheMax, setCacheMax] = useState(initial?.cache_max_size ?? "10G");
  const [proxy, setProxy] = useState(initial?.proxy ?? "");
  const [reconnect, setReconnect] = useState(initial?.reconnect ?? true);
  const [autoMount, setAutoMount] = useState(initial?.auto_mount ?? false);
  const [group, setGroup] = useState(initial?.group ?? "");
  const [sysProxy, setSysProxy] = useState(initial?.use_system_proxy ?? false);
  const [probeMode, setProbeMode] = useState<ProbeMode>(initial?.probe_mode ?? "light");
  const [extraArgs, setExtraArgs] = useState(initial?.extra_args ?? "");
  /** 盘符预检结果（空串 = 没问题） */
  const [targetWarn, setTargetWarn] = useState("");

  const [s3Provider, setS3Provider] = useState(initial?.s3_provider ?? "Other");
  const [region, setRegion] = useState(initial?.region ?? "");
  const [pathStyle, setPathStyle] = useState(initial?.s3_path_style ?? false);
  /** 加密目录（rclone crypt）：默认关。挂载点里看到明文，底层存密文 */
  const [crypt, setCrypt] = useState(initial?.crypt ?? false);
  const [cryptPw, setCryptPw] = useState("");
  /** 二次密码 / salt：留空沿用已存的（改了会让已有数据解不开，所以不提供「清空」） */
  const [cryptSalt, setCryptSalt] = useState("");
  const [cryptFilename, setCryptFilename] = useState<CryptFilename>(
    initial?.crypt_filename === "obfuscate" || initial?.crypt_filename === "off"
      ? initial.crypt_filename
      : "standard"
  );
  const [cryptDirNames, setCryptDirNames] = useState(initial?.crypt_dir_names ?? true);
  const [presetKey, setPresetKey] = useState(
    S3_PRESETS.find((p) => p.provider === (initial?.s3_provider ?? "Other"))?.key ?? "other"
  );

  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [result, setResult] = useState<DiagResult | null>(null);
  const [error, setError] = useState("");

  const meta = PROTOCOL_META[protocol];
  const isS3 = protocol === "s3";

  /** conf 里真存过密码吗（后端从 rclone.conf 查的，编辑时随列表下发；新建一律没有）。
   *  以前占位符一律写「留空沿用已保存密码」，没存过的连接也这么说，用户以为输过密码，
   *  实际 rclone 发空密码挂载报 logon invalid —— 提示必须说真话。 */
  const savedPass = editing ? !!initial?.has_password : false;
  /** 这些协议没密码挂不上（S3 有公开桶，secret 允许为空） */
  const needsPass = protocol !== "s3";
  /** 没存过密码又没填：不许保存，别再造出一条「看起来有密码」的连接 */
  const passMissing = needsPass && !anon && !savedPass && password === "";
  /** 加密密码：编辑时留空表示沿用已存的；新建 / 从没存过就必须填 */
  const savedCryptPw = editing ? !!initial?.has_crypt_password : false;
  const cryptPwMissing = crypt && !savedCryptPw && cryptPw.trim() === "";

  const portNum = Number(portText);
  const port = portNum >= 1 && portNum <= 65535 ? portNum : meta.port;
  /** 自定义模式下没填或超出范围：不许保存，也提示一下 */
  const portBad = portCustom && !(portNum >= 1 && portNum <= 65535);

  /** 下拉分组顺序：当前协议排最前，省得每次往下翻 */
  const portGroups = useMemo(
    () =>
      (["smb", "webdav", "ftp", "sftp"] as Protocol[]).sort((a, b) =>
        a === protocol ? -1 : b === protocol ? 1 : 0
      ),
    [protocol]
  );

  // 新建连接时挂载点用后端给的平台默认值（Linux 家目录、macOS /Volumes）：
  // /mnt 属于 root 普通用户建不了目录，macOS 挂家目录 Finder 又看不见。
  // 编辑已有连接不动它的设置。
  useEffect(() => {
    if (editing || win) return;
    let alive = true;
    api
      .defaultMountTarget()
      .then((p) => {
        if (alive) setTarget((cur) => (cur.trim() ? cur : p));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const payload = useMemo<ConnectionInput>(
    () => ({
      id: initial?.id ?? null,
      name,
      protocol,
      host,
      port,
      path,
      user,
      domain,
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
      group,
      use_system_proxy: sysProxy,
      probe_mode: probeMode,
      extra_args: extraArgs,
      crypt,
      crypt_filename: cryptFilename,
      crypt_dir_names: cryptDirNames,
      crypt_password: cryptPw,
      crypt_salt: cryptSalt,
      crypt_clear: false,
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
      domain,
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
      group,
      sysProxy,
      probeMode,
      extraArgs,
      crypt,
      cryptFilename,
      cryptDirNames,
      cryptPw,
      cryptSalt,
    ]
  );

  // 盘符/挂载点预检：输入停下来 350ms 再问后端，避免每敲一个字都查一次盘符表
  useEffect(() => {
    let alive = true;
    const timer = window.setTimeout(() => {
      api
        .checkTarget(target, initial?.id ?? "")
        .then((r) => {
          if (!alive) return;
          if (r.available) {
            setTargetWarn("");
          } else if (r.reason === "other-connection") {
            setTargetWarn(t("cf.targetByConn", { name: r.owner }));
          } else {
            setTargetWarn(t("cf.targetBusy"));
          }
        })
        .catch(() => alive && setTargetWarn(""));
    }, 350);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [target, initial?.id, t]);

  function changeProtocol(p: Protocol) {
    setProtocol(p);
    // 换协议就把端口回到该协议的常用值，用户改过的手填端口不再沿用
    setPortText(String(PROTOCOL_META[p].port));
    setPortCustom(false);
    setResult(null);
    if (!editing && win) setTarget(target || "Z:");
  }

  /** 下拉选端口：'custom' 是最后一项「自定义…」 */
  function pickPort(v: string) {
    if (v === "custom") {
      setPortCustom(true);
      return;
    }
    setPortCustom(false);
    setPortText(v);
  }

  /** 下拉选项文案：`445 · SMB`、`139 · SMB · NetBIOS`、`445 · SMB · 默认` */
  function portLabel(o: { port: number; note?: string }, proto: Protocol) {
    const parts = [String(o.port), PROTOCOL_META[proto].short];
    if (o.note) parts.push(o.note);
    if (o.port === PROTOCOL_META[proto].port) parts.push(t("cf.portDefault"));
    return parts.join(" · ");
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
      closeLabel={t("cf.close")}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            {t("cf.cancel")}
          </button>
          <button className="btn" onClick={doTest} disabled={testing || !host || portBad}>
            {testing ? t("cf.testing") : t("cf.test")}
          </button>
          <button
            className="btn primary"
            onClick={doSave}
            disabled={saving || !name || !host || !!targetWarn || portBad || passMissing || cryptPwMissing}
          >
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

        <label className="field">
          <span>{t("cf.group")}</span>
          <input className="ctl" value={group} onChange={(e) => setGroup(e.target.value)} placeholder={t("cf.groupPh")} />
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
            <div className="port-box">
              <select
                className="ctl"
                value={portCustom ? "custom" : String(port)}
                onChange={(e) => pickPort(e.target.value)}
              >
                {portGroups.map((p) => (
                  <optgroup key={p} label={PROTOCOL_META[p].label}>
                    {PORT_PRESETS.filter((o) => o.proto === p).map((o) => (
                      <option key={`${p}-${o.port}`} value={String(o.port)}>
                        {portLabel(o, p)}
                      </option>
                    ))}
                  </optgroup>
                ))}
                <option value="custom">{t("cf.portCustom")}</option>
              </select>
              {portCustom && (
                <input
                  className="ctl"
                  type="number"
                  min={1}
                  max={65535}
                  value={portText}
                  onChange={(e) => setPortText(e.target.value.replace(/[^\d]/g, ""))}
                  placeholder={String(meta.port)}
                  aria-label={t("cf.port")}
                />
              )}
            </div>
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

        {protocol === "smb" && (
          <label className="field">
            <span>{t("cf.domain")}</span>
            <input
              className="ctl"
              value={domain}
              onChange={(e) => setDomain(e.target.value)}
              placeholder={t("cf.domainPh")}
              autoComplete="off"
            />
          </label>
        )}

        <label className="field">
          <span>
            {isS3 ? t("cf.secret") : t("cf.pass")}
            {savedPass && <span className="pass-badge">{t("cf.passSaved")}</span>}
          </span>
          <div className="pass-wrap">
            <input
              className="ctl"
              type={showPass ? "text" : "password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder={savedPass ? t("cf.passKeep") : t("cf.passRequired")}
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

      {targetWarn && <p className="tips bad-text">⚠ {targetWarn}</p>}

      {portBad && <p className="tips bad-text">⚠ {t("cf.portBad")}</p>}

      {!win && /^\/mnt(\/|$)/.test(target.trim()) && <p className="tips">{t("cf.rootTargetTip")}</p>}

      {protocol === "webdav" && <p className="tips">{t("cf.davTip")}</p>}

      <div className="checks">
        {(protocol === "smb" || protocol === "ftp") && (
          <label className="check">
            <input type="checkbox" checked={anon} onChange={(e) => setAnon(e.target.checked)} />
            {t("cf.anon")}
          </label>
        )}
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

      {/* 加密目录（rclone crypt）：小众功能，默认关；勾了才展开密码设置 */}
      <div className={"crypt-box" + (crypt ? " on" : "")}>
        <label className="check">
          <input type="checkbox" checked={crypt} onChange={(e) => setCrypt(e.target.checked)} />
          {t("cf.crypt")}
          <span className="crypt-tag">{t("cf.cryptTag")}</span>
        </label>
        {!crypt ? (
          <p className="tips">{t("cf.cryptHint")}</p>
        ) : (
          <div className="crypt-body">
            <p className="tips crypt-warn">⚠ {t("cf.cryptWarn")}</p>
            {savedCryptPw && <p className="tips bad-text">⚠ {t("cf.cryptChangeWarn")}</p>}
            <div className="form-grid">
              <label className="field">
                <span>
                  {t("cf.cryptPass")}
                  {savedCryptPw && <span className="pass-badge">{t("cf.passSaved")}</span>}
                </span>
                <input
                  className="ctl"
                  type="password"
                  value={cryptPw}
                  onChange={(e) => setCryptPw(e.target.value)}
                  placeholder={savedCryptPw ? t("cf.passKeep") : t("cf.cryptPassRequired")}
                  autoComplete="new-password"
                />
              </label>
              <label className="field">
                <span>{t("cf.cryptSalt")}</span>
                <input
                  className="ctl"
                  type="password"
                  value={cryptSalt}
                  onChange={(e) => setCryptSalt(e.target.value)}
                  placeholder={t("cf.cryptSaltPh")}
                  autoComplete="new-password"
                />
              </label>
              <label className="field">
                <span>{t("cf.cryptFilename")}</span>
                <select
                  className="ctl"
                  value={cryptFilename}
                  onChange={(e) => setCryptFilename(e.target.value as CryptFilename)}
                >
                  <option value="standard">{t("cf.cryptFnStandard")}</option>
                  <option value="obfuscate">{t("cf.cryptFnObfuscate")}</option>
                  <option value="off">{t("cf.cryptFnOff")}</option>
                </select>
              </label>
            </div>
            <label className={"check" + (cryptFilename === "off" ? " disabled" : "")}>
              <input
                type="checkbox"
                checked={cryptDirNames && cryptFilename !== "off"}
                disabled={cryptFilename === "off"}
                onChange={(e) => setCryptDirNames(e.target.checked)}
              />
              {t("cf.cryptDir")}
            </label>
            <p className="tips">{t("cf.cryptPerf")}</p>
          </div>
        )}
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
          <label className="field">
            <span>{t("cf.probeMode")}</span>
            <select className="ctl" value={probeMode} onChange={(e) => setProbeMode(e.target.value as ProbeMode)}>
              <option value="light">{t("pm.probe.light")}</option>
              <option value="full">{t("pm.probe.full")}</option>
              <option value="process">{t("pm.probe.process")}</option>
            </select>
          </label>
          <label className="check wide" title={t("cf.sysProxyTip")}>
            <input type="checkbox" checked={sysProxy} onChange={(e) => setSysProxy(e.target.checked)} />
            {t("cf.sysProxy")}
          </label>
          <label className="check wide">
            <input type="checkbox" checked={readOnly} onChange={(e) => setReadOnly(e.target.checked)} />
            {t("cf.readOnly")}
          </label>
          <label className="field span2">
            <span>{t("cf.extraArgs")}</span>
            <input
              className="ctl mono"
              value={extraArgs}
              onChange={(e) => setExtraArgs(e.target.value)}
              placeholder={t("cf.extraArgsPh")}
            />
            <small className="muted">{t("cf.extraArgsHint")}</small>
          </label>
        </div>
        <p className="tips">{t("cf.probeModeTip")}</p>
        <p className="tips">{t("cf.extraArgsTip")}</p>
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
