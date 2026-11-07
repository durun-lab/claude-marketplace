// dist/ を /m/ で配り、iPhone（WebKit）と Android（Chromium）の端末の設定でスクリーンショットを撮る。
// 偽の母艦のデモ（demo.html）で画面を撮り、index.html では Service Worker とマニフェストが働くことを確かめる。
// playwright は repo の依存にせず、PLAYWRIGHT_DIR（playwright を入れた場所）から読む。
// 使い方: pnpm build && PLAYWRIGHT_DIR=/d/dev/pw node screenshots.mjs
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
  { name: "iphone", type: webkit, device: "iPhone 14" },
  { name: "android", type: chromium, device: "Pixel 7" },
];
const SHOTS = [
  ["pair", "state=unpaired&code=ABCD2345"],
  ["comments", "state=online&tab=Comments"],
  ["offline-queue", "state=queued&tab=Comments"],
  ["watch", "state=online&tab=Watch"],
  ["proposals", "state=online&tab=Proposals"],
];

const shoot = async (base, target) => {
  const browser = await target.type.launch();
  const context = await browser.newContext({ ...devices[target.device], colorScheme: "light" });
  for (const scheme of ["light", "dark"]) {
    await context.pages()[0]?.close();
    const page = await context.newPage();
    await page.emulateMedia({ colorScheme: scheme });
    for (const [name, query] of SHOTS) {
      await page.goto(`${base}/m/demo.html?${query}`);
      await page.waitForSelector(".tabs, .pair");
      await page.waitForTimeout(200);
      await page.screenshot({ path: `${OUT}${target.name}-${scheme}-${name}.png` });
    }
  }
  await browser.close();
};

/** 画面の資源だけがキャッシュされ、API は触られないことを実ブラウザーで確かめる。 */
const checkServiceWorker = async (base) => {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  await page.goto(`${base}/m/`);
  const result = await page.evaluate(async () => {
    await globalThis.navigator.serviceWorker.ready;
    await globalThis.fetch("/m/api/project").catch(() => null);
    const cached = [];
    for (const name of await globalThis.caches.keys()) for (const r of await (await globalThis.caches.open(name)).keys()) cached.push(new URL(r.url).pathname);
    const manifest = await (await globalThis.fetch(globalThis.document.querySelector("link[rel=manifest]").href)).json();
    return { cached, display: manifest.display };
  });
  await browser.close();
  const apiCached = result.cached.some((p) => p.startsWith("/m/api/"));
  process.stdout.write(`service worker: ${result.cached.length} cached, api cached: ${apiCached}, display: ${result.display}\n`);
  if (apiCached || !result.cached.includes("/m/app.js")) throw new Error("service worker cache is wrong");
};

await mkdir(OUT, { recursive: true });
const server = await serve();
const base = `http://localhost:${server.address().port}`;
try {
  await checkServiceWorker(base);
  for (const target of TARGETS) await shoot(base, target);
} finally {
  server.close();
}
