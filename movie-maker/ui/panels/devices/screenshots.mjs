import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const outDir = fileURLToPath(new URL("./screenshots/", import.meta.url));

/** モックの API で、符号と QR を出した状態と、足りない設定の文言（serve が未設定、ポートが使われている）を撮る。右の列と同じ幅。 */
export default async (browser, { dist }) => {
  await mkdir(outDir, { recursive: true });
  for (const theme of ["light", "dark"]) {
    const page = await browser.newPage({ viewport: { width: 322, height: 760 }, deviceScaleFactor: 2 });
    await page.goto(`${dist("devices-demo.html")}?theme=${theme}`);
    await page.getByRole("button", { name: "Add device" }).click();
    await page.waitForSelector(".mm-devices-qr");
    await page.mouse.move(0, 0);
    await page.screenshot({ path: `${outDir}devices-${theme}-pairing.png` });
    for (const state of ["serve", "port", "tailscale"]) {
      await page.goto(`${dist("devices-demo.html")}?theme=${theme}&state=${state}`);
      await page.waitForSelector(".mm-devices-notice");
      await page.screenshot({ path: `${outDir}devices-${theme}-${state}.png`, clip: { x: 0, y: 0, width: 322, height: 420 } });
    }
    await page.close();
  }
};
