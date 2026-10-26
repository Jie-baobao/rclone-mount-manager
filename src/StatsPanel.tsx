import type { Stats, MountState } from "./api";
import type { T } from "./i18n";

function fmtBytes(n: number): string {
  if (!n) return "0";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

function fmtSpeed(bps: number): string {
  if (!bps) return "0 B/s";
  return `${fmtBytes(bps)}/s`;
}

/** 延迟折线：0 表示这次探测失败，用红点标出来；虚线是当前阈值 */
function Spark({ data, threshold }: { data: number[]; threshold: number }) {
  const W = 300;
  const H = 56;
  if (data.length < 2) return null;
  // 按实际延迟缩放，曲线才有起伏；阈值线只有在能画进来时才画
  const peak = Math.max(...data.filter((v) => v > 0), 1);
  const max = Math.max(peak * 1.25, 60);
  const showThreshold = threshold <= max * 1.05;
  const step = W / Math.max(data.length - 1, 1);
  const y = (v: number) => H - 3 - (Math.min(v, max) / max) * (H - 12);
  const points = data.map((v, i) => `${(i * step).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const thresholdY = y(threshold);

  return (
    <svg className="spark" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img">
      {showThreshold && (
        <line
          x1="0"
          x2={W}
          y1={thresholdY}
          y2={thresholdY}
          stroke="var(--warn)"
          strokeDasharray="4 4"
          strokeWidth="1"
          opacity="0.55"
        />
      )}      <polyline
        points={points}
        fill="none"
        stroke="var(--accent)"
        strokeWidth="1.6"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
      {data.map((v, i) =>
        v === 0 ? (
          <circle key={i} cx={(i * step).toFixed(1)} cy={H - 3} r="2" fill="var(--bad, #dc2626)" />
        ) : null
      )}
    </svg>
  );
}

/**
 * 详情页的「节点概况」：带宽、传输量、探测成功率、延迟趋势。
 * 数据全部来自后端内存里的采样（5 秒一次本机 rc 查询），不落盘。
 */
export default function StatsPanel({
  stats,
  state,
  threshold,
  t,
}: {
  stats: Stats | null;
  state: MountState | undefined;
  threshold: number;
  t: T;
}) {
  const s = stats;
  const ok = s?.probes_ok ?? 0;
  const fail = s?.probes_fail ?? 0;
  const total = ok + fail;
  const rate = total > 0 ? Math.round((ok / total) * 100) : null;
  const mounted = !!state?.mounted;

  return (
    <section className="panel">
      <div className="panel-head">
        <h3>{t("stats.title")}</h3>
        <div className="panel-tools">
          <span className="muted small">{mounted ? t("stats.note") : t("stats.idle")}</span>
        </div>
      </div>

      <div className="stat-grid">
        <div className="stat">
          <label>{t("stats.speed")}</label>
          <strong>{fmtSpeed(s?.speed_bps ?? 0)}</strong>
          <small>{t("stats.peak", { v: fmtSpeed(s?.peak_bps ?? 0) })}</small>
        </div>
        <div className="stat">
          <label>{t("stats.total")}</label>
          <strong>{fmtBytes(s?.total_bytes ?? 0)}</strong>
          <small>{t("stats.transfers", { n: s?.transfers ?? 0, e: s?.errors ?? 0 })}</small>
        </div>
        <div className="stat">
          <label>{t("stats.success")}</label>
          <strong>{rate == null ? "—" : `${rate}%`}</strong>
          <small>{t("stats.probes", { ok, fail })}</small>
        </div>
        <div className="stat">
          <label>{t("stats.latency")}</label>
          <strong>{s?.latency_ms != null ? `${s.latency_ms} ms` : "—"}</strong>
          <small>{t("stats.failRate", { v: rate == null ? "—" : 100 - rate })}</small>
        </div>
      </div>

      <div className="spark-wrap">
        <div className="spark-head">
          <span>{t("stats.history")}</span>
          <span className="muted small">{t("stats.threshold", { v: threshold })}</span>
        </div>
        {s && s.history.length >= 2 ? (
          <Spark data={s.history} threshold={threshold} />
        ) : (
          <p className="muted small">{t("stats.noHistory")}</p>
        )}
      </div>
    </section>
  );
}
