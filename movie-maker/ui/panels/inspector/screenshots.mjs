import { mkdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const outDir = fileURLToPath(new URL("./screenshots/", import.meta.url));
const FIXTURE = new URL("../../../fixtures/ui/inspector.json", import.meta.url);

/**
 * キーフレームの欄。今の右の列の幅（キーフレームのある行の下に出す）と、右の列を 400 に広げたときの幅
 * （縦のスクロールバーを除いて 362、行の右に出す）。選んだひし形と Show Only Keyframed Properties、ダークの配色も撮る。
 */
const shootKeyframeStrip = async (browser, dist) => {
  const shots = [
    { name: "narrow", theme: "light", w: undefined, select: false, only: false },
    { name: "narrow-dark", theme: "dark", w: undefined, select: true, only: false },
    { name: "wide", theme: "light", w: 362, select: true, only: false },
    { name: "wide-keyed-only", theme: "light", w: 362, select: true, only: true },
  ];
  for (const { name, theme, w, select, only } of shots) {
    const page = await browser.newPage({ viewport: { width: 1000, height: 900 }, deviceScaleFactor: 2 });
    await page.goto(`${dist("keyframes-demo.html")}?theme=${theme}&t=2${w ? `&w=${w}` : ""}`);
    await page.waitForSelector(".mm-insp-strip");
    // 切り替えを押すとフォーカスが移って選択が外れるので、先に切り替えてからひし形を選ぶ。
    if (only) await page.check(".mm-insp-keyedonly input");
    if (select) await page.click('[aria-label="position keyframe at 2.00s"]');
    await page.screenshot({ path: `${outDir}keyframe-strip-${name}.png` });
    await page.close();
  }
};

/** デモのページ（title と footage）と、本体の右の列に載った Inspector を撮る。 */
export default async (browser, { dist }) => {
  await mkdir(outDir, { recursive: true });
  for (const theme of ["light", "dark"]) {
    const page = await browser.newPage({ viewport: { width: 760, height: 900 }, deviceScaleFactor: 2 });
    await page.goto(`${dist("inspector-demo.html")}?theme=${theme}`);
    await page.screenshot({ path: `${outDir}inspector-${theme}-title.png` });
    await page.selectOption("select[aria-label=layer]", "l-footage");
    await page.screenshot({ path: `${outDir}inspector-${theme}-footage.png` });
    await page.close();
  }
  await shootKeyframeStrip(browser, dist);
  const project = JSON.parse(await readFile(FIXTURE, "utf8"));
  const detail = { layer: project.compositions[0].layers[0], assets: project.assets };
  const page = await browser.newPage({ viewport: { width: 900, height: 900 }, deviceScaleFactor: 2 });
  await page.goto(`${dist("index.html")}?theme=light`);
  await page.waitForSelector(".mm-insp");
  await page.waitForTimeout(200);
  await page.evaluate((d) => window.dispatchEvent(new CustomEvent("mm:layer-selected", { detail: d })), detail);
  await page.waitForSelector(".mm-insp-header");
  await page.screenshot({ path: `${outDir}shell-light.png` });
  await page.close();
};
