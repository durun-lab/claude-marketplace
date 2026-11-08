import { render } from "preact";
import { App } from "./App.tsx";
import { createController, type Platform } from "./controller.ts";
import { createIdbStore } from "./outbox/store.ts";
import { registerServiceWorker } from "./sw/register.ts";
import { mountPoint } from "./root.ts";
import "./app.css";

const browserPlatform = (): Platform => ({
  fetch: (input, init) => fetch(input, init),
  openEvents: (url) => new EventSource(url) as unknown as ReturnType<Platform["openEvents"]>,
  onNetworkBack: (fn) => window.addEventListener("online", fn),
  onVisibility: (fn) => document.addEventListener("visibilitychange", () => fn(document.visibilityState === "visible")),
  outboxStore: createIdbStore(),
});

const controller = createController(browserPlatform());
void controller.connection.start();
render(<App controller={controller} search={location.search} userAgent={navigator.userAgent} />, mountPoint());
void registerServiceWorker();
// 容量が足りないときに送信待ちのキューを消されないよう頼む。ホーム画面の Web アプリと入れた PWA で通りやすい
void navigator.storage?.persist?.().catch(() => false);
