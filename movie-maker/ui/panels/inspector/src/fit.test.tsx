// @vitest-environment jsdom
import { render } from "preact";
import { useRef } from "preact/hooks";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fitHeight, MIN_FIT_HEIGHT, useFitToColumn } from "./fit.ts";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => {
  render(null, host);
  vi.unstubAllGlobals();
});

describe("fitHeight", () => {
  it("列の見えている高さから上の位置と下の余白を引き、狭くても最小を残す", () => {
    expect(fitHeight(900, 44, 12)).toBe(844);
    expect(fitHeight(200, 44, 12)).toBe(MIN_FIT_HEIGHT);
  });
});

describe("useFitToColumn", () => {
  const Box = () => {
    const ref = useRef<HTMLDivElement>(null);
    useFitToColumn(ref);
    return <div ref={ref} data-box />;
  };

  it("スクロールする列の見えている所の下端までの高さにし、列の大きさが変わると測り直す", () => {
    const fits: Array<() => void> = [];
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(cb: () => void) {
          fits.push(cb);
        }
        observe() {}
        disconnect() {}
      },
    );
    const column = document.createElement("div");
    column.style.overflowY = "auto";
    column.style.paddingBottom = "12px";
    let visible = 900;
    Object.defineProperty(column, "clientHeight", { get: () => visible });
    host.append(column);
    act(() => render(<Box />, column));
    const box = column.querySelector<HTMLElement>("[data-box]");
    if (!box) throw new Error("箱が無い");
    box.getBoundingClientRect = () => ({ top: 44 }) as DOMRect;
    act(() => fits[0]?.());
    expect(box.style.height).toBe("844px");
    visible = 600;
    act(() => fits[0]?.());
    expect(box.style.height).toBe("544px");
    render(null, column);
    column.remove();
  });
});
