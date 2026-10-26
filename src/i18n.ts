/**
 * 多语言支持：默认跟随系统语言，也可以在设置页手动指定。
 *
 * 文案不再是写死在这个文件里的对象，而是每个语言一份独立的 JSON：
 *
 *   src/locales/languages.json   ← 内置语言清单（代码 / 名称 / 文件路径）
 *   src/locales/zh-CN.json
 *   src/locales/en.json
 *   ...
 *
 * 新增一种语言只需要两步，不用改任何代码：
 *   ① 复制一份 JSON，改名成目标语言代码（如 pt-BR.json），把值翻译掉；
 *   ② 在 languages.json 的 languages 数组里加一行。
 *
 * 这些 JSON 在构建时由 Vite 的 import.meta.glob 全部打进前端资源包，
 * 运行时不读磁盘，所以发布出去的安装包自带全部语言，也不会因为文件丢失而白屏。
 *
 * 覆盖范围：所有界面文案（主界面、连接表单、设置、日志提示、状态、单位、时间）。
 * 唯一例外是后端返回的原文（WebDAV 诊断说明、rclone 自己的报错），那部分保持
 * 服务端/工具原话，便于对照官方文档排查，日志本身也不会被翻译。
 */
import manifest from "./locales/languages.json";

type Dict = Record<string, string>;

/** 构建时把 locales 目录下的 JSON 全部收进来，key 形如 "./locales/en.json" */
const modules = import.meta.glob("./locales/*.json", { eager: true }) as Record<
  string,
  { default?: Dict } | Dict
>;

/** 清单文件本身不是词典，跳过 */
const MANIFEST_FILE = "languages.json";

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

const dicts = new Map<string, Dict>();
for (const [path, mod] of Object.entries(modules)) {
  const name = fileName(path);
  if (name === MANIFEST_FILE) continue;
  const dict = (mod as { default?: Dict }).default ?? (mod as Dict);
  if (dict && typeof dict === "object") dicts.set(name, dict);
}

export type LanguageMeta = { code: string; native: string; file: string };

/**
 * 清单里声明、且文件确实存在的语言。顺序即设置页下拉框的顺序。
 * 声明了但文件没放进来时只丢掉这一项并在控制台告警，不影响其它语言。
 */
export const LANGUAGES: LanguageMeta[] = manifest.languages.filter((l) => {
  const ok = dicts.has(fileName(l.file));
  if (!ok) console.warn(`[i18n] 语言文件缺失，已跳过：${l.code} → ${l.file}`);
  return ok;
});

export const LANGS: string[] = LANGUAGES.map((l) => l.code);

/** 兜底语言：清单里的 fallback，没写或不存在就用第一项 */
export const FALLBACK_LANG: string = LANGS.includes(manifest.fallback)
  ? manifest.fallback
  : LANGS[0] ?? "zh-CN";

/** 设置页语言下拉框：code=system 表示跟随系统（文案走 t("st.langSystem")） */
export const LANGUAGE_OPTIONS: Array<{ code: string; native: string }> = [
  { code: "system", native: "" },
  ...LANGUAGES.map(({ code, native }) => ({ code, native })),
];

export type Lang = string;

function dictOf(code: string): Dict {
  const meta = LANGUAGES.find((l) => l.code === code);
  return (meta && dicts.get(fileName(meta.file))) || {};
}

const FALLBACK_DICT = dictOf(FALLBACK_LANG);

/**
 * 把任意语言标签（"zh-Hant-TW" / "en_US" / "fr-CA"）对到清单里的语言代码。
 * 先精确匹配，再按主语言匹配；中文额外区分简繁。
 */
function matchCode(tag: string): Lang | null {
  const t = tag.trim().toLowerCase().replace(/_/g, "-");
  if (!t) return null;
  const exact = LANGS.find((c) => c.toLowerCase() === t);
  if (exact) return exact;
  if (t === "zh" || t.startsWith("zh-")) {
    const want = /hant|tw|hk|mo/.test(t) ? "zh-tw" : "zh-cn";
    return LANGS.find((c) => c.toLowerCase() === want) ?? null;
  }
  const base = t.split("-")[0];
  return LANGS.find((c) => c.toLowerCase().split("-")[0] === base) ?? null;
}

/** 把设置值解析成实际语言（system → 看浏览器语言，认不出来就用兜底语言） */
export function resolveLanguage(setting: string): Lang {
  if (setting === "system") {
    return matchCode(navigator.language || "") ?? FALLBACK_LANG;
  }
  return matchCode(setting) ?? FALLBACK_LANG;
}

export type T = (key: string, vars?: Record<string, string | number>) => string;

/** 生成翻译函数：缺 key 时回退到兜底语言（简体中文），再兜底显示 key 本身 */
export function makeT(lang: Lang): T {
  const d = dictOf(lang);
  return (key, vars) => {
    let s = d[key] ?? FALLBACK_DICT[key] ?? key;
    if (vars) {
      for (const [k, v] of Object.entries(vars)) {
        s = s.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
      }
    }
    return s;
  };
}
