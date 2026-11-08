import type { CommentShape } from "@mm/sdk";
import { useRef, useState } from "preact/hooks";
import { draftShape, fitRect, normalizePoint, pointsAttr, type FrameSize, type Point, type Tool } from "./shapes.ts";

/** 点の半径（コンポジションの長い辺に対する割合）。 */
const POINT_RADIUS = 0.012;
const NO_BOX = { left: 0, top: 0, width: 0, height: 0 };

const ShapeView = ({ shape, frame, draft = false }: { shape: CommentShape; frame: FrameSize; draft?: boolean }) => {
  const cls = draft ? "shape draft" : "shape";
  const [a, b] = shape.points;
  const x = (p: Point): number => p[0] * frame.width;
  const y = (p: Point): number => p[1] * frame.height;
  if (shape.kind === "point" && a) return <circle class={cls} cx={x(a)} cy={y(a)} r={POINT_RADIUS * Math.max(frame.width, frame.height)} />;
  if (shape.kind === "rect" && a && b) {
    return <rect class={cls} x={x(a)} y={y(a)} width={x(b) - x(a)} height={y(b) - y(a)} />;
  }
  return <polyline class={cls} points={pointsAttr(shape.points, frame)} />;
};

export interface Editing {
  tool: Tool;
  onShape: (shape: CommentShape) => void;
}

export interface DrawLayerProps {
  shapes: CommentShape[];
  /** コンポジションの大きさ。映像の矩形（レターボックスを除く）を決める。 */
  frame: FrameSize;
  editing?: Editing;
}

/**
 * 止めたフレームの上に重ねる描き込みの面。座標は映像の矩形を 0〜1 に正規化し、デスクトップの Program と同じ所を指す。
 * 面は映像の枠の全体を覆い、viewBox と `meet` で映像の矩形に合わせる（`object-fit: contain` の映像と同じ収め方）。
 * `editing` を渡すと指で描ける。余白から描いた点は映像の縁に寄せる。渡さなければ見るだけで、タッチは下の映像へ通す。
 */
export const DrawLayer = ({ shapes, frame, editing }: DrawLayerProps) => {
  const svg = useRef<SVGSVGElement>(null);
  const [trail, setTrail] = useState<Point[]>([]);
  const at = (e: PointerEvent): Point => normalizePoint(e.clientX, e.clientY, fitRect(frame, svg.current?.getBoundingClientRect() ?? NO_BOX));
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
      viewBox={`0 0 ${frame.width} ${frame.height}`}
      preserveAspectRatio="xMidYMid meet"
      onPointerDown={editing ? down : undefined}
      onPointerMove={editing ? move : undefined}
      onPointerUp={editing ? up : undefined}
      onPointerCancel={editing ? () => setTrail([]) : undefined}
    >
      {shapes.map((s, i) => (
        <ShapeView key={i} shape={s} frame={frame} />
      ))}
      {live && <ShapeView shape={live} frame={frame} draft />}
    </svg>
  );
};
