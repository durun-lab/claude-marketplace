// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { ASSET_SELECTED_EVENT } from "@mm/sdk";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { PLAYHEAD_EVENT, SEEK_EVENT } from "../../panels/inspector/src/KeyframePanels.tsx";
import { LAYER_SELECTED_EVENT, LAYER_SELECTION_REQUEST_EVENT } from "./Shell.tsx";

/**
 * アプリが initialization_script で shell に入れる WebView の橋。jsdom の試験は Vite が `new URL(…, import.meta.url)` を
 * ページの URL に書き換えるので、ディレクトリのパスから組む。
 */
const BRIDGE = readFileSync(resolve(import.meta.dirname, "../../../apps/movie-maker/web/transport.js"), "utf8");
const INFO = { fps: 30, duration: 10 };
const SELECTION = { layer: null, assets: [], composition: null };
const ANSWERS: Record<string, unknown> = { project_info: INFO, layer_selection: SELECTION };

type Handler = (e: { payload: unknown }) => void;
const tauriEvents = new Map<string, Handler>();
const invoke = vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>(
  async (command) => ANSWERS[command],
);

/** Tauri の Channel の代わり。 */
class FakeChannel {
  onmessage: ((msg: unknown) => void) | null = null;
}
/** 橋が `subscribe` に渡した Channel。 */
let channel: FakeChannel | null = null;
/** 橋が始まった直後のページの要素の数（body の子、head の style）。 */
let added = { body: -1, styles: -1 };

const flush = () => new Promise<void>((ok) => setTimeout(ok, 0));
const argsOf = (command: string) => invoke.mock.calls.filter(([c]) => c === command).map(([, args]) => args);
const key = (init: KeyboardEventInit, target: EventTarget = window) => {
  const e = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(e);
  return e;
};
/** DOM のイベントの detail を集める。 */
const record = (name: string) => {
  const seen: unknown[] = [];
  window.addEventListener(name, (e) => seen.push((e as CustomEvent).detail));
  return seen;
};
const nowUs = () => Math.round((performance.timeOrigin + performance.now()) * 1000);

beforeAll(async () => {
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    cb(0);
    return 0;
  });
  const listen = async (name: string, handler: Handler) => {
    tauriEvents.set(name, handler);
    return () => undefined;
  };
  Object.assign(window, { __TAURI__: { core: { invoke, Channel: FakeChannel }, event: { listen } } });
  new Function(BRIDGE)();
  await flush();
  channel = (argsOf("subscribe")[0]?.["onTick"] as FakeChannel | undefined) ?? null;
  added = { body: document.body.children.length, styles: document.head.querySelectorAll("style").length };
});

beforeEach(() => {
  invoke.mockClear();
  document.body.replaceChildren();
});

describe("WebView の橋のキー", () => {
  it("画面を持たず、再生位置の通知を購読する", () => {
    expect(added).toEqual({ body: 0, styles: 0 });
    expect(channel?.onmessage).toBeTypeOf("function");
  });

  it("WebView のキーをアプリのキーの表に送り、押し続けは繰り返しの印を付け、Space はページのスクロールに使わせない", () => {
    const space = key({ code: "Space", key: " " });
    key({ code: "KeyZ", key: "Z", ctrlKey: true, shiftKey: true });
    key({ code: "KeyL", key: "l", repeat: true });
    expect(argsOf("key_event")).toEqual([
      { source: "webview", key: "Space", repeat: false },
      { source: "webview", key: "Ctrl+Shift+KeyZ", repeat: false },
      { source: "webview", key: "KeyL", repeat: true },
    ]);
    expect(space.defaultPrevented).toBe(true);
  });

  it("入力欄のキーと、パネルが処理したキーは送らない", () => {
    const input = document.body.appendChild(document.createElement("input"));
    key({ code: "Space", key: " " }, input);
    const panel = document.body.appendChild(document.createElement("div"));
    panel.addEventListener("keydown", (e) => e.preventDefault());
    key({ code: "ArrowLeft", key: "ArrowLeft" }, panel);
    expect(argsOf("key_event")).toEqual([]);
  });

  it("ボタンにフォーカスがあるときは編集のキーを送らず、再生のキーは送る", () => {
    const button = document.body.appendChild(document.createElement("button"));
    key({ code: "Delete", key: "Delete" }, button);
    key({ code: "KeyX", key: "x", altKey: true }, button);
    key({ code: "Comma", key: "," }, button);
    key({ code: "Space", key: " " }, button);
    expect(argsOf("key_event")).toEqual([{ source: "webview", key: "Space", repeat: false }]);
  });
});

describe("WebView の橋のイベント", () => {
  it("再生位置の通知を fps 付きの再生位置のイベントにして、画面に出した時刻を返す", () => {
    const seen = record(PLAYHEAD_EVENT);
    channel?.onmessage?.({ seq: 7, t: 2.5, sent_us: nowUs(), playing: false, lag_ms: 1 });
    expect(seen).toEqual([{ time: 2.5, fps: INFO.fps }]);
    expect(argsOf("sync_ack")).toEqual([
      { seq: 7, recvUs: expect.any(Number), rafUs: expect.any(Number), shownT: 2.5 },
    ]);
  });

  it("再生中は送られてから経った分だけ進め、コンポジションの長さで止める", () => {
    channel?.onmessage?.({ seq: 8, t: INFO.duration - 0.01, sent_us: nowUs() - 1_000_000, playing: true, lag_ms: 1 });
    expect(argsOf("sync_ack")[0]?.["shownT"]).toBe(INFO.duration);
  });

  it("Timeline の選択を shell の DOM のイベントにつなぎ、fps と長さを取り直す", async () => {
    const seen = record(LAYER_SELECTED_EVENT);
    tauriEvents.get(LAYER_SELECTED_EVENT)?.({ payload: { layer: null, assets: [] } });
    await flush();
    expect(seen).toEqual([{ layer: null, assets: [] }]);
    expect(argsOf("project_info")).toHaveLength(1);
  });

  it("shell の頼みには、今の選択を読んで選択の DOM のイベントで答える", async () => {
    const seen = record(LAYER_SELECTED_EVENT);
    window.dispatchEvent(new CustomEvent(LAYER_SELECTION_REQUEST_EVENT));
    await flush();
    expect(argsOf("layer_selection")).toHaveLength(1);
    expect(seen).toEqual([SELECTION]);
  });

  it("パネルが選んだ素材は Source で開き、シークの頼みは再生位置を動かす", () => {
    window.dispatchEvent(new CustomEvent(ASSET_SELECTED_EVENT, { detail: { asset: "a1" } }));
    window.dispatchEvent(new CustomEvent(SEEK_EVENT, { detail: { time: 3 } }));
    window.dispatchEvent(new CustomEvent(SEEK_EVENT, { detail: { time: Number.NaN } }));
    expect(argsOf("source_open")).toEqual([{ asset: "a1" }]);
    expect(argsOf("transport")).toEqual([{ action: "seek", value: 3 }]);
  });
});
