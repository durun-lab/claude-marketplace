// dist/ を /m/ で配り、iPhone（WebKit）と Android（Chromium）の端末の設定でスクリーンショットを撮る。
// 偽の PC のデモ（demo.html）で画面を撮り、index.html では Service Worker とマニフェストが働くことを確かめる。
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

// PC の映像の配信の偽物。HLS と提案の短い動画は demo/media の実物、フレームは前後で色の違う SVG。
const MEDIA = fileURLToPath(new URL("./demo/media/", import.meta.url));
const MEDIA_TYPES = { ".m3u8": "application/vnd.apple.mpegurl", ".m4s": "video/iso.segment", ".mp4": "video/mp4" };
const frameSvg = (query) => {
  const after = query.has("proposal");
  const [bg, fg] = after ? ["#1f6f4a", "#9be7c0"] : ["#3b4252", "#b7c0d0"];
  const shift = after ? 90 : 0;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 720 405"><rect width="720" height="405" fill="${bg}"/>
<circle cx="${240 + shift}" cy="200" r="90" fill="${fg}"/><text x="360" y="360" font-size="34" font-family="sans-serif" fill="#fff" text-anchor="middle">${after ? "After" : "Before"} t=${query.get("t")}s</text></svg>`;
};
const serveMedia = async (url, res) => {
  const name = url.pathname.split("/").pop();
  if (url.pathname === "/m/api/frame") {
    res.writeHead(200, { "Content-Type": "image/svg+xml" });
    return res.end(frameSvg(url.searchParams));
  }
  const file = url.pathname.endsWith("/clip.mp4") ? "clip.mp4" : name.replace(/^360p_init/, "360p_init");
  const ok = /^[\w.]+$/.test(file) && (await stat(MEDIA + file).catch(() => null));
  res.writeHead(ok ? 200 : 404, { "Content-Type": MEDIA_TYPES[extname(file)] ?? "text/plain" });
  if (ok) createReadStream(MEDIA + file).pipe(res);
  else res.end();
};

const serve = () =>
  new Promise((resolve) => {
    const server = createServer(async (req, res) => {
      const url = new URL(req.url, "http://x");
      const path = url.pathname;
      if (/^\/m\/api\/(stream\/|frame|proposals\/\d+\/clip\.mp4)/.test(path)) return serveMedia(url, res);
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
// 3 つ目は撮る前の操作。描き込みは映像の上を指でなぞる。
const draw = async (page) => {
  const box = await page.locator(".draw").boundingBox();
  const at = (x, y) => [box.x + box.width * x, box.y + box.height * y];
  await page.mouse.move(...at(0.2, 0.25));
  await page.mouse.down();
  for (const [x, y] of [[0.3, 0.2], [0.45, 0.3], [0.55, 0.5], [0.7, 0.45]]) await page.mouse.move(...at(x, y), { steps: 4 });
  await page.mouse.up();
};
const SHOTS = [
  ["pair", "state=unpaired&code=ABCD2345"],
  ["watch", "state=online&tab=Watch"],
  ["watch-compose", "state=online&view=compose"],
  ["watch-draw", "state=online&view=compose", async (page) => {
    await page.getByRole("button", { name: "Draw" }).click();
    await draw(page);
    await page.locator("textarea").fill("Move this title up a little.");
  }],
  ["watch-thread", "state=online&view=thread"],
  ["offline-queue", "state=queued&tab=Watch"],
  ["proposals", "state=online&tab=Proposals"],
  ["proposal-detail", "state=online&tab=Proposals&view=detail"],
];

const shoot = async (base, target) => {
  const browser = await target.type.launch();
  const context = await browser.newContext({ ...devices[target.device], colorScheme: "light" });
  for (const scheme of ["light", "dark"]) {
    await context.pages()[0]?.close();
    const page = await context.newPage();
    await page.emulateMedia({ colorScheme: scheme });
    for (const [name, query, before] of SHOTS) {
      await page.goto(`${base}/m/demo.html?${query}`);
      await page.waitForSelector(".tabs, .pair");
      await page.waitForTimeout(900);
      await before?.(page);
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

/** 親のプレイリストから画質ごとのプレイリスト、区間と読み進み、映像が実際に進むことを H.264 の入った Chrome で確かめる。 */
const checkPlayback = async (base) => {
  const browser = await chromium.launch({ channel: process.env.PLAYBACK_CHANNEL ?? "chrome" });
  const page = await (await browser.newContext({ ...devices["Pixel 7"] })).newPage();
  const requested = [];
  page.on("request", (r) => requested.push(new URL(r.url()).pathname.split("/").pop()));
  await page.goto(`${base}/m/demo.html?state=online&tab=Watch`);
  await page.getByRole("button", { name: "Play" }).click();
  await page.waitForFunction(() => globalThis.document.querySelector("video").currentTime > 1, null, { timeout: 15000 });
  await browser.close();
  process.stdout.write(`playback: ${requested.filter((n) => /m3u8|m4s|mp4/.test(n)).join(" ")}\n`);
  const order = ["index.m3u8", "360p.m3u8", "360p_init.mp4"].map((n) => requested.indexOf(n));
  if (order.includes(-1) || order.some((v, i) => i > 0 && v < order[i - 1])) throw new Error("master playlist was not read first");
};

await mkdir(OUT, { recursive: true });
const server = await serve();
const base = `http://localhost:${server.address().port}`;
try {
  await checkServiceWorker(base);
  await checkPlayback(base);
  for (const target of TARGETS) await shoot(base, target);
} finally {
  server.close();
}
