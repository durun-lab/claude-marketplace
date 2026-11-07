// 偽の母艦で PWA を動かすページ。スクリーンショットと手元の確認用で、母艦には載せない。
// ?state=unpaired|online|queued（圏外で送信待ちが 2 件）、?tab=Watch|Story|Comments|Proposals、?code=ABCD2345
import { render } from "preact";
import { App, TABS, type Tab } from "../src/App.tsx";
import { createController } from "../src/controller.ts";
import { createMockServer } from "../src/mock/mockServer.ts";
import { mountPoint } from "../src/root.ts";
import "../src/app.css";

const params = new URLSearchParams(location.search);
const state = params.get("state") ?? "online";
const tab = TABS.find((t): t is Tab => t === params.get("tab"));

const memory = new Map<string, string>();
const server = createMockServer({
  paired: state !== "unpaired",
  online: state !== "queued",
  comments: [
    { id: "m2", text: "The intro cut feels late.", state: "fresh", at: null, author: "You" },
    { id: "m1", text: "Lower the BGM under the narration.", state: "done", at: null, author: "You" },
  ],
});
const controller = createController({
  ...server.platform,
  onNetworkBack: () => undefined,
  storage: { getItem: (k) => memory.get(k) ?? null, setItem: (k, v) => void memory.set(k, v) },
});
if (state === "queued") {
  controller.outbox.add("Check the color of the sky at 0:42.", null);
  controller.outbox.add("Fix the typo in the title card.", null);
}
void controller.connection.start();
const ua = params.get("ua") ?? navigator.userAgent;
render(<App controller={controller} search={location.search} userAgent={ua} initialTab={tab} />, mountPoint());
