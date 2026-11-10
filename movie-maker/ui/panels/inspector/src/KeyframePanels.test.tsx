// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import type { Layer } from "@mm/sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { KeyframePanels, PLAYHEAD_EVENT, SEEK_EVENT } from "./KeyframePanels.tsx";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => render(null, host));

const t = (num: number) => ({ num, den: 1 });
const layer = {
  id: "l1",
  start: t(0),
  in: t(0),
  out: t(4),
  opacity: {
    value: { type: "number", value: 1 },
    keyframes: [
      { time: t(0), value: { type: "number", value: 0 }, interpolation: "linear" },
      { time: t(2), value: { type: "number", value: 1 }, interpolation: "linear" },
    ],
  },
  transform: {},
  source: { kind: "demo", version: 1, params: {} },
} as unknown as Layer;

describe("KeyframePanels", () => {
  it("再生位置のイベントでキーフレームナビゲーターを出し、前後の移動はシークのイベントにする", () => {
    act(() => render(<KeyframePanels layer={layer} onOp={vi.fn()} />, host));
    expect(host.querySelector('[aria-label="opacity keyframe at playhead"]')).toBeNull();
    act(() => {
      window.dispatchEvent(new CustomEvent(PLAYHEAD_EVENT, { detail: { time: 1, fps: 30 } }));
    });
    const seeks: number[] = [];
    const onSeek = (e: Event) => seeks.push((e as CustomEvent<{ time: number }>).detail.time);
    window.addEventListener(SEEK_EVENT, onSeek);
    act(() => host.querySelector<HTMLButtonElement>('[aria-label="opacity next keyframe"]')?.click());
    window.removeEventListener(SEEK_EVENT, onSeek);
    expect(seeks).toEqual([2]);
  });

  it("Inspector で名前を押したパラメータを Graph に出す", () => {
    act(() => render(<KeyframePanels layer={layer} onOp={vi.fn()} />, host));
    expect(host.querySelector(".mm-graph")?.textContent).toContain("Select a parameter");
    act(() => host.querySelector<HTMLButtonElement>(".mm-insp-pick")?.click());
    expect(host.querySelector(".mm-graph-title")?.textContent).toBe("Graph: opacity");
    expect(host.querySelectorAll(".mm-graph-key")).toHaveLength(2);
  });

  it("Timeline のキーフレームの行で選んだパラメータを Inspector と Graph で選び、選び直すたびに従う", () => {
    const show = (serial: number) =>
      act(() => render(<KeyframePanels layer={layer} param={{ path: "opacity", serial }} onOp={vi.fn()} />, host));
    const selected = () => host.querySelector('.mm-insp-row[data-selected="true"]');
    show(1);
    expect(host.querySelector(".mm-graph-title")?.textContent).toBe("Graph: opacity");
    expect(selected()?.querySelector(".mm-insp-label")?.textContent).toContain("opacity");
    const scale = [...host.querySelectorAll<HTMLButtonElement>(".mm-insp-pick")].find((b) => b.textContent?.includes("scale"));
    act(() => scale?.click());
    expect(host.querySelector(".mm-graph-title")?.textContent).toBe("Graph: scale");
    show(1);
    const kept = host.querySelector(".mm-graph-title")?.textContent;
    expect(kept, "同じ選択が送り直されても Inspector で選んだものを保つ").toBe("Graph: scale");
    show(2);
    expect(host.querySelector(".mm-graph-title")?.textContent).toBe("Graph: opacity");
  });
});

describe("KeyframePanels の入力中の入力欄", () => {
  it("止まっている間に同じ再生位置が届いても、入力中の入力欄を戻さない", () => {
    const playhead = () => {
      window.dispatchEvent(new CustomEvent(PLAYHEAD_EVENT, { detail: { time: 1, fps: 30 } }));
    };
    act(() => render(<KeyframePanels layer={layer} onOp={vi.fn()} />, host));
    act(playhead);
    const input = host.querySelector<HTMLInputElement>('input[aria-label="opacity"]');
    if (!input) throw new Error("opacity の入力欄が無い");
    input.value = "0.75";
    act(() => void input.dispatchEvent(new Event("input", { bubbles: true })));
    act(playhead);
    expect(input.value).toBe("0.75");
  });

  it("再生中に再生位置が進んでも、入力中の入力欄を戻さない", () => {
    const at = (time: number) => () => {
      window.dispatchEvent(new CustomEvent(PLAYHEAD_EVENT, { detail: { time, fps: 30 } }));
    };
    const fixedLayer = {
      ...layer,
      transform: { position: { value: { type: "vec2", value: [10, 20] } } },
    } as unknown as Layer;
    act(() => render(<KeyframePanels layer={fixedLayer} onOp={vi.fn()} />, host));
    act(at(1));
    const input = host.querySelector<HTMLInputElement>('input[aria-label="position x"]');
    if (!input) throw new Error("position x の入力欄が無い");
    act(() => void input.focus());
    input.value = "123";
    act(() => void input.dispatchEvent(new Event("input", { bubbles: true })));
    act(at(1.1));
    act(at(1.2));
    expect(input.value).toBe("123");
  });
});
