// モバイル版の通しの確認。本物の mm-serve の人の待ち受けと Claude の待ち受けの MCP を相手に、
// iPhone（WebKit）と Android（Chromium）の端末の設定で flow.mjs の手順を通す。
// playwright は repo の依存にせず、PLAYWRIGHT_DIR（playwright を入れた場所）から読む。
// 使い方は docs/design/11-testing.md の「モバイル版の通しの確認」。
import { mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import process from "node:process";
import { URL, fileURLToPath } from "node:url";
import { STEPS } from "./flow.mjs";
import { makeCert, mcpClient, startPc, startRelay } from "./pc.mjs";

const require = createRequire(`${process.env.PLAYWRIGHT_DIR ?? "."}/`);
const { chromium, webkit, devices } = require("playwright");

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const REPO = here("../../../");
const TARGETS = {
  iphone: { type: webkit, device: "iPhone 14", deviceName: "iPhone" },
  android: { type: chromium, device: "Pixel 7", deviceName: "Android" },
};
const DEFAULT_REPEAT = 1;

const config = () => {
  const work = resolve(process.env.MM_E2E_WORK ?? `${REPO}target/mobile-e2e`);
  return {
    exe: process.env.MM_RIG ?? `${REPO}target/debug/examples/mobile_rig.exe`,
    project: `${REPO}fixtures/projects/mobile/project.json`,
    transcript: `${REPO}fixtures/ingest/transcript-ja.json`,
    assets: here("../dist"),
    workRoot: work,
    appData: `${work}/appdata`,
    targets: (process.env.MM_E2E_TARGETS ?? "iphone,android").split(","),
    repeat: Number(process.env.MM_E2E_REPEAT ?? DEFAULT_REPEAT),
  };
};

/** 1 回の通し。PC の側、中継、ブラウザーを新しく作り、終わったら全部止める。 */
const runOnce = async (cfg, name, tls) => {
  const target = TARGETS[name];
  const pc = await startPc(cfg);
  const relay = await startRelay(pc.mobile_port, tls);
  const browser = await target.type.launch();
  const results = [];
  try {
    // 中継の証明書は自己署名なので、証明書の誤りだけを見逃す
    const context = await browser.newContext({ ...devices[target.device], ignoreHTTPSErrors: true });
    const page = await context.newPage();
    const hlsRequests = [];
    page.on("request", (r) => /\/hls-[\w-]+\.js$/.test(r.url()) && hlsRequests.push(r.url()));
    const cx = { page, context, pc, relay, mcp: mcpClient(pc), deviceName: target.deviceName, browserName: target.type.name(), hlsRequests, state: {} };
    for (const [step, fn] of STEPS) {
      const started = Date.now();
      try {
        results.push({ step, ok: true, note: await fn(cx), ms: Date.now() - started });
      } catch (e) {
        await page.screenshot({ path: `${cfg.workRoot}/${name}-failed.png` }).catch(() => undefined);
        results.push({ step, ok: false, note: e.message.split("\n")[0], ms: Date.now() - started, logs: pc.logs.join("").slice(-2000) });
        break;
      }
    }
  } finally {
    await browser.close();
    relay.close();
    await pc.stop();
  }
  return results;
};

const print = (name, round, results) => {
  for (const r of results) process.stdout.write(`${name} #${round} ${r.ok ? "ok  " : "FAIL"} ${r.step} (${r.ms} ms): ${r.note}\n`);
  const failed = results.find((r) => !r.ok);
  if (failed?.logs) process.stdout.write(`--- mobile_rig の stderr ---\n${failed.logs}\n`);
  return !failed && results.length === STEPS.length;
};

const cfg = config();
await mkdir(cfg.appData, { recursive: true });
const tls = await makeCert(cfg.workRoot);
let allOk = true;
for (const name of cfg.targets) {
  for (let round = 1; round <= cfg.repeat; round++) allOk = print(name, round, await runOnce(cfg, name, tls)) && allOk;
}
process.stdout.write(allOk ? "mobile e2e: all passed\n" : "mobile e2e: FAILED\n");
process.exitCode = allOk ? 0 : 1;
