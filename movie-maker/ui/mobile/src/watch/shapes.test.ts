import { describe, expect, it } from "vitest";
import { DEFAULT_FRAME, fitRect, normalizePoint, pointsAttr } from "./shapes.ts";

/** iPhone 14 の縦向きの Watch の映像の枠（幅 390 の 16:9）。 */
const STAGE = { left: 0, top: 47, width: 390, height: 219.375 };
const SHORT = { width: 1080, height: 1920 };
const WIDE = { width: 1920, height: 1080 };

describe("映像の矩形", () => {
  it("デスクトップの fit_rect と同じ値になる（mm-ui-layout の geom.rs の試験と同じ入力）", () => {
    const r = fitRect(WIDE, { left: 0, top: 0, width: 1000, height: 1000 });
    expect(r.width).toBeCloseTo(1000);
    expect(r.left).toBeCloseTo(0);
    expect(r.height).toBeCloseTo(562.5);
    expect(r.top).toBeCloseTo(218.75);
  });

  it("縦長のコンポジションは 16:9 の枠の中央に左右の余白を残して収まる", () => {
    const r = fitRect(SHORT, STAGE);
    expect(r.height).toBeCloseTo(STAGE.height);
    expect(r.width).toBeCloseTo((STAGE.height * SHORT.width) / SHORT.height);
    expect(r.left).toBeCloseTo((STAGE.width - r.width) / 2);
    expect(r.top).toBeCloseTo(STAGE.top);
  });

  it("大きさ 0 は空の矩形", () => {
    expect(fitRect({ width: 0, height: 9 }, STAGE).width).toBe(0);
    expect(fitRect(SHORT, { ...STAGE, width: 0 }).height).toBe(0);
  });
});

describe("描くときの座標", () => {
  it("16:9 のコンポジションは枠の全体を基準にし、映像の矩形にしても値が変わらない", () => {
    for (const frame of [WIDE, DEFAULT_FRAME]) {
      expect(fitRect(frame, STAGE)).toEqual(STAGE);
      expect(normalizePoint(97.5, 47 + 164.53125, fitRect(frame, STAGE))).toEqual(normalizePoint(97.5, 47 + 164.53125, STAGE));
    }
  });

  it("縦長のコンポジションは映像の矩形を基準にし、デスクトップの Program の同じ所を指す", () => {
    const r = fitRect(SHORT, STAGE);
    const [x, y] = normalizePoint(r.left + 0.25 * r.width, r.top + 0.75 * r.height, r);
    expect(x).toBeCloseTo(0.25);
    expect(y).toBeCloseTo(0.75);
    // デスクトップの Program（1000x1000 の領域）で同じ点を映像の矩形に戻すと、同じ割合の所になる。
    const desk = fitRect(SHORT, { left: 0, top: 0, width: 1000, height: 1000 });
    expect(desk.left + x * desk.width).toBeCloseTo(desk.left + 0.25 * desk.width);
    expect(desk.top + y * desk.height).toBeCloseTo(750);
  });

  it("左右の余白から描いた点は映像の縁に寄せる", () => {
    const r = fitRect(SHORT, STAGE);
    expect(normalizePoint(5, STAGE.top, r)).toEqual([0, 0]);
    expect(normalizePoint(STAGE.width - 5, STAGE.top + STAGE.height, r)).toEqual([1, 1]);
  });
});

describe("見せるときの座標", () => {
  it("正規化した点はコンポジションの大きさの viewBox の座標にする", () => {
    expect(pointsAttr([[0.25, 0.75], [1, 0]], SHORT)).toBe("270,1440 1080,0");
  });
});
