// @vitest-environment jsdom
import type { CommentShape } from "@mm/sdk";
import { render } from "preact";
import { act } from "preact/test-utils";
import { describe, expect, it } from "vitest";
import { DrawLayer } from "./DrawLayer.tsx";

/** 縦向きの Watch の映像の枠（幅 390 の 16:9）。 */
const STAGE = { left: 0, top: 0, width: 390, height: 219.375 };
const SHORT = { width: 1080, height: 1920 };

const mount = (onShape: (s: CommentShape) => void, shapes: CommentShape[] = []): SVGSVGElement => {
  const root = document.createElement("div");
  render(<DrawLayer shapes={shapes} frame={SHORT} editing={{ tool: "stroke", onShape }} />, root);
  const svg = root.querySelector("svg");
  if (!svg) throw new Error("描く面が無い");
  svg.getBoundingClientRect = () => ({ ...STAGE, x: 0, y: 0, right: STAGE.width, bottom: STAGE.height, toJSON: () => STAGE });
  svg.setPointerCapture = () => undefined;
  return svg;
};

// jsdom の要素には onpointerdown などが無く、preact はそのとき名前の大小を保って購読する（PointerDown）。
const EVENT_NAMES: Record<string, string> = { pointerdown: "PointerDown", pointerup: "PointerUp" };

const pointer = (svg: SVGSVGElement, type: string, x: number, y: number): Promise<void> =>
  act(() => void svg.dispatchEvent(new MouseEvent(EVENT_NAMES[type] ?? type, { clientX: x, clientY: y, bubbles: true })));

describe("DrawLayer", () => {
  it("縦長のコンポジションでは、映像の矩形を基準にした座標で描いた形を渡す", async () => {
    const got: CommentShape[] = [];
    const svg = mount((s) => got.push(s));
    const width = (STAGE.height * SHORT.width) / SHORT.height;
    const left = (STAGE.width - width) / 2;
    await pointer(svg, "pointerdown", left + 0.25 * width, 0.75 * STAGE.height);
    await pointer(svg, "pointerup", left + 0.25 * width, 0.75 * STAGE.height);
    const [p] = got[0]?.points ?? [];
    expect(got[0]?.kind).toBe("point");
    expect(p?.[0]).toBeCloseTo(0.25);
    expect(p?.[1]).toBeCloseTo(0.75);
  });

  it("見せる面は viewBox をコンポジションの大きさにし、映像と同じく中央に収める", () => {
    const svg = mount(() => undefined, [{ kind: "rect", points: [[0.5, 0.25], [1, 0.5]] }]);
    expect(svg.getAttribute("viewBox")).toBe("0 0 1080 1920");
    expect(svg.getAttribute("preserveAspectRatio")).toBe("xMidYMid meet");
    const rect = svg.querySelector("rect");
    expect([rect?.getAttribute("x"), rect?.getAttribute("y"), rect?.getAttribute("width"), rect?.getAttribute("height")]).toEqual(["540", "480", "540", "480"]);
  });
});
