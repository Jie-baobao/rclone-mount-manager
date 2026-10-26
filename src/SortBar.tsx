import type { T } from "./i18n";

export type SortBy = "default" | "name" | "status" | "group";
export type SortDir = "asc" | "desc";

/**
 * 排序条：名称 / 状态 / 分组 三选一。
 * 点第一下正序（↑）、第二下倒序（↓）、第三下回到默认（添加顺序，箭头消失）。
 * 只有被选中的那一项会变色并显示箭头。
 */
export default function SortBar({
  sortBy,
  sortDir,
  onChange,
  t,
}: {
  sortBy: SortBy;
  sortDir: SortDir;
  onChange: (by: SortBy, dir: SortDir) => void;
  t: T;
}) {
  const items: Array<{ key: SortBy; label: string }> = [
    { key: "name", label: t("sort.name") },
    { key: "status", label: t("sort.status") },
    { key: "group", label: t("sort.group") },
  ];

  function cycle(key: SortBy) {
    if (sortBy !== key) {
      onChange(key, "asc");
    } else if (sortDir === "asc") {
      onChange(key, "desc");
    } else {
      onChange("default", "asc");
    }
  }

  return (
    <div className="sortbar" role="group" aria-label={t("sort.name")}>
      {items.map((it) => {
        const active = sortBy === it.key;
        return (
          <button
            key={it.key}
            className={`sort-chip ${active ? (sortDir === "asc" ? "active asc" : "active desc") : ""}`}
            onClick={() => cycle(it.key)}
            title={
              active
                ? sortDir === "asc"
                  ? t("sort.toDesc")
                  : t("sort.toDefault")
                : t("sort.toAsc")
            }
          >
            {it.label}
            {active && <span className="arrow">{sortDir === "asc" ? "↑" : "↓"}</span>}
          </button>
        );
      })}
    </div>
  );
}
