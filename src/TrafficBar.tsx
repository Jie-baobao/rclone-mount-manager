import { fmtBytes } from "./api";
import type { T } from "./i18n";

/**
 * 上传/下载流量条：首页给全部连接的合计，详情页「节点概况」给单个连接的合计。
 *
 * 口径提醒（也写进了 hover 提示）：rclone 的 core/stats 只报「收发合计」，
 * 上下行是按「当时是否有活跃上传」推断的，不是协议层精确值；
 * 数字本身来自落盘账本，所以重启应用不会清零。
 */
export default function TrafficBar({
  todayUp,
  todayDown,
  totalUp,
  totalDown,
  t,
}: {
  todayUp: number;
  todayDown: number;
  totalUp: number;
  totalDown: number;
  t: T;
}) {
  return (
    <div className="traffic-bar" title={t("traffic.tip")}>
      <div className="traffic-group">
        <label>{t("traffic.today")}</label>
        <span className="up">
          <i>↑</i>
          {t("traffic.up")} {fmtBytes(todayUp)}
        </span>
        <span className="down">
          <i>↓</i>
          {t("traffic.down")} {fmtBytes(todayDown)}
        </span>
      </div>
      <div className="traffic-group">
        <label>{t("traffic.total")}</label>
        <span className="up">
          <i>↑</i>
          {fmtBytes(totalUp)}
        </span>
        <span className="down">
          <i>↓</i>
          {fmtBytes(totalDown)}
        </span>
      </div>
    </div>
  );
}
