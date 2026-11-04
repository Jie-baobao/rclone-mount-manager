import { useEffect, useRef, useState } from "react";
import type { T } from "./i18n";
import { api } from "./api";

/**
 * 全屏锁定遮罩：盖住整个主窗口（含设置弹窗），密码验证通过才放行。
 *
 * 为什么只是「盖住」也够：真正的门禁在后端——锁定期间 mount / mount_all 一律拒绝，
 * 自动挂载被 supervisor 推迟，解锁信号只能由后端 lock_verify 验过密码后发出。
 * 这层 UI 负责的是把「为什么点不动」讲清楚，并收集密码。
 */
export default function LockScreen({
  t,
  onUnlocked,
}: {
  t: T;
  onUnlocked: () => void;
}) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);

  /**
   * 遮罩只挡鼠标，Tab 还能把焦点挪到底下的按钮上、敲回车就能点它。
   * 所以锁屏期间把 Tab 吃掉、焦点钉在密码框——这层界面里本来也只有它可交互。
   */
  useEffect(() => {
    const el = inputRef.current;
    el?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Tab") {
        e.preventDefault();
        el?.focus();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  async function unlock() {
    if (busy || !password) return;
    setBusy(true);
    setError("");
    try {
      const ok = await api.lockVerify(password);
      if (ok) {
        onUnlocked();
      } else {
        setError(t("lock.wrong"));
        setPassword("");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="lockscreen">
      <div className="lock-card">
        <div className="lock-icon" aria-hidden>
          🔒
        </div>
        <h2>{t("lock.title")}</h2>
        <p className="muted">{t("lock.hint")}</p>
        <input
          className="ctl"
          type="password"
          ref={inputRef}
          autoFocus
          value={password}
          placeholder={t("lock.placeholder")}
          disabled={busy}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void unlock();
          }}
        />
        {error && <p className="tips bad-text">⚠ {error}</p>}
        <button className="btn primary" onClick={() => void unlock()} disabled={busy || !password}>
          {busy ? "…" : t("lock.unlock")}
        </button>
      </div>
    </div>
  );
}
