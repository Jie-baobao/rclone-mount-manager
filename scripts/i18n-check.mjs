#!/usr/bin/env node
/**
 * 语言文件自检：node scripts/i18n-check.mjs
 *
 * 检查 src/locales/languages.json 里声明的每个语言文件：
 *   - 文件是否存在、能否解析
 *   - 是否缺键（以 fallback 语言为基准）、是否多出基准语言没有的键
 *   - 值是否为空
 *   - 占位符（{name} 这种）是否与基准语言一致
 *
 * 有问题时退出码为 1，可以直接挂在 CI 上。
 */
import { readFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const localesDir = join(root, "src", "locales");
const manifestPath = join(localesDir, "languages.json");

const readJson = (p) => JSON.parse(readFileSync(p, "utf8"));
const placeholders = (s) => [...String(s).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

const manifest = readJson(manifestPath);
const entries = manifest.languages ?? [];
if (!entries.length) {
  console.error("languages.json 里没有任何语言");
  process.exit(1);
}

const baseCode = entries.some((e) => e.code === manifest.fallback)
  ? manifest.fallback
  : entries[0].code;
const basePath = join(localesDir, (entries.find((e) => e.code === baseCode)?.file ?? "").replace(/^\.\//, ""));
const base = readJson(basePath);
const baseKeys = Object.keys(base);

let problems = 0;
const report = (msg) => {
  problems += 1;
  console.log(`  ✗ ${msg}`);
};

console.log(`基准语言：${baseCode}（${baseKeys.length} 个键）`);

for (const entry of entries) {
  const { code, native, file } = entry;
  const path = join(localesDir, String(file).replace(/^\.\//, ""));
  console.log(`\n${code}  ${native}  ${file}`);
  if (!existsSync(path)) {
    report("语言文件不存在");
    continue;
  }

  let dict;
  try {
    dict = readJson(path);
  } catch (e) {
    report(`JSON 解析失败：${e.message}`);
    continue;
  }

  const keys = Object.keys(dict);
  const missing = baseKeys.filter((k) => !(k in dict));
  const extra = keys.filter((k) => !(k in base));
  const empty = keys.filter((k) => !String(dict[k] ?? "").trim());
  const badVars = keys.filter((k) => {
    if (!(k in base)) return false;
    const a = placeholders(base[k]).join(",");
    const b = placeholders(dict[k]).join(",");
    return a !== b;
  });

  console.log(`  键数 ${keys.length}`);
  if (missing.length) report(`缺 ${missing.length} 个键：${missing.slice(0, 8).join(", ")}${missing.length > 8 ? " …" : ""}`);
  if (extra.length) report(`多 ${extra.length} 个键：${extra.slice(0, 8).join(", ")}${extra.length > 8 ? " …" : ""}`);
  if (empty.length) report(`空值 ${empty.length} 个：${empty.slice(0, 8).join(", ")}${empty.length > 8 ? " …" : ""}`);
  if (badVars.length) report(`占位符不一致：${badVars.slice(0, 8).join(", ")}`);
  if (!missing.length && !extra.length && !empty.length && !badVars.length) console.log("  ✓ 通过");
}

console.log("");
if (problems) {
  console.error(`共 ${problems} 处问题，请修完再提交。`);
  process.exit(1);
}
console.log("全部语言文件检查通过。");
