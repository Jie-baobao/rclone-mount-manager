import { useState } from "react";
import {
  api,
  fmtBytes,
  fmtLatency,
  probeKindLabel,
  type ProbeMode,
  type Stats,
  type MountState,
} from "./api";
import TrafficBar from "./TrafficBar";
import type { T } from "./i18n";

/** 图里只画最近这么多次探测：数据量固定，节点跑一整天也不会拖慢界面 */
const WINDOW = 20;

function fmtSpeed(bps: number): string {
  if (!bps) return "0 B/s";
  return `${fmtBytes(bps)}/s`;
}

/**
 * Y 轴步长：从「整齐数」里挑一个，让 max = 步长 × 4 刚好容得下峰值。
 * 亚毫秒也要有得选（0.1 / 0.2 ms），否则轻量探测的曲线永远贴在基线上。
 */
const STEPS_MS = [0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000];
const STEPS_MS_BIG = [10000, 20000, 50000, 100000, 200000];

function pickStep(peakMs: number): number {
  const need = peakMs * 1.15;
  for (const s of [...STEPS_MS, ...STEPS_MS_BIG]) {
    if (s * 4 >= need) return s;
  }
  return 500000;
}

/** 刻度文字：0.1 → "0.1"，10 → "10"，1500 → "1.5k" */
function fmtTick(ms: number, step: number): string {
  if (ms >= 1000) return `${(ms / 1000).toFixed(ms % 1000 === 0 ? 0 : 1)}k`;
  if (step < 1) return ms.toFixed(1);
  return String(Math.round(ms));
}

/** 时间跨度：300 秒 → 5m，90 秒 → 90s */
function fmtSpan(secs: number): string {
  if (secs >= 3600) return `${(secs / 3600).toFixed(1)}h`;
  if (secs >= 60) return `${Math.round(secs / 60)}m`;
  return `${Math.round(secs)}s`;
}

