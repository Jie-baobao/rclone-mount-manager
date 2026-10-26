import type { Stats, MountState } from "./api";
import type { T } from "./i18n";

/** 图里只画最近这么多次探测：数据量固定，节点跑一整天也不会拖慢界面 */
const WINDOW = 20;

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

/** Y 轴上限取「整齐数」：1 / 2 / 5 × 10^n，刻度才好读 */
function niceMax(v: number): number {
  if (!isFinite(v) || v <= 0) return 100;
  const pow = Math.pow(10, Math.floor(Math.log10(v)));
  const n = v / pow;
  const step = n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10;
  return step * pow;
}

/** 刻度文字：1200 → 1.2k，避免四位数字挤在一起 */
function fmtTick(v: number): string {
  if (v >= 1000) return `${(v / 1000).toFixed(v % 1000 === 0 ? 0 : 1)}k`;
  return String(Math.round(v));
}

/** 时间跨度：300 秒 → 5m，90 秒 → 90s */
function fmtSpan(secs: number): string {
  if (secs >= 3600) return `${(secs / 3600).toFixed(1)}h`;
  if (secs >= 60) return `${Math.round(secs / 60)}m`;
  return `${Math.round(secs)}s`;
}

/**
 * 延迟折线：带 Y 轴数值、横向参考网格线、阈值虚线与 X 轴时间刻度。
 * 只取最近 WINDOW 次探测（按探测间隔换算出实际时间段）。
 * 0 表示这次探测失败，用红点标在基线上。
 */
function LatencyChart({
  data,
  threshold,
  intervalSecs,
  t,
}: {
  data: number[];
  threshold: number;
  intervalSecs: number;
  t: T;
}) {
  const W = 560;
  const H = 176;
  // 上方留出 22px，给 Y 轴单位「ms」腾位置，不然会和最高刻度贴在一起
  const PAD = { l: 42, r: 16, t: 22, b: 24 };
  const plotW = W - PAD.l - PAD.r;
  const plotH = H - PAD.t - PAD.b;

  const pts = data.slice(-WINDOW);
  const values = pts.filter((v) => v > 0);
  const peak = values.length ? Math.max(...values) : 0;
  const max = niceMax(Math.max(peak * 1.2, 60));

  const stepX = pts.length > 1 ? plotW / (pts.length - 1) : 0;
  const x = (i: number) => PAD.l + i * stepX;
  const y = (v: number) => PAD.t + plotH - (Math.min(Math.max(v, 0), max) / max) * plotH;

  // Y 轴：0 / 1/4 / 1/2 / 3/4 / 上限，四条网格线 + 数值
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => Math.round(max * f));
  // 阈值线只有在画得进来的时候才画，否则会把曲线压成一条直线
  const showThreshold = threshold > 0 && threshold <= max;

  const line = pts.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const area = `${PAD.l},${y(0).toFixed(1)} ${line} ${x(pts.length - 1).toFixed(1)},${y(0).toFixed(1)}`;

  const span = fmtSpan(Math.max(pts.length - 1, 1) * Math.max(intervalSecs, 1));
  const spanHalf = fmtSpan((Math.max(pts.length - 1, 1) * Math.max(intervalSecs, 1)) / 2);

  return (
    <svg className="chart" viewBox={`0 0 ${W} ${H}`} role="img" aria-label={t("stats.history", { n: pts.length })}>
      {/* 横向参考线与 Y 轴数值 */}
      {ticks.map((v, i) => {
        const gy = y(v);
        return (
          <g key={`t${i}`}>
            <line x1={PAD.l} x2={W - PAD.r} y1={gy} y2={gy} stroke="var(--border)" strokeWidth="1" />
            <text x={PAD.l - 7} y={gy + 3.5} textAnchor="end" className="chart-tick">
              {fmtTick(v)}
            </text>
          </g>
        );
      })}
      <text x={PAD.l - 7} y={12} textAnchor="end" className="chart-unit">
        ms
      </text>

      {/* 阈值虚线 */}
      {showThreshold && (
        <>
          <line
            x1={PAD.l}
            x2={W - PAD.r}
            y1={y(threshold)}
            y2={y(threshold)}
            stroke="var(--warn)"
            strokeDasharray="4 4"
            strokeWidth="1"
            opacity="0.75"
          />
          <text x={W - PAD.r} y={y(threshold) - 4} textAnchor="end" className="chart-warn">
            {fmtTick(threshold)}
          </text>
        </>
      )}

      {/* 曲线与浅色面积 */}
      <polygon points={area} fill="var(--accent)" opacity="0.08" />
      <polyline
        points={line}
        fill="none"
        stroke="var(--accent)"
        strokeWidth="1.8"
        strokeLinejoin="round"
        strokeLinecap="round"
      />

      {/* 探测失败点 */}
      {pts.map((v, i) =>
        v === 0 ? <circle key={`f${i}`} cx={x(i)} cy={y(0)} r="2.2" fill="var(--danger)" /> : null
      )}

      {/* 最新一次的值 */}
      <circle cx={x(pts.length - 1)} cy={y(pts[pts.length - 1])} r="2.8" fill="var(--accent)" />

      {/* X 轴时间刻度 */}
      <line
        x1={PAD.l}
        x2={W - PAD.r}
        y1={PAD.t + plotH}
        y2={PAD.t + plotH}
        stroke="var(--border)"
        strokeWidth="1"
      />
      <text x={PAD.l} y={H - 7} className="chart-tick">
        −{span}
      </text>
      <text x={PAD.l + plotW / 2} y={H - 7} textAnchor="middle" className="chart-tick">
        −{spanHalf}
      </text>
      <text x={W - PAD.r} y={H - 7} textAnchor="end" className="chart-tick">
        {t("stats.now")}
      </text>
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
  intervalSecs,
  t,
}: {
  stats: Stats | null;
  state: MountState | undefined;
  threshold: number;
  intervalSecs: number;
  t: T;
}) {
  const s = stats;
  const ok = s?.probes_ok ?? 0;
  const fail = s?.probes_fail ?? 0;
  const total = ok + fail;
  const rate = total > 0 ? Math.round((ok / total) * 100) : null;
  const mounted = !!state?.mounted;

  const window = (s?.history ?? []).slice(-WINDOW);
  const good = window.filter((v) => v > 0);
  const min = good.length ? Math.min(...good) : 0;
  const max = good.length ? Math.max(...good) : 0;
  const avg = good.length ? Math.round(good.reduce((a, b) => a + b, 0) / good.length) : 0;

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
          <span>{t("stats.history", { n: window.length || WINDOW })}</span>
          <span className="muted small">{t("stats.threshold", { v: threshold })}</span>
        </div>
        {window.length >= 2 ? (
          <>
            <LatencyChart data={window} threshold={threshold} intervalSecs={intervalSecs} t={t} />
            <p className="chart-legend muted small">
              {t("stats.range", { min: Math.round(min), avg, max: Math.round(max) })}
            </p>
          </>
        ) : (
          <p className="muted small">{t("stats.noHistory")}</p>
        )}
      </div>
    </section>
  );
}
