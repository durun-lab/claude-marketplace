// 偽の PC で PWA を動かすページ。スクリーンショットと手元の確認に使う。
// ?state=unpaired|online|queued（圏外で送信待ちが 2 件）、?tab=Watch|Story|Proposals、?code=ABCD2345
// ?view=compose|thread|detail で、Watch の書く欄・スレッド、Proposals の 1 件の画面を開いた状態にする。
import { render } from "preact";
import { App, TABS, type Tab } from "../src/App.tsx";
import { createController } from "../src/controller.ts";
import { createMockServer } from "../src/mock/mockServer.ts";
import { createMemoryStore } from "../src/outbox/store.ts";
import { mountPoint } from "../src/root.ts";
import { demoComments, demoProposals } from "./fixtures.ts";
import "../src/app.css";

const params = new URLSearchParams(location.search);
const state = params.get("state") ?? "online";
const tab = TABS.find((t): t is Tab => t === params.get("tab"));

const server = createMockServer({
  paired: state !== "unpaired",
  online: state !== "queued",
  comments: demoComments(),
  proposals: demoProposals(),
  cuts: [4, 8],
});
const controller = createController({
  ...server.platform,
  onNetworkBack: () => undefined,
  outboxStore: createMemoryStore(),
});
if (state === "queued") {
  controller.outbox.add({ comp: "c1", at: { num: 4200, den: 1000 }, text: "Check the color of the sky here." });
  controller.outbox.add({ comp: "c1", at: { num: 9000, den: 1000 }, text: "Fix the typo in the title card." });
}
void controller.connection.start();
const ua = params.get("ua") ?? navigator.userAgent;
render(<App controller={controller} search={location.search} userAgent={ua} initialTab={tab} />, mountPoint());

// 画面の中の操作が要る見え方は、描き終わってからボタンを押して作る。
const OPEN_DELAY_MS = 400;
const press = (text: string): void => {
  [...document.querySelectorAll<HTMLElement>("button")].find((b) => b.textContent?.includes(text))?.click();
};
const OPEN: Record<string, string> = { compose: "Comment", thread: "Zoom here", detail: "Slow the zoom" };
const target = OPEN[params.get("view") ?? ""];
if (target) setTimeout(() => press(target), OPEN_DELAY_MS);
