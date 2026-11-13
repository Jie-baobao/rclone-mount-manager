import { fmtBytes, type TrafficPoint } from "./api";
import type { T } from "./i18n";

/**
 * 近 7 天每日流量：上行 / 下行两根柱子并排，高度按 7 天最大值归一。
 * 数据来自后端落盘的 traffic_history.json（跨重启保留）。
 * 悬停一列可以看到那天的具体字节数。
 */
export default function TrafficChart({ points, t }: { points: TrafficPoint[]; t: T }) {
  if (points.length === 0 || points.every((p) => p.up === 0 && p.down === 0)) {
    return <p className="muted small">{t("traffic.histEmpty")}</p>;
  }
  const max = Math.max(...points.map((p) => Math.max(p.up, p.down)), 1);
  return (
    <div className="tchart">
      <div className="tchart-legend muted small">
        <span>
          <i className="tchart-sw up" />
          {t("traffic.up")}
        </span>
        <span>
          <i className="tchart-sw down" />
          {t("traffic.down")}
        </span>
        <span className="tchart-max">
          {t("traffic.histMax", { v: fmtBytes(max) })}
        </span>
      </div>
      <div className="tchart-bars">
        {points.map((p) => (
          <div
            className="tchart-day"
            key={p.date}
            title={`${p.date} · ${t("traffic.up")} ${fmtBytes(p.up)} · ${t("traffic.down")} ${fmtBytes(p.down)}`}
          >
            <div className="tchart-cols">
              <i
                className="tchart-bar up"
                style={{ height: `${Math.max(Math.round((p.up / max) * 100), 2)}%` }}
              />
              <i
                className="tchart-bar down"
                style={{ height: `${Math.max(Math.round((p.down / max) * 100), 2)}%` }}
              />
            </div>
            <span className="tchart-label mono">{p.date.slice(5)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
