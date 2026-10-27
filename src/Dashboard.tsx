import { fmtLatency, healthOf, healthTip, PROTOCOL_META, type Connection, type MountState } from "./api";
import type { T } from "./i18n";

/**
 * 首页（没有选中任何连接时显示）。
 * 打开软件默认停在这里：给出连接总数 / 已开启 / 状态差 / 已关闭四个数字，
 * 再把「状态差」的连接单独列出来，点一下就能跳进详情。
 * 一条连接都没有时退回三步上手引导。
 */
export default function Dashboard({
  conns,
  states,
  t,
  onSelect,
}: {
  conns: Connection[];
  states: Record<string, MountState>;
  t: T;
  onSelect: (id: string) => void;
}) {
  const mounted = conns.filter((c) => states[c.id]?.mounted);
  const problem = mounted.filter((c) => {
    const cls = healthOf(states[c.id], t).cls;
    return cls.includes("bad") || cls.includes("warn");
  });
  const stopped = conns.length - mounted.length;

  const cards: Array<{ key: string; label: string; value: number; cls: string }> = [
    { key: "total", label: t("home.total"), value: conns.length, cls: "" },
    { key: "mounted", label: t("home.mounted"), value: mounted.length, cls: mounted.length ? "ok" : "" },
    { key: "problem", label: t("home.problem"), value: problem.length, cls: problem.length ? "warn" : "" },
    { key: "stopped", label: t("home.stopped"), value: stopped, cls: "" },
  ];

  return (
    <div className="home">
      <header className="home-head">
        <h2>{t("home.title")}</h2>
        <p className="muted">{conns.length === 0 ? t("app.heroSub") : t("home.sub")}</p>
      </header>

      {conns.length === 0 ? (
        <section className="panel">
          <h3 className="home-steps-title">{t("app.heroTitle")}</h3>
          <ol className="home-steps">
            <li>{t("app.heroStep1")}</li>
            <li>{t("app.heroStep2")}</li>
            <li>{t("app.heroStep3")}</li>
          </ol>
          <p className="tips">{t("app.heroTip")}</p>
        </section>
      ) : (
        <>
          <div className="home-cards">
            {cards.map((c) => (
              <div key={c.key} className={`home-card ${c.cls}`}>
                <label>{c.label}</label>
                <strong>{c.value}</strong>
              </div>
            ))}
          </div>

          <section className="panel">
            <div className="panel-head">
              <h3>{t("home.attention")}</h3>
              <div className="panel-tools">
                <span className="muted small">{problem.length ? `${problem.length} / ${mounted.length}` : ""}</span>
              </div>
            </div>
            {problem.length === 0 ? (
              <p className="muted small home-empty">{t("home.allGood")}</p>
            ) : (
              <ul className="home-list">
                {problem.map((c) => {
                  const st = states[c.id];
                  const h = healthOf(st, t);
                  return (
                    <li key={c.id} onClick={() => onSelect(c.id)} title={healthTip(st, t)}>
                      <span className={h.cls} />
                      <span className="home-list-name">{c.name}</span>
                      <span className="tag">{PROTOCOL_META[c.protocol].short}</span>
                      <span className="tag mono">{c.target}</span>
                      <span className="home-list-state">
                        {h.label}
                        {st?.latency_us != null ? ` · ${fmtLatency(st.latency_us)}` : ""}
                      </span>
                    </li>
                  );
                })}
              </ul>
            )}
          </section>
        </>
      )}

      <p className="muted small home-tip">{t("home.tip")}</p>
    </div>
  );
}
