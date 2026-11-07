import type { CommentShape } from "@mm/sdk";

export type Point = [number, number];
export type Tool = "stroke" | "rect";

/** 画面上の座標を、描く面の左上を (0, 0)、右下を (1, 1) にした座標にする。はみ出しは縁に寄せる。 */
export const normalizePoint = (
  clientX: number,
  clientY: number,
  box: { left: number; top: number; width: number; height: number },
): Point => {
  if (box.width <= 0 || box.height <= 0) return [0, 0];
  const clamp = (v: number): number => Math.min(1, Math.max(0, v));
  return [clamp((clientX - box.left) / box.width), clamp((clientY - box.top) / box.height)];
};

/** 点を間引く間隔（正規化した座標）。指のぶれで同じ所に溜まる点を省く。 */
const MIN_STEP = 0.004;

/** 線を描き終えたときの形。近すぎる点を間引き、点 1 つだけなら `point` にする。 */
export const strokeShape = (raw: Point[]): CommentShape => {
  const points = raw.filter((p, i) => {
    const prev = raw[i - 1];
    return !prev || Math.hypot(p[0] - prev[0], p[1] - prev[1]) >= MIN_STEP || i === raw.length - 1;
  });
  const first = points[0];
  if (points.length <= 1 && first) return { kind: "point", points: [first] };
  return { kind: "stroke", points };
};

/** 矩形の形。対角の 2 点を、左上と右下の順にそろえる。 */
export const rectShape = (a: Point, b: Point): CommentShape => ({
  kind: "rect",
  points: [
    [Math.min(a[0], b[0]), Math.min(a[1], b[1])],
    [Math.max(a[0], b[0]), Math.max(a[1], b[1])],
  ],
});

/** 描いている途中の形。ツールと始点・今の点から作る。 */
export const draftShape = (tool: Tool, trail: Point[]): CommentShape | null => {
  const first = trail[0];
  const last = trail[trail.length - 1];
  if (!first || !last) return null;
  return tool === "rect" ? rectShape(first, last) : strokeShape(trail);
};

/** SVG の `points` 属性にする。座標系は viewBox `0 0 1 1`。 */
export const pointsAttr = (points: Point[]): string => points.map(([x, y]) => `${x},${y}`).join(" ");
