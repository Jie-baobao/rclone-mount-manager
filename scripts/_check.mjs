import { chromium } from "playwright";
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1180, height: 800 } });
const errs = [];
page.on("console", (m) => { if (m.type() === "error") errs.push(m.text().slice(0, 160)); });
page.on("pageerror", (e) => errs.push("pageerror: " + String(e).slice(0, 160)));
await page.goto("http://localhost:5184/", { waitUntil: "networkidle" });
await page.waitForTimeout(500);

const codes = ["zh-CN", "zh-TW", "en", "fr", "de", "ja", "ko"];
const bad = [];
for (const code of codes) {
  await page.locator(".sidebar-foot button.btn.ghost").click();
  await page.waitForTimeout(300);
  if (code === "zh-CN") {
    console.log("下拉项：", (await page.locator(".modal.settings select.ctl").first().locator("option").allInnerTexts()).join(" | "));
  }
  await page.selectOption(".modal.settings select.ctl >> nth=0", code);
  await page.locator(".modal .modal-foot .btn.primary").click();
  await page.waitForTimeout(350);
  const modalText = await page.locator(".modal").innerText();
  await page.locator(".modal .icon-btn").click();
  await page.waitForTimeout(250);
  const homeText = await page.locator(".main").innerText();
  await page.locator(".conn").first().click();
  await page.waitForTimeout(300);
  const detail = await page.locator(".main").innerText();
  await page.locator(".conn").first().click();
  await page.waitForTimeout(200);
  const leak = (modalText + "\n" + homeText + "\n" + detail).match(/\b(?:app|cf|st|stats|home|toast|h|u|pm|ps)\.[a-zA-Z.]+/g);
  if (leak) bad.push(code + ": " + [...new Set(leak)].join(", "));
  console.log(code, "ok");
}
console.log("缺 key:", bad.length ? bad.join(" | ") : "无");
console.log("控制台错误:", errs.length ? errs.join(" | ") : "无");
await browser.close();
