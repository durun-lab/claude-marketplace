import { mkdir, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const outDir = fileURLToPath(new URL("./screenshots/", import.meta.url));
const FIXTURE = new URL("../../../fixtures/ui/inspector.json", import.meta.url);

/** キーフレームの時間の欄。今の列の幅（欄は行の下）と、広げた列（欄は行の右）、選んだひし形と Show Only Keyframed Properties。 */
const shootKeyframeStrip = async (browser, dist) => {
  const shots = [
    { name: "narrow", w: undefined, select: false, only: false },
    { name: "wide", w: 420, select: true, only: false },
    { name: "wide-keyed-only", w: 420, select: true, only: true },
  ];
  for (const { name, w, select, only } of shots) {
    const page = await browser.newPage({ viewport: { width: 1000, height: 900 }, deviceScaleFactor: 2 });
    await page.goto(`${dist("keyframes-demo.html")}?theme=light&t=2${w ? `&w=${w}` : ""}`);
    await page.waitForSelector(".mm-insp-strip");
    if (select) await page.click('[aria-label="position keyframe at 2.00s"]');
    if (only) await page.check(".mm-insp-keyedonly input");
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
