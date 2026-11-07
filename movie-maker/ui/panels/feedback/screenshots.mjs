import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const outDir = fileURLToPath(new URL("./screenshots/", import.meta.url));

/**
 * モックの API で、回の中・PR で止まった・差し戻し・取り込み済みの 4 件を撮る。右の列と同じ幅。
 * 2 枚目は「Point at UI」で部品を指している間（枠）と、指した後の添付を撮る。
 */
export default async (browser, { dist }) => {
  await mkdir(outDir, { recursive: true });
  for (const theme of ["light", "dark"]) {
    const page = await browser.newPage({ viewport: { width: 322, height: 1000 }, deviceScaleFactor: 2 });
    await page.goto(`${dist("feedback-demo.html")}?theme=${theme}`);
    await page.waitForSelector(".mm-feedback-card");
    await page.mouse.move(0, 0);
    await page.screenshot({ path: `${outDir}feedback-${theme}.png` });
    await page.getByRole("textbox", { name: "Feedback" }).fill("Make the filter buttons say what they count.");
    await page.getByRole("button", { name: "Point at UI" }).click();
    await page.getByRole("button", { name: "Pending" }).hover();
    await page.screenshot({ path: `${outDir}feedback-${theme}-pointing.png`, clip: { x: 0, y: 0, width: 322, height: 420 } });
    await page.getByRole("button", { name: "Pending" }).click();
    await page.waitForSelector(".mm-feedback-chip");
    await page.mouse.move(0, 0);
    await page.screenshot({ path: `${outDir}feedback-${theme}-attached.png`, clip: { x: 0, y: 0, width: 322, height: 420 } });
    await page.close();
  }
};