/**
 * 延迟折线：带 Y 轴数值、横向参考网格线、阈值虚线与 X 轴时间刻度。
 * 输入单位是**微秒**，纵轴按毫秒显示并自适应刻度（0.1ms 起）。
 * 只取最近 WINDOW 次探测；0 表示这次探测失败，用红点标在基线上。
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
  const PAD = { l: 46, r: 16, t: 22, b: 24 };
  const plotW = W - PAD.l - PAD.r;
  const plotH = H - PAD.t - PAD.b;

  const pts = data.slice(-WINDOW);
  const values = pts.filter((v) => v > 0);
  const peakMs = values.length ? Math.max(...values) / 1000 : 0;
  const step = pickStep(peakMs);
  const maxMs = step * 4;

  const stepX = pts.length > 1 ? plotW / (pts.length - 1) : 0;
  const x = (i: number) => PAD.l + i * stepX;
  const y = (us: number) => PAD.t + plotH - (Math.min(us / 1000, maxMs) / maxMs) * plotH;

  // Y 轴：0 / 1/4 / 1/2 / 3/4 / 上限，四条网格线 + 数值
  const ticks = [0, 1, 2, 3, 4].map((i) => i * step);
  // 阈值线只有在画得进来的时候才画，否则会把曲线压成一条直线
  const showThreshold = threshold > 0 && threshold <= maxMs;

  const line = pts.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const area = `${PAD.l},${y(0).toFixed(1)} ${line} ${x(pts.length - 1).toFixed(1)},${y(0).toFixed(1)}`;

  const span = fmtSpan(Math.max(pts.length - 1, 1) * Math.max(intervalSecs, 1));
  const spanHalf = fmtSpan((Math.max(pts.length - 1, 1) * Math.max(intervalSecs, 1)) / 2);

  return (
    <svg
      className="chart"
      viewBox={`0 0 ${W} ${H}`}
      role="img"
      aria-label={t("stats.history", { n: pts.length })}
    >
      {/* 横向参考线与 Y 轴数值 */}
      {ticks.map((v, i) => {
        const gy = y(v * 1000);
        return (
          <g key={`t${i}`}>
            <line x1={PAD.l} x2={W - PAD.r} y1={gy} y2={gy} stroke="var(--border)" strokeWidth="1" />
            <text x={PAD.l - 7} y={gy + 3.5} textAnchor="end" className="chart-tick">
              {fmtTick(v, step)}
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
            y1={y(threshold * 1000)}
            y2={y(threshold * 1000)}
            stroke="var(--warn)"
            strokeDasharray="4 4"
            strokeWidth="1"
            opacity="0.75"
          />
          <text x={W - PAD.r} y={y(threshold * 1000) - 4} textAnchor="end" className="chart-warn">
            {fmtTick(threshold, step)}
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
 *
 * 延迟数字的口径取决于探测方式：轻量=TCP 握手（通常 <1ms，不代表读写延迟），
 * 完整=列一次目录的耗时，仅进程=没有网络往返，不显示数字。
 */
export default function StatsPanel({
  stats,
  state,
  mode,
  threshold,
  intervalSecs,
  onReset,
  t,
}: {
  stats: Stats | null;
  state: MountState | undefined;
  mode: ProbeMode;
  threshold: number;
  intervalSecs: number;
  /** 清零后让父组件立即重拉一次统计 */
  onReset?: () => void;
  t: T;
}) {
  const s = stats;
  const ok = s?.probes_ok ?? 0;
  const fail = s?.probes_fail ?? 0;
  const total = ok + fail;
  const rate = total > 0 ? Math.round((ok / total) * 100) : null;
  const mounted = !!state?.mounted;
  const tracksLatency = mode !== "process";

  // 折叠状态记在本地：高级设置是原生 details，这里也一样，只是多记一个开关
  const [open, setOpen] = useState(() => {
    try {
      return localStorage.getItem("rmm.statsOpen") !== "0";
    } catch {
      return true;
    }
  });

  // 「测一次真实延迟」：用户主动点才跑完整探测，只跑一次，不参与定时轮询
  const [measuring, setMeasuring] = useState(false);
  const [real, setReal] = useState<{ ok: boolean; text: string } | null>(null);

  async function measureOnce() {
    if (!state?.id || measuring) return;
    setMeasuring(true);
    try {
      const us = await api.probeNow(state.id);
      setReal({ ok: true, text: t("stats.realResult", { v: fmtLatency(us) }) });
    } catch (e) {
      setReal({ ok: false, text: t("stats.realFail", { e: String(e) }) });
    } finally {
      setMeasuring(false);
    }
  }

  const window = (s?.history ?? []).slice(-WINDOW);
  const good = window.filter((v) => v > 0);
  const min = good.length ? Math.min(...good) : 0;
  const max = good.length ? Math.max(...good) : 0;
  const avg = good.length ? Math.round(good.reduce((a, b) => a + b, 0) / good.length) : 0;

  // 把这次挂载的计数清零：想单独量一次拷贝花了多少流量时用
  async function resetFlow() {
    if (!state?.id) return;
    try {
      await api.resetStats(state.id);
      onReset?.();
    } catch {
      /* 挂载已停等场景直接忽略，下次采样会覆盖 */
    }
  }

  return (
    <details
      className="panel details"
      open={open}
      onToggle={(e) => {
        const next = e.currentTarget.open;
        setOpen(next);
        try {
          localStorage.setItem("rmm.statsOpen", next ? "1" : "0");
        } catch {
          /* 隐私模式下写不了，纯体验问题 */
        }
      }}
    >
      <summary className="panel-summary">
        <h3>{t("stats.title")}</h3>
        <span className="muted small">{mounted ? t("stats.note") : t("stats.idle")}</span>
      </summary>

      <div className="stat-grid">
        <div className="stat">
          <label>{t("stats.speed")}</label>
          <strong>{fmtSpeed(s?.speed_bps ?? 0)}</strong>
          <small>{t("stats.peak", { v: fmtSpeed(s?.peak_bps ?? 0) })}</small>
        </div>
        <div className="stat has-reset" title={t("stats.totalHint")}>
          <label>{t("stats.total")}</label>
          <strong>{fmtBytes(s?.total_bytes ?? 0)}</strong>
          <small>{t("stats.transfers", { n: s?.transfers ?? 0, e: s?.errors ?? 0 })}</small>
          {mounted && (
            <button
              className="stat-reset"
              title={t("stats.resetTip")}
              aria-label={t("stats.reset")}
              onClick={() => void resetFlow()}
            >
              ↺
            </button>
          )}
        </div>
        <div className="stat">
          <label>{t("stats.success")}</label>
          <strong>{rate == null ? "—" : `${rate}%`}</strong>
          <small>{t("stats.probes", { ok, fail })}</small>
        </div>
        <div className="stat">
          <label>{tracksLatency ? t("stats.latency") : t("stats.latencyNone")}</label>
          <strong>{tracksLatency ? fmtLatency(s?.latency_us) : "—"}</strong>
          <small>
            {tracksLatency
              ? `${probeKindLabel(mode, t)} · ${t("stats.failRate", { v: rate == null ? "—" : 100 - rate })}`
              : t("stats.probeNa")}
          </small>
        </div>
      </div>

      {/* 单条连接的上传/下载合计（今日 + 累计），数据来自后端落盘账本 */}
      <TrafficBar
        todayUp={s?.today_up ?? 0}
        todayDown={s?.today_down ?? 0}
        totalUp={s?.total_up ?? 0}
        totalDown={s?.total_down ?? 0}
        t={t}
      />

      <div className="spark-wrap">
        <div className="spark-head">
          <span>{t(mode === "light" ? "stats.historyTcp" : "stats.history", { n: window.length || WINDOW })}</span>
          <div className="spark-tools">
            {tracksLatency && mode === "full" && (
              <span className="muted small">{t("stats.threshold", { v: threshold })}</span>
            )}
            {tracksLatency && mounted && (
              <>
                {real && (
                  <span className={`small ${real.ok ? "muted" : "bad-text"}`}>{real.text}</span>
                )}
                <button
                  className="btn ghost small"
                  disabled={measuring}
                  title={t("stats.measureTip")}
                  onClick={() => void measureOnce()}
                >
                  {measuring ? t("stats.measuring") : t("stats.measureReal")}
                </button>
              </>
            )}
          </div>
        </div>
        {tracksLatency && window.length >= 2 ? (
          <>
            <LatencyChart data={window} threshold={threshold} intervalSecs={intervalSecs} t={t} />
            <p className="chart-legend muted small">
              {t("stats.range", {
                min: fmtLatency(min),
                avg: fmtLatency(avg),
                max: fmtLatency(max),
              })}
            </p>
            {mode === "light" && <p className="chart-legend muted small">{t("stats.tcpNote")}</p>}
          </>
        ) : (
          <p className="muted small">{tracksLatency ? t("stats.noHistory") : t("stats.probeNa")}</p>
        )}
      </div>
    </details>
  );
}
