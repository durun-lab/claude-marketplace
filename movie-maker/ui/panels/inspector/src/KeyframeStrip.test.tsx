// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import type { Keyframe, Layer, Op } from "@mm/sdk";
import { afterEach, describe, expect, it, vi } from "vitest";
import { InspectorPanel, type InspectorPanelProps } from "./InspectorPanel.tsx";
import { SETTLE_TIMEOUT_MS } from "./KeyframeStrip.tsx";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => {
  render(null, host);
  vi.useRealTimers();
});

const t = (num: number, den = 1) => ({ num, den });
const key = (time: number, value: number): Keyframe => ({
  time: t(time),
  value: { type: "number", value },
  interpolation: "linear",
});

// コンポジションの 2 秒から 6 秒（4 秒の範囲）。キーフレームの欄の時刻の範囲は幅 400px、左端は x=100 に置く。
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
  const axis = el?.querySelector<HTMLElement>(".mm-insp-strip-axis");
  if (axis) axis.getBoundingClientRect = () => ({ left: LEFT, width: WIDTH, right: LEFT + WIDTH, top: 0, bottom: 0, height: 0, x: LEFT, y: 0 }) as DOMRect;
  return el;
};
const diamond = (label: string, sec: number) =>
  host.querySelector<HTMLElement>(`[aria-label="${label} keyframe at ${sec.toFixed(2)}s"]`);
// jsdom に onpointerdown 等が無い版では、preact は "PointerDown" の名前で購読する。実ブラウザは小文字。
const eventName = (el: Element, type: string) => {
  if (`on${type}` in el) return type;
  return type.replace(/^pointer(\w)/, (_, c: string) => `Pointer${c.toUpperCase()}`);
};
const fire = (el: Element | null | undefined, type: string, clientX: number, button = 0) =>
  act(() => void el?.dispatchEvent(new MouseEvent(eventName(el, type), { clientX, button, bubbles: true })));
const press = (el: Element | null | undefined, k: string) =>
  act(() => void el?.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true })));
const select = (label: string, sec: number) => {
  strip(label);
  fire(diamond(label, sec), "pointerdown", xAt(sec));
  fire(diamond(label, sec), "pointerup", xAt(sec));
};

describe("パラメータの行のキーフレームの欄", () => {
  it("キーフレームを、レイヤーの範囲に対する位置のひし形で出す", () => {
    mount({ onOp: vi.fn() });
    expect(diamond("opacity", 2)?.style.left).toBe("0%");
    expect(diamond("opacity", 4)?.style.left).toBe("50%");
  });

  it("範囲の長さは (out - in) / speed で、タイムリマップがあっても変わらない", () => {
    const fast = {
      ...layer,
      in: t(1),
      out: t(9),
      speed: { num: 2, den: 1 },
      time_remap: { keyframes: [{ time: t(0), source: t(0), interpolation: "linear" }] },
      opacity: { value: { type: "number", value: 1 }, keyframes: [key(3, 0), key(6, 1)] },
    } as unknown as Layer;
    mount({ onOp: vi.fn(), layer: fast });
    expect(diamond("opacity", 3)?.style.left).toBe("25%");
    expect(diamond("opacity", 6)?.style.left).toBe("100%");
  });

  it("再生位置の縦線を出し、範囲の外では出さない", () => {
    mount({ onOp: vi.fn(), time: 3 });
    expect(strip("opacity")?.querySelector<HTMLElement>(".mm-insp-strip-playhead")?.style.left).toBe("25%");
    mount({ onOp: vi.fn(), time: 7 });
    expect(strip("opacity")?.querySelector(".mm-insp-strip-playhead")).toBeNull();
  });
});

describe("キーフレームの欄の出し方と押し方", () => {
  it("再生位置が届くまでは、キーフレームの欄を出さない（fps が分からずフレームの先頭にそろえられない）", () => {
    mount({ onOp: vi.fn(), time: undefined });
    expect(host.querySelector(".mm-insp-strip")).toBeNull();
  });

  it("キーフレームの無い行は、狭い Inspector で隠すために data-keyed を false にする", () => {
    mount({ onOp: vi.fn() });
    expect(strip("opacity")?.dataset.keyed).toBe("true");
    expect(strip("scale")?.dataset.keyed).toBe("false");
  });

  it("ひし形の無い所を押すと、いちばん近いフレームの先頭へ再生位置を動かす", () => {
    const onSeek = vi.fn();
    mount({ onOp: vi.fn(), onSeek });
    fire(strip("opacity"), "pointerdown", xAt(5.04));
    expect(onSeek).toHaveBeenCalledWith(5);
  });

  it("主ボタンのほかで押しても、再生位置も選択も変えない", () => {
    const onSeek = vi.fn();
    mount({ onOp: vi.fn(), onSeek });
    fire(strip("opacity"), "pointerdown", xAt(5), 2);
    fire(diamond("opacity", 4), "pointerdown", xAt(4), 2);
    expect(onSeek).not.toHaveBeenCalled();
    expect(diamond("opacity", 4)?.dataset.selected).toBe("false");
  });
});

