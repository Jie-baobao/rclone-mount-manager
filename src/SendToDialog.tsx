import { useCallback, useEffect, useState } from "react";
import Modal from "./Modal";
import {
  api,
  events,
  fmtBytes,
  type DirEntry,
  type SendProgress,
  type SendReport,
  type SendRequest,
} from "./api";
import type { T } from "./i18n";

const baseName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

/** "Z:" → "Z:\"（Windows 盘符），类 Unix 的挂载点原样返回 */
function rootOf(target: string): string {
  const v = target.trim();
  return /^[a-zA-Z]:$/.test(v) ? `${v}\\` : v;
}

function parentOf(p: string): string {
  const s = p.replace(/[\\/]+$/, "");
  const i = Math.max(s.lastIndexOf("\\"), s.lastIndexOf("/"));
  if (i < 0) return p;
  const head = s.slice(0, i);
  return /^[a-zA-Z]:$/.test(head) ? `${head}\\` : head || "/";
}

/**
 * 「发送到」确认窗口：资源管理器点了菜单项之后弹这个。
 *
 * 刻意不做完整的目录树浏览器——用户要的是「把文件丢到那儿」，
 * 给他能在当前目录里点进子目录、能一键回到「最近用过的位置」就够了。
 */
export default function SendToDialog({
  req,
  connName,
  t,
  onClose,
  notify,
}: {
  req: SendRequest;
  connName: string;
  t: T;
  onClose: () => void;
  notify: (kind: "ok" | "bad", text: string) => void;
}) {
  const [dest, setDest] = useState(() => rootOf(req.target));
  const [pathInput, setPathInput] = useState(() => rootOf(req.target));
  const [dirs, setDirs] = useState<DirEntry[]>([]);
  const [dirErr, setDirErr] = useState("");
  const [recent, setRecent] = useState<string[]>([]);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<SendProgress | null>(null);
  const [report, setReport] = useState<SendReport | null>(null);

  useEffect(() => {
    setPathInput(dest);
  }, [dest]);

  // 换目录就重新列一次子目录；切得快时用 alive 丢掉过期的响应
  useEffect(() => {
    let alive = true;
    api
      .sendtoDirs(dest)
      .then((list) => {
        if (!alive) return;
        setDirs(list);
        setDirErr("");
      })
      .catch((e) => {
        if (!alive) return;
        setDirs([]);
        setDirErr(String(e));
      });
    return () => {
      alive = false;
    };
  }, [dest]);

  useEffect(() => {
    api.sendtoRecent(req.target).then(setRecent).catch(() => undefined);
  }, [req.target]);

  // 进度与结果由后端推：传大文件时窗口一直开着，关掉也不影响后台传输
  useEffect(() => {
    const uns: Array<() => void> = [];
    events.onSendtoProgress(setProgress).then((u) => uns.push(u));
    events
      .onSendtoDone((r) => {
        setReport(r);
        setRunning(false);
      })
      .then((u) => uns.push(u));
    return () => uns.forEach((f) => f());
  }, []);

  const start = useCallback(
    async (mode: "copy" | "move") => {
      setReport(null);
      setProgress(null);
      setRunning(true);
      try {
        await api.sendtoRun(req.files, dest, req.target, mode);
      } catch (e) {
        setRunning(false);
        notify("bad", String(e));
      }
    },
    [dest, notify, req.files, req.target]
  );

  const pct =
    progress && progress.total_bytes > 0
      ? Math.min(100, Math.round((progress.done_bytes / progress.total_bytes) * 100))
      : 0;

  const footer = report ? (
    <>
      <button className="btn ghost" onClick={onClose}>
        {t("sendto.close")}
      </button>
      <button
        className="btn primary"
        onClick={() => void api.openPath(report.dest).catch(() => undefined)}
      >
        {t("sendto.openDest")}
      </button>
    </>
  ) : running ? (
    <>
      <span className="muted small grow-left">{t("sendto.keepRunning")}</span>
      <button className="btn ghost" onClick={() => void api.sendtoCancel()}>
        {t("sendto.cancel")}
      </button>
    </>
  ) : (
    <>
      <button className="btn ghost" onClick={onClose}>
        {t("sendto.close")}
      </button>
      <button className="btn primary" disabled={!!dirErr} onClick={() => void start("copy")}>
        {t("sendto.copyHere")}
      </button>
      <button className="btn" disabled={!!dirErr} onClick={() => void start("move")}>
        {t("sendto.moveHere")}
      </button>
    </>
  );

  return (
    <Modal
      title={t("sendto.title", { name: connName })}
      subtitle={t("sendto.subtitle", { n: req.files.length })}
      onClose={onClose}
      footer={footer}
      size="sendto"
      closeLabel={t("sendto.close")}
    >
      <div className="sendto">
        <div className="sendto-files">
          <div className="sendto-files-head">
            <strong>{t("sendto.files", { n: req.files.length })}</strong>
            {progress && progress.total_bytes > 0 && (
              <span className="muted small">{fmtBytes(progress.total_bytes)}</span>
            )}
          </div>
          <ul>
            {req.files.slice(0, 6).map((f) => (
              <li key={f} className="mono ellipsis" title={f}>
                {baseName(f)}
              </li>
            ))}
            {req.files.length > 6 && (
              <li className="muted small">{t("sendto.moreFiles", { n: req.files.length - 6 })}</li>
            )}
          </ul>
        </div>

        {!report && !running && (
          <>
            <div className="sendto-path">
              <span className="field-label">{t("sendto.dest")}</span>
              <div className="pass-row">
                <input
                  className="ctl mono"
                  value={pathInput}
                  onChange={(e) => setPathInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") setDest(pathInput.trim());
                  }}
                  spellCheck={false}
                />
                <button className="btn ghost small" onClick={() => setDest(pathInput.trim())}>
                  {t("sendto.go")}
                </button>
                <button className="btn ghost small" onClick={() => setDest(parentOf(dest))}>
                  {t("sendto.up")}
                </button>
              </div>
            </div>

            {recent.length > 0 && (
              <div className="sendto-recent">
                <span className="muted small">{t("sendto.recent")}</span>
                {recent.map((p) => (
                  <button
                    key={p}
                    className="chip"
                    title={p}
                    onClick={() => setDest(p)}
                    disabled={p === dest}
                  >
                    {baseName(p) || p}
                  </button>
                ))}
              </div>
            )}

            <div className="dir-list">
              {dirErr ? (
                <p className="muted small">{dirErr}</p>
              ) : dirs.length === 0 ? (
                <p className="muted small">{t("sendto.noSubdir")}</p>
              ) : (
                dirs.map((d) => (
                  <button
                    key={d.path}
                    className="dir-item"
                    title={d.path}
                    onClick={() => setDest(d.path)}
                  >
                    <span className="dir-icon" aria-hidden="true">
                      📁
                    </span>
                    <span className="ellipsis">{d.name}</span>
                  </button>
                ))
              )}
            </div>
          </>
        )}

        {(running || (progress && !report)) && (
          <div className="sendto-progress">
            <div className="bar">
              <span style={{ width: `${pct}%` }} />
            </div>
            <div className="muted small">
              {progress?.phase === "planning"
                ? t("sendto.planning")
                : progress
                ? t("sendto.progress", {
                    done: progress.done_files,
                    total: progress.total_files,
                    bytes: fmtBytes(progress.done_bytes),
                    size: fmtBytes(progress.total_bytes),
                  })
                : ""}
            </div>
            {progress?.current && (
              <div className="muted small ellipsis" title={progress.current}>
                {progress.current}
              </div>
            )}
          </div>
        )}

        {report && (
          <div className={`sendto-report ${report.ok ? "ok" : "bad"}`}>
            <p>
              {report.error
                ? report.error
                : t("sendto.report", {
                    copied: report.copied,
                    skipped: report.skipped,
                    failed: report.failed,
                  })}
            </p>
            {report.details.length > 0 && (
              <details>
                <summary>{t("sendto.details", { n: report.details.length })}</summary>
                <ul>
                  {report.details.map((d, i) => (
                    <li key={i} className="small mono">
                      {d}
                    </li>
                  ))}
                </ul>
              </details>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}
