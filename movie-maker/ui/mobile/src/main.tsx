import { render } from "preact";
import { App } from "./App.tsx";
import { createController, type Platform } from "./controller.ts";
import { registerServiceWorker } from "./sw/register.ts";
import { mountPoint } from "./root.ts";
import "./app.css";

const browserPlatform = (): Platform => ({
  fetch: (input, init) => fetch(input, init),
  openEvents: (url) => new EventSource(url) as unknown as ReturnType<Platform["openEvents"]>,
  onNetworkBack: (fn) => window.addEventListener("online", fn),
  storage: window.localStorage,
});

const controller = createController(browserPlatform());
void controller.connection.start();
render(<App controller={controller} search={location.search} userAgent={navigator.userAgent} />, mountPoint());
void registerServiceWorker();