describe("キーフレームの欄の選択", () => {
  it("ひし形を押すと選び（accent の色）、再生位置は動かさない", () => {
    const onSeek = vi.fn();
    mount({ onOp: vi.fn(), onSeek });
    select("opacity", 4);
    expect(diamond("opacity", 4)?.dataset.selected).toBe("true");
    expect(diamond("opacity", 2)?.dataset.selected).toBe("false");
    expect(onSeek).not.toHaveBeenCalled();
  });

  it("ひし形の無い所を押すか、フォーカスが外れると選択を外す", () => {
    mount({ onOp: vi.fn(), onSeek: vi.fn() });
    select("opacity", 4);
    fire(strip("opacity"), "pointerdown", xAt(5));
    expect(diamond("opacity", 4)?.dataset.selected).toBe("false");
    select("opacity", 4);
    act(() => host.querySelector<HTMLInputElement>(".mm-insp-input")?.focus());
    expect(diamond("opacity", 4)?.dataset.selected).toBe("false");
  });
});

describe("キーフレームの欄のドラッグ", () => {
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

describe("キーフレームの欄のドラッグの取りやめと離した後", () => {
  it("Esc でドラッグを取りやめ、離しても操作を送らない", () => {
    const onOp = vi.fn();
    mount({ onOp });
    const s = strip("opacity");
    const d = diamond("opacity", 4);
    fire(d, "pointerdown", xAt(4));
    fire(d, "pointermove", xAt(5));
    press(s, "Escape");
    expect(diamond("opacity", 4)?.style.left).toBe("50%");
    fire(d, "pointerup", xAt(5));
    expect(onOp).not.toHaveBeenCalled();
  });

  it("離した後は、操作の結果が届くまで動かした先にひし形を残し、届かなければ戻す", () => {
    vi.useFakeTimers();
    mount({ onOp: vi.fn() });
    strip("opacity");
    const d = diamond("opacity", 4);
    fire(d, "pointerdown", xAt(4));
    fire(d, "pointermove", xAt(5));
    fire(d, "pointerup", xAt(5));
    expect(diamond("opacity", 4)?.style.left).toBe("75%");
    act(() => void vi.advanceTimersByTime(SETTLE_TIMEOUT_MS));
    expect(diamond("opacity", 4)?.style.left).toBe("50%");
  });
});

describe("キーフレームの欄のキー操作", () => {
  it("Delete で選んだキーフレームを removeKeyframe で外し、アプリへキーを渡さない", () => {
    const onOp = vi.fn();
    mount({ onOp });
    select("opacity", 4);
    const e = new KeyboardEvent("keydown", { key: "Delete", bubbles: true, cancelable: true });
    act(() => void strip("opacity")?.dispatchEvent(e));
    expect(onOp).toHaveBeenCalledWith({ op: "removeKeyframe", target: { layer: "l1", path: "opacity" }, time: t(4) });
    expect(e.defaultPrevented).toBe(true);
  });

  it("入力欄の Delete と Backspace は文字の削除のまま（キーフレームを外さない）", () => {
    const onOp = vi.fn();
    mount({ onOp });
    select("opacity", 4);
    const input = host.querySelector<HTMLInputElement>('[aria-label="opacity"]');
    press(input, "Delete");
    press(input, "Backspace");
    expect(onOp).not.toHaveBeenCalled();
  });

  it("選んだキーフレームがもう無ければ、何も送らない", () => {
    const onOp = vi.fn();
    mount({ onOp });
    select("opacity", 4);
    const moved = { ...layer, opacity: { value: { type: "number", value: 1 }, keyframes: [key(2, 0)] } } as unknown as Layer;
    mount({ onOp, layer: moved });
    press(strip("opacity"), "Delete");
    expect(onOp).not.toHaveBeenCalled();
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

  it("オンにするとキーフレームのあるパラメータの行と、その区画だけを出す", () => {
    mount({ onOp: vi.fn() });
    expect(labels()).toContain("scale");
    act(() => host.querySelector<HTMLInputElement>(".mm-insp-keyedonly input")?.click());
    expect(labels().sort()).toEqual(["opacity", "rotation"]);
    const titles = [...host.querySelectorAll(".mm-insp-titletext")].map((s) => s.textContent);
    expect(titles).toEqual(["Layer", "Transform"]);
  });
});

describe("区画のキーフレームの欄", () => {
  it("中のパラメータのキーフレームを時刻ごとに 1 つのひし形（まとめのひし形）にして出す", () => {
    mount({ onOp: vi.fn() });
    const transform = [...host.querySelectorAll(".mm-insp-section")].find((s) => s.querySelector(".mm-insp-titletext")?.textContent === "Transform");
    const lefts = [...(transform?.querySelectorAll<HTMLElement>("[data-summary-key]") ?? [])].map((e) => e.style.left);
    expect(lefts).toEqual(["50%", "75%"]);
  });

  it("キーフレームの無い区画は data-keyed を false にする", () => {
    mount({ onOp: vi.fn() });
    const crop = [...host.querySelectorAll(".mm-insp-section")].find((s) => s.querySelector(".mm-insp-titletext")?.textContent === "Crop");
    expect(crop?.querySelector<HTMLElement>(".mm-insp-strip-summary")?.dataset.keyed).toBe("false");
  });
});
