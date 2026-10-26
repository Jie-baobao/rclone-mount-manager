// 开发期界面截图：npm run dev 起服务后，node scripts/shot.mjs
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const url = process.env.PREVIEW_URL ?? "http://127.0.0.1:5183/";
mkdirSync("docs/screenshots", { recursive: true });

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1180, height: 760 }, deviceScaleFactor: 1 });
await page.goto(url, { waitUntil: "networkidle" });
await page.waitForTimeout(600);

// 默认状态：没有选中任何连接 → 首页概览
await page.screenshot({ path: "docs/screenshots/main.png" });

// 排序条：点「分组」→ 正序（↑ 高亮），再点 → 倒序（↓）
await page.locator(".sort-chip", { hasText: /分组|Group|Groupe|Gruppe|グループ|그룹/ }).click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/sort-group.png" });
await page.locator(".sort-chip.active").click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/sort-group-desc.png" });
// 第三下恢复默认
await page.locator(".sort-chip.active").click();
await page.waitForTimeout(300);
await page.locator(".sort-chip", { hasText: /状态|Status|État|상태/ }).click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/sort-status.png" });
await page.locator(".sort-chip.active").click();
await page.waitForTimeout(200);
await page.locator(".sort-chip.active").click();
await page.waitForTimeout(300);

// 点第一个连接进详情（含「节点概况」图表），视口高一点好完整截下来
await page.locator(".conn").first().click();
await page.setViewportSize({ width: 1180, height: 900 });
await page.waitForTimeout(500);
await page.screenshot({ path: "docs/screenshots/detail-stats.png" });
await page.setViewportSize({ width: 1180, height: 760 });

// 再点一次同一条 → 取消选中，回到首页
await page.locator(".conn").first().click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/home.png" });

// 设置页上半部分（通用 / 启动 / 探测规则 / 日志）
await page.locator(".sidebar-foot button.btn.ghost").click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/settings.png" });

// 设置页下半部分（依赖与更新 / 数据）
await page.locator(".modal.settings .modal-body").evaluate((el) => {
  el.scrollTop = el.scrollHeight;
});
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/settings-deps.png" });
await page.locator(".modal .icon-btn").click();
await page.waitForTimeout(200);

// 每种语言：开设置 → 选语言 → 保存 → 关掉
const pickLang = async (code) => {
  await page.locator(".sidebar-foot button.btn.ghost").click();
  await page.waitForTimeout(350);
  await page.selectOption(".modal.settings select.ctl >> nth=0", code);
  await page.getByRole("button", { name: /保存|Save|Enregistrer|Speichern/ }).last().click();
  await page.waitForTimeout(500);
  await page.locator(".modal .icon-btn").click();
  await page.waitForTimeout(400);
};

// 法语界面
await pickLang("fr");
await page.screenshot({ path: "docs/screenshots/fr-home.png" });
await page.locator(".conn").first().click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/fr-detail.png" });
await page.locator(".conn").first().click();
await page.waitForTimeout(300);

// 德语界面
await pickLang("de");
await page.screenshot({ path: "docs/screenshots/de-home.png" });
await page.locator(".sidebar-foot button.btn.ghost").click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/de-settings.png" });
await page.locator(".modal .icon-btn").click();

// 打开新建连接弹窗
await page.getByRole("button", { name: /New Connection|新建连接|Nouvelle|Neue/ }).click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/new-connection.png" });

// 展开高级选项
const adv = page.locator("details.adv summary");
if (await adv.count()) {
  await adv.click();
  await page.waitForTimeout(300);
  await page.screenshot({ path: "docs/screenshots/modal-adv.png" });
}

// S3 表单
await page.selectOption(".modal select.ctl >> nth=0", "s3").catch(() => {});
await page.waitForTimeout(200);
await page.selectOption(".modal select.ctl >> nth=1", "aliyun").catch(() => {});
await page.waitForTimeout(300);
await page.screenshot({ path: "docs/screenshots/s3-form.png" });

await page.locator(".modal .icon-btn").click().catch(() => {});
await page.waitForTimeout(200);

// 挂载失败状态（这条连接模拟数据里带上次的错误）
await page.locator(".conn").nth(1).click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/error-state.png" });

// 深色模式（首页）
const dark = await browser.newPage({ viewport: { width: 1180, height: 760 }, colorScheme: "dark" });
await dark.goto(url, { waitUntil: "networkidle" });
await dark.waitForTimeout(500);
await dark.screenshot({ path: "docs/screenshots/main-dark.png" });
await dark.locator(".conn").first().click();
await dark.waitForTimeout(400);
await dark.screenshot({ path: "docs/screenshots/detail-dark.png" });

await browser.close();
console.log(
  "截图完成：main / detail-stats / home / settings / settings-deps / fr-home / fr-detail / de-home / de-settings / new-connection / modal-adv / s3-form / error-state / main-dark / detail-dark"
);
