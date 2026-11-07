// Story の画面を偽の母艦のデモ（demo.html）で操作して、390x844 のスクリーンショットを撮る。
// playwright は repo の依存にせず、PLAYWRIGHT_DIR（playwright を入れた場所）から読む。
// 使い方: pnpm build && PLAYWRIGHT_DIR=/d/dev/pw node screenshots-story.mjs
import { createReadStream } from "node:fs";
import { mkdir, stat } from "node:fs/promises";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { extname } from "node:path";
import process from "node:process";
import { URL, fileURLToPath } from "node:url";

const require = createRequire(`${process.env.PLAYWRIGHT_DIR ?? "."}/`);
const { chromium, webkit, devices } = require("playwright");

const DIST = fileURLToPath(new URL("./dist/", import.meta.url));
const OUT = fileURLToPath(new URL("./screenshots/", import.meta.url));
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".png": "image/png", ".webmanifest": "application/manifest+json" };

const serve = () =>
  new Promise((resolve) => {
    const server = createServer(async (req, res) => {
      const path = new URL(req.url, "http://x").pathname;
      const file = path === "/m/" ? "index.html" : path.replace(/^\/m\//, "");
      const ok = path.startsWith("/m/") && !file.includes("..") && (await stat(DIST + file).catch(() => null));
      res.writeHead(ok ? 200 : 404, { "Content-Type": TYPES[extname(file)] ?? "text/plain" });
      if (ok) createReadStream(DIST + file).pipe(res);
      else res.end();
    });
    server.listen(0, () => resolve(server));
  });

const TARGETS = [
  { name: "iphone", type: webkit, device: "iPhone 14", schemes: ["light", "dark"] },
  { name: "android", type: chromium, device: "Pixel 7", schemes: ["light"] },
];

const word = (page, text) => page.locator(".st-word", { hasText: new RegExp(`^${text}$`) }).first();

/** 画面の遷移の順に撮る。各段階の名前がファイル名になる。 */
const STEPS = [
  ["assets", async () => undefined],
  ["reader", (page) => page.locator(".st-assets button:not(:disabled)").first().click()],
  ["range-start", (page) => word(page, "Then").click()],
  ["range", (page) => word(page, "gold.").click()],
  ["hold-done", (page) => page.getByRole("button", { name: "Hold", exact: true }).click()],
  ["pc-mode", async (page) => {
    await word(page, "today").click();
    await word(page, "talk").click();
    await page.locator(".st-pc input").check();
  }],
  ["pc-sent", (page) => page.getByRole("button", { name: "Keep", exact: true }).click()],
  ["arrange", async (page) => {
    await page.locator(".st-pc input").uncheck();
    await page.getByRole("button", { name: "Arrange", exact: true }).click();
  }],
];

const shoot = async (base, target) => {
  const browser = await target.type.launch();
  for (const scheme of target.schemes) {
    const context = await browser.newContext({ ...devices[target.device], colorScheme: scheme });
    const page = await context.newPage();
    await page.goto(`${base}/m/demo.html?state=online&tab=Story`);
    await page.waitForSelector(".st-assets");
    for (const [name, step] of STEPS) {
      await step(page);
      await page.waitForTimeout(250);
      await page.screenshot({ path: `${OUT}story-${target.name}-${scheme}-${name}.png` });
    }
    await page.getByRole("button", { name: "Close", exact: true }).click();
    await page.getByRole("button", { name: "Clips", exact: true }).click();
    await page.locator('input[type="search"]').fill("ferry");
    await page.locator('input[type="search"]').press("Enter");
    await page.waitForSelector(".st-hits button");
    await page.screenshot({ path: `${OUT}story-${target.name}-${scheme}-search.png` });
    await context.close();
  }
  await browser.close();
};

await mkdir(OUT, { recursive: true });
const server = await serve();
const base = `http://localhost:${server.address().port}`;
for (const target of TARGETS) await shoot(base, target);
server.close();
