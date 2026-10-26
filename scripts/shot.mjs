// 开发期界面截图：npm run dev 起服务后，node scripts/shot.mjs
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const url = process.env.PREVIEW_URL ?? "http://127.0.0.1:5183/";
mkdirSync("docs/screenshots", { recursive: true });

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1180, height: 760 }, deviceScaleFactor: 1 });
await page.goto(url, { waitUntil: "networkidle" });
await page.waitForTimeout(600);
await page.screenshot({ path: "docs/screenshots/main.png" });

// 详情页（含「节点概况」统计面板），需要更高一点的视口
await page.setViewportSize({ width: 1180, height: 900 });
await page.waitForTimeout(300);
await page.screenshot({ path: "docs/screenshots/detail-stats.png" });
await page.setViewportSize({ width: 1180, height: 760 });

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

// 打开新建连接弹窗
await page.getByRole("button", { name: /新建连接/ }).click();
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

// 挂载失败状态（点第二个连接，模拟数据里它带上次的错误）
await page.getByText("办公室共享盘").click();
await page.waitForTimeout(400);
await page.screenshot({ path: "docs/screenshots/error-state.png" });

// 深色模式
const dark = await browser.newPage({ viewport: { width: 1180, height: 760 }, colorScheme: "dark" });
await dark.goto(url, { waitUntil: "networkidle" });
await dark.waitForTimeout(500);
await dark.screenshot({ path: "docs/screenshots/main-dark.png" });

await browser.close();
console.log("截图完成：main / settings / settings-deps / new-connection / modal-adv / s3-form / error-state / main-dark");
