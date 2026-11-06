import { useMemo, useState } from "react";
import Modal from "./Modal";
import { api, type Conflict } from "./api";
import type { T } from "./i18n";

/**
 * 挂载前发现位置被占用的统一弹窗。
 *
 * 刻意不做自动顺延：盘符一改，桌面快捷方式、批处理脚本里的 `Z:` 就全废了。
 * 所以这里只列出「哪条连接、卡在哪个位置、为什么」，让用户自己挑新位置——
 * 多条冲突合并成一个弹窗，一次改完，不用一条条点。
 */
export default function ConflictDialog({
  conflicts,
  t,
  onClose,
  notify,
  onResolved,
}: {
  conflicts: Conflict[];
  t: T;
  onClose: () => void;
  notify: (kind: "ok" | "bad", text: string) => void;
  onResolved: () => void;
}) {
  // 每行预填第一个候选，用户不动也能直接确定；没有候选就留空让他手填
  const [picks, setPicks] = useState<Record<string, string>>(() =>
    Object.fromEntries(conflicts.map((c) => [c.id, c.candidates[0] ?? ""]))
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  // 两行选到同一个位置的话，后端会整批拒绝，不如在这儿先说清楚
  const dup = useMemo(() => {
    const seen = new Map<string, number>();
    for (const v of Object.values(picks)) {
      const k = v.trim().toLowerCase();
      if (k) seen.set(k, (seen.get(k) ?? 0) + 1);
    }
    return [...seen.entries()].find(([, n]) => n > 1)?.[0] ?? "";
  }, [picks]);

  const ready = conflicts.every((c) => (picks[c.id] ?? "").trim() !== "") && !dup;

  async function apply() {
    setBusy(true);
    setError("");
    try {
      const n = await api.resolveConflicts(
        conflicts.map((c) => ({ id: c.id, target: (picks[c.id] ?? "").trim() }))
      );
      notify("ok", t("toast.conflictsFixed", { n }));
      onResolved();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={t("cfx.title")}
      subtitle={t("cfx.desc")}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose} disabled={busy}>
            {t("cfx.later")}
          </button>
          <button className="btn primary" onClick={apply} disabled={!ready || busy}>
            {busy ? t("cfx.applying") : t("cfx.apply")}
          </button>
        </>
      }
    >
      <div className="conflict-list">
        {conflicts.map((c) => {
          const reason =
            c.reason === "other-connection"
              ? t("cfx.reasonConn", { name: c.owner })
              : t("cfx.reasonOccupied");
          return (
            <div className="conflict-row" key={c.id}>
              <div className="conflict-who">
                <strong>{c.name}</strong>
                <span className="muted small">
                  {c.target} · {reason}
                </span>
              </div>
              <label className="conflict-pick">
                <span className="muted small">{t("cfx.to")}</span>
                <input
                  className="ctl"
                  list={`cfx-cand-${c.id}`}
                  value={picks[c.id] ?? ""}
                  placeholder={c.candidates.length ? "" : t("cfx.noCandidate")}
                  onChange={(e) => setPicks((p) => ({ ...p, [c.id]: e.target.value }))}
                />
                <datalist id={`cfx-cand-${c.id}`}>
                  {c.candidates.map((v) => (
                    <option key={v} value={v} />
                  ))}
                </datalist>
              </label>
            </div>
          );
        })}
      </div>

      {dup && <p className="tips bad-text">{t("cfx.dup", { target: dup })}</p>}
      {error && <p className="tips bad-text">{error}</p>}
      <p className="tips">{t("cfx.note")}</p>
    </Modal>
  );
}
