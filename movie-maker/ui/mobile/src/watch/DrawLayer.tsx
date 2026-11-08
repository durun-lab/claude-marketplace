import type { CommentShape } from "@mm/sdk";
import { useRef, useState } from "preact/hooks";
import { draftShape, fitRect, normalizePoint, pointsAttr, type CompSize, type Point, type Tool } from "./shapes.ts";

const NO_BOX = { left: 0, top: 0, width: 0, height: 0 };

const ShapeView = ({ shape, size, draft = false }: { shape: CommentShape; size: CompSize; draft?: boolean }) => {
  const cls = draft ? "shape draft" : "shape";
  const [a, b] = shape.points;
  const x = (p: Point): number => p[0] * size.width;
  const y = (p: Point): number => p[1] * size.height;
  // 点は長さ 0 の線を丸い端で描き、直径を画面のピクセルで決める（デスクトップの Program の点と同じ 12 px）。
  if (shape.kind === "point" && a) return <path class={`${cls} point`} d={`M${x(a)} ${y(a)}h0`} />;
  if (shape.kind === "rect" && a && b) {
    return <rect class={cls} x={x(a)} y={y(a)} width={x(b) - x(a)} height={y(b) - y(a)} />;
  }
  return <polyline class={cls} points={pointsAttr(shape.points, size)} />;
};

export interface Editing {
  tool: Tool;
  onShape: (shape: CommentShape) => void;
}

export interface DrawLayerProps {
  shapes: CommentShape[];
  /** コンポジションの大きさ。映像の矩形（レターボックスを除く）を決める。 */
  size: CompSize;
  editing?: Editing;
}

/**
 * 止めたフレームの上に重ねる描き込みの SVG。座標は映像の矩形を 0〜1 に正規化し、デスクトップの Program と同じ所を指す。
 * SVG は映像の領域の全体を覆い、viewBox と `meet` で映像の矩形に合わせる（`object-fit: contain` の映像と同じ収め方）。
 * `editing` を渡すと指で描ける。レターボックスの余白から描いた点は映像の縁に寄せる。渡さなければ見るだけで、タッチは下の映像へ通す。
 */
export const DrawLayer = ({ shapes, size, editing }: DrawLayerProps) => {
  const svg = useRef<SVGSVGElement>(null);
  const [trail, setTrail] = useState<Point[]>([]);
  const at = (e: PointerEvent): Point => normalizePoint(e.clientX, e.clientY, fitRect(size, svg.current?.getBoundingClientRect() ?? NO_BOX));
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
      viewBox={`0 0 ${size.width} ${size.height}`}
      preserveAspectRatio="xMidYMid meet"
      onPointerDown={editing ? down : undefined}
      onPointerMove={editing ? move : undefined}
      onPointerUp={editing ? up : undefined}
      onPointerCancel={editing ? () => setTrail([]) : undefined}
    >
      {shapes.map((s, i) => (
        <ShapeView key={i} shape={s} size={size} />
      ))}
      {live && <ShapeView shape={live} size={size} draft />}
    </svg>
  );
};
