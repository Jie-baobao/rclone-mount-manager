import { useEffect, useMemo, useRef, useState } from "react";
import SendToDialog from "./SendToDialog";
import { api, events, type Connection, type SendRequest } from "./api";
import { makeT, resolveLanguage } from "./i18n";

/**
 * 「发送到」独立小窗的根组件。
 *
 * 为什么要有这个窗口：用户在资源管理器里右键 → 发送到 → 某个挂载盘时，
 * 主界面往往是收在托盘里的后台状态。为了问一句「放到哪个目录」就把整个
 * 管理器拽到前台太重了，所以这里只开一个 720×680 的小窗，里面就是确认框本身，
 * 主窗口该藏着就继续藏着。
 *
 * 这个窗口只干三件事：取请求、渲染确认框、关掉自己。传输本身在后端线程里跑，
 * 窗口关掉不影响。
 */
export default function SendToWindow() {
  const [req, setReq] = useState<SendRequest | null>(null);
  const [conns, setConns] = useState<Connection[]>([]);
  const [lang, setLang] = useState("system");
  const [toast, setToast] = useState<{ kind: "ok" | "bad"; text: string } | null>(null);
  const toastTimer = useRef<number | null>(null);

  const t = useMemo(() => makeT(resolveLanguage(lang)), [lang]);

  const notify = (kind: "ok" | "bad", text: string) => {
    setToast({ kind, text });
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 4500);
  };

  // 取请求 → 首帧画完 → 再显示窗口（后端建窗时是隐藏的，避免白闪）
  useEffect(() => {
    let shown = false;
    const show = () => {
      if (shown) return;
      shown = true;
      void api.sendtoWindowReady().catch(() => undefined);
    };

    api.getSettings().then((s) => setLang(s.language)).catch(() => undefined);
    api.list().then(setConns).catch(() => undefined);
    api
      .sendtoTake()
      .then((r) => {
        if (r) setReq(r);
      })
      .catch(() => undefined)
      .then(show, show);
    // 兜底：万一取请求卡住，也别让窗口一直不出现
    const timer = window.setTimeout(show, 1500);

    // 同一个窗口还在的时候又来一次「发送到」：直接换成新任务
    const un = events.onSendtoRequest((r) => setReq(r));
    return () => {
      window.clearTimeout(timer);
      un.then((f) => f()).catch(() => undefined);
    };
  }, []);

  // 连接已经被删掉时退回显示盘符
  const connName = useMemo(() => {
    if (!req) return "";
    const key = req.target.trim().toUpperCase();
    const c = conns.find((x) => x.target.trim().toUpperCase() === key);
    if (!c) return req.target;
    return c.group.trim() ? `[${c.group}] ${c.name}` : c.name;
  }, [conns, req]);

  const close = () => {
    void api.sendtoWindowClose().catch(() => undefined);
  };

  return (
    <div className="sendto-window">
      {req ? (
        <SendToDialog
          // 换任务时整块重置：目标目录、进度、结果都要回到初始状态
          key={`${req.target}|${req.files.join("\u0001")}`}
          req={req}
          connName={connName}
          t={t}
          onClose={close}
          notify={notify}
          standalone
        />
      ) : (
        <div className="sendto-waiting muted small">{t("sendto.preparing")}</div>
      )}
      {toast && <div className={`toast ${toast.kind}`}>{toast.text}</div>}
    </div>
  );
}
