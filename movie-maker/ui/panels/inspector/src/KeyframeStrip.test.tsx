// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import type { Keyframe, Layer, Op } from "@mm/sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { InspectorPanel, type InspectorPanelProps } from "./InspectorPanel.tsx";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => render(null, host));

const t = (num: number, den = 1) => ({ num, den });
const key = (time: number, value: number): Keyframe => ({
  time: t(time),
  value: { type: "number", value },
  interpolation: "linear",
});

// コンポジションの 2 秒から 6 秒（4 秒の範囲）。欄の幅は 400px、左端は x=100 に置く。
const layer = {
  id: "l1",
  start: t(2),
  in: t(0),
  out: t(4),
  opacity: { value: { type: "number", value: 1 }, keyframes: [key(2, 0), key(4, 1)] },
  transform: { rotation: { value: { type: "number", value: 0 }, keyframes: [key(4, 0), key(5, 1)] } },
  source: { kind: "demo", version: 1, params: {} },
} as unknown as Layer;

const LEFT = 100;
const WIDTH = 400;
const xAt = (sec: number) => LEFT + ((sec - 2) / 4) * WIDTH;

const mount = (props: Partial<InspectorPanelProps> & { onOp: (op: Op) => void }) =>
  render(<InspectorPanel layer={layer} fps={10} time={3} {...props} />, host);

const strip = (label: string) => {
  const el = host.querySelector<HTMLElement>(`[aria-label="${label} keyframes"]`);
  const track = el?.querySelector<HTMLElement>(".mm-insp-strip-track");
  if (track) track.getBoundingClientRect = () => ({ left: LEFT, width: WIDTH, right: LEFT + WIDTH, top: 0, bottom: 0, height: 0, x: LEFT, y: 0 }) as DOMRect;
  return el;
};
const diamond = (label: string, sec: number) =>
  host.querySelector<HTMLElement>(`[aria-label="${label} keyframe at ${sec.toFixed(2)}s"]`);
// jsdom に onpointerdown 等が無い版では、preact は "PointerDown" の名前で購読する。実ブラウザは小文字。
const eventName = (el: Element, type: string) => {
  if (`on${type}` in el) return type;
  return type.replace(/^pointer(\w)/, (_, c: string) => `Pointer${c.toUpperCase()}`);
};
const fire = (el: Element | null | undefined, type: string, clientX: number) =>
  act(() => void el?.dispatchEvent(new MouseEvent(eventName(el, type), { clientX, bubbles: true })));

describe("パラメータの行の時間の欄", () => {
  it("キーフレームを、レイヤーの範囲に対する位置のひし形で出す", () => {
    mount({ onOp: vi.fn() });
    expect(diamond("opacity", 2)?.style.left).toBe("0%");
    expect(diamond("opacity", 4)?.style.left).toBe("50%");
  });

  it("再生位置の縦線を出し、範囲の外では出さない", () => {
    mount({ onOp: vi.fn(), time: 3 });
    expect(strip("opacity")?.querySelector<HTMLElement>(".mm-insp-strip-head")?.style.left).toBe("25%");
    mount({ onOp: vi.fn(), time: 7 });
    expect(strip("opacity")?.querySelector(".mm-insp-strip-head")).toBeNull();
  });

  it("欄のクリックで再生位置を動かす（フレームにそろえる）", () => {
    const onSeek = vi.fn();
    mount({ onOp: vi.fn(), onSeek });
    fire(strip("opacity"), "pointerdown", xAt(5.04));
    expect(onSeek).toHaveBeenCalledWith(5);
  });

  it("ひし形を選ぶと accent の印が付き、クリックでは再生位置を動かさない", () => {
    const onSeek = vi.fn();
    mount({ onOp: vi.fn(), onSeek });
    strip("opacity");
    fire(diamond("opacity", 4), "pointerdown", xAt(4));
    expect(diamond("opacity", 4)?.dataset.selected).toBe("true");
    expect(diamond("opacity", 2)?.dataset.selected).toBe("false");
    expect(onSeek).not.toHaveBeenCalled();
  });

});

describe("時間の欄の操作", () => {
  it("ドラッグ中は操作を送らず、離したときに moveKeyframe を 1 回だけ送る", () => {
    const onOp = vi.fn();
    mount({ onOp });
    strip("opacity");
    const d = diamond("opacity", 4);
    fire(d, "pointerdown", xAt(4));
    fire(d, "pointermove", xAt(4.5));
    fire(d, "pointermove", xAt(5));
    expect(onOp).not.toHaveBeenCalled();
    expect(diamond("opacity", 4)?.style.left).toBe("75%");
    fire(d, "pointerup", xAt(5));
    expect(onOp).toHaveBeenCalledTimes(1);
    expect(onOp).toHaveBeenCalledWith({
      op: "moveKeyframe",
      target: { layer: "l1", path: "opacity" },
      from: t(4),
      keyframe: { ...key(4, 1), time: t(5) },
    });
  });

  it("動かさずに離す、または別のキーフレームの上に落とすと操作を送らない", () => {
    const onOp = vi.fn();
    mount({ onOp });
    strip("opacity");
    const d = diamond("opacity", 4);
    fire(d, "pointerdown", xAt(4));
    fire(d, "pointerup", xAt(4));
    fire(d, "pointerdown", xAt(4));
    fire(d, "pointerup", xAt(2));
    expect(onOp).not.toHaveBeenCalled();
  });

});

describe("時間の欄のキー操作", () => {
  it("Delete で選んだキーフレームを removeKeyframe で外す", () => {
    const onOp = vi.fn();
    mount({ onOp });
    const s = strip("opacity");
    fire(diamond("opacity", 4), "pointerdown", xAt(4));
    fire(diamond("opacity", 4), "pointerup", xAt(4));
    act(() => void s?.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true })));
    expect(onOp).toHaveBeenCalledWith({ op: "removeKeyframe", target: { layer: "l1", path: "opacity" }, time: t(4) });
  });

  it("ツールチップは Premiere の呼び名", () => {
    mount({ onOp: vi.fn(), onSeek: vi.fn() });
    const titles = [...host.querySelectorAll("button[title]")].map((b) => b.getAttribute("title"));
    expect(titles).toContain("Go to Previous Keyframe");
    expect(titles).toContain("Add/Remove Keyframe");
    expect(titles).toContain("Go to Next Keyframe");
  });
});

describe("Show Only Keyframed Properties", () => {
  const labels = () => [...host.querySelectorAll(".mm-insp-label")].map((l) => l.textContent);

  it("オンにするとキーフレームのあるパラメータの行と、その組だけを出す", () => {
    mount({ onOp: vi.fn() });
    expect(labels()).toContain("scale");
    act(() => host.querySelector<HTMLInputElement>(".mm-insp-keyedonly input")?.click());
    expect(labels().sort()).toEqual(["opacity", "rotation"]);
    const titles = [...host.querySelectorAll(".mm-insp-titletext")].map((s) => s.textContent);
    expect(titles).toEqual(["Layer", "Transform"]);
  });
});

describe("組の見出しのまとめたひし形", () => {
  it("中のパラメータのキーフレームを時刻ごとに 1 つにして、見るだけで出す", () => {
    mount({ onOp: vi.fn() });
    const transform = [...host.querySelectorAll(".mm-insp-section")].find((s) => s.querySelector(".mm-insp-titletext")?.textContent === "Transform");
    const lefts = [...(transform?.querySelectorAll<HTMLElement>("[data-summary-key]") ?? [])].map((e) => e.style.left);
    expect(lefts).toEqual(["50%", "75%"]);
  });

});
