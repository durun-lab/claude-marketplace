import type { CommentShape } from "@mm/sdk";
import { useRef, useState } from "preact/hooks";
import { draftShape, normalizePoint, pointsAttr, type Point, type Tool } from "./shapes.ts";

const POINT_RADIUS = 0.012;

const ShapeView = ({ shape, draft = false }: { shape: CommentShape; draft?: boolean }) => {
  const cls = draft ? "shape draft" : "shape";
  const [a, b] = shape.points;
  if (shape.kind === "point" && a) return <circle class={cls} cx={a[0]} cy={a[1]} r={POINT_RADIUS} />;
  if (shape.kind === "rect" && a && b) {
    return <rect class={cls} x={a[0]} y={a[1]} width={b[0] - a[0]} height={b[1] - a[1]} />;
  }
  return <polyline class={cls} points={pointsAttr(shape.points)} />;
};

export interface Editing {
  tool: Tool;
  onShape: (shape: CommentShape) => void;
}

/**
 * 止めたフレームの上に重ねる描き込みの面。座標は 0〜1 に正規化し、表示の大きさが変わっても位置が合う。
 * `editing` を渡すと指で描ける。渡さなければ見るだけで、タッチは下の映像へ通す。
 */
export const DrawLayer = ({ shapes, editing }: { shapes: CommentShape[]; editing?: Editing }) => {
  const svg = useRef<SVGSVGElement>(null);
  const [trail, setTrail] = useState<Point[]>([]);
  const at = (e: PointerEvent): Point =>
    normalizePoint(e.clientX, e.clientY, svg.current?.getBoundingClientRect() ?? { left: 0, top: 0, width: 0, height: 0 });
  const down = (e: PointerEvent): void => {
    svg.current?.setPointerCapture(e.pointerId);
    setTrail([at(e)]);
  };
  const move = (e: PointerEvent): void => {
    if (trail.length > 0) setTrail((t) => [...t, at(e)]);
  };
  const up = (): void => {
    const shape = editing ? draftShape(editing.tool, trail) : null;
    if (shape) editing?.onShape(shape);
    setTrail([]);
  };
  const live = editing ? draftShape(editing.tool, trail) : null;
  return (
    <svg
      ref={svg}
      class={`draw${editing ? " editing" : ""}`}
      viewBox="0 0 1 1"
      preserveAspectRatio="none"
      onPointerDown={editing ? down : undefined}
      onPointerMove={editing ? move : undefined}
      onPointerUp={editing ? up : undefined}
      onPointerCancel={editing ? () => setTrail([]) : undefined}
    >
      {shapes.map((s, i) => (
        <ShapeView key={i} shape={s} />
      ))}
      {live && <ShapeView shape={live} draft />}
    </svg>
  );
};
