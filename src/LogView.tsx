import { useMemo, useState } from "react";
import { copyText } from "./clipboard";
import type { T } from "./i18n";

/** 按 rclone 日志行的级别上色：ERROR/CRITICAL 红、NOTICE/WARN 黄、DEBUG 灰 */
function levelOf(line: string): string {
  const u = line.toUpperCase();
  if (u.includes("CRITICAL") || u.includes("ERROR") || u.includes("FAILED")) return "err";
  if (u.includes("NOTICE") || u.includes("WARN")) return "warn";
  if (u.includes("DEBUG")) return "dim";
  return "";
}

/**
 * 挂载日志面板：关键字过滤 + 一键复制 + 按级别着色。
 *
 * 只做前端过滤（日志本来就在内存里，几万行也只筛一遍），
 * 不动后端，刷新后过滤词仍然生效。
 */
export default function LogView({
  text,
  filter,
  onFilter,
  t,
}: {
  text: string;
  filter: string;
  onFilter: (v: string) => void;
  t: T;
}) {
  const [copied, setCopied] = useState(false);

  const lines = useMemo(() => {
    const all = text.split("\n");
    const q = filter.trim().toLowerCase();
    return q ? all.filter((l) => l.toLowerCase().includes(q)) : all;
  }, [text, filter]);

  async function copy() {
    await copyText(lines.join("\n"));
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  }

  return (
    <>
      <div className="log-tools">
        <input
          className="ctl log-filter"
          value={filter}
          onChange={(e) => onFilter(e.target.value)}
          placeholder={t("log.filterPh")}
          aria-label={t("log.filterPh")}
        />
        <span className="muted small">
          {filter.trim() ? t("log.matched", { n: lines.length }) : t("log.lines", { n: lines.length })}
        </span>
        <button className="btn ghost small" onClick={() => void copy()} disabled={!text}>
          {copied ? t("log.copied") : t("log.copy")}
        </button>
      </div>
      <pre className="log">
        {lines.map((line, i) => (
          <span key={i} className={`log-line ${levelOf(line)}`}>
            {line || " "}
            {"\n"}
          </span>
        ))}
      </pre>
    </>
  );
}
