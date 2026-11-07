import type { Fade, FadeCurve, Layer, LayerAudio, Mask, Op, Param } from "@mm/sdk";

const num = (value: number): Param => ({ value: { type: "number", value } });
const vec2 = (x: number, y: number): Param => ({ value: { type: "vec2", value: [x, y] } });

/** 新しいマスクの矩形の初期値（レイヤーの座標）。 */
const DEFAULT_POSITION: [number, number] = [0, 0];
const DEFAULT_WIDTH = 640;
const DEFAULT_HEIGHT = 360;
const DEFAULT_RADIUS = 24;

/**
 * レイヤーのマスクと重ならない `<レイヤー>-mask-N`。マスクの ID はプロジェクト全体で重ならない決まりなので、
 * プロジェクトで一意のレイヤーの ID を頭に付け、ほかのレイヤーのマスクとも重ならないようにする。
 */
export const nextMaskId = (layerId: string, masks: readonly Mask[]): string => {
  const used = new Set(masks.map((m) => m.id));
  for (let n = masks.length + 1; ; n += 1) {
    const id = `${layerId}-mask-${n}`;
    if (!used.has(id)) return id;
  }
};

/** 角丸の矩形のマスクを足す操作。 */
export const addRectMaskOp = (layer: Layer): Op => ({
  op: "addMask",
  layer: layer.id,
  mask: {
    id: nextMaskId(layer.id, layer.masks ?? []),
    rect: {
      position: vec2(...DEFAULT_POSITION),
      size: vec2(DEFAULT_WIDTH, DEFAULT_HEIGHT),
      radius: num(DEFAULT_RADIUS),
    },
  },
});

/** マスクを外す操作。 */
export const removeMaskOp = (maskId: string): Op => ({ op: "removeMask", mask: maskId });

/** フェードの長さの精度（1 / 1000 秒）。 */
const FADE_DEN = 1000;

export const FADE_CURVES: readonly FadeCurve[] = ["linear", "equal_power", "db"];

/** 曲線の表示名（UI の文言は英語）。dB は dB の直線。 */
export const FADE_CURVE_LABELS: Record<FadeCurve, string> = {
  linear: "Linear",
  equal_power: "Equal power",
  db: "dB linear",
};

export type FadeEdge = "fade_in" | "fade_out";

/** フェードの長さ（秒）。無ければ 0。 */
export const fadeSeconds = (fade: Fade | null | undefined): number =>
  fade ? fade.duration.num / fade.duration.den : 0;

/** フェードの長さを変える操作。0 以下ならフェードを外す。曲線は今のものを保つ。音の設定が無ければ null。 */
export const setFadeDurationOp = (layer: Layer, edge: FadeEdge, seconds: number): Op | null => {
  const audio = layer.audio;
  if (!audio) return null;
  const ticks = Math.round(seconds * FADE_DEN);
  const { [edge]: current, ...rest } = audio;
  const next: LayerAudio = ticks > 0 ? { ...rest, [edge]: { ...current, duration: { num: ticks, den: FADE_DEN } } } : rest;
  return { op: "setLayerAudio", layer: layer.id, audio: next };
};

/** フェードの曲線を変える操作。フェードが無ければ null。 */
export const setFadeCurveOp = (layer: Layer, edge: FadeEdge, curve: FadeCurve): Op | null => {
  const fade = layer.audio?.[edge];
  if (!layer.audio || !fade) return null;
  return { op: "setLayerAudio", layer: layer.id, audio: { ...layer.audio, [edge]: { ...fade, curve } } };
};
