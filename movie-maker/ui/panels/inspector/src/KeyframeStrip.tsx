import { type Keyframe, type Layer, type Op, type Param, type ParamRef, type RationalTime, frameTime, sameTime, toSec } from "@mm/sdk";
import { useRef, useState } from "preact/hooks";

/** レイヤーのコンポジション上の範囲（秒）。イン点からアウト点まで。 */
export interface LayerSpan {
  start: number;
  end: number;
}

/** `[start, start + (out - in) / speed)`。キーフレームの時刻もこの時間軸（コンポジションの時刻）で持つ。 */
export const layerSpan = (layer: Layer): LayerSpan => {
  const start = toSec(layer.start);
  const speed = layer.speed ? layer.speed.num / layer.speed.den : 1;
  return { start, end: start + (toSec(layer.out) - toSec(layer.in)) / speed };
};

const PERCENT = 100;
const SEC_DIGITS = 2;

const fraction = (span: LayerSpan, sec: number): number => {
  const length = span.end - span.start;
  return length > 0 ? (sec - span.start) / length : 0;
};

const inSpan = (span: LayerSpan, sec: number): boolean => sec >= span.start && sec <= span.end;

/** 同じ時刻のひし形を 1 つにする。 */
const uniqueTimes = (times: readonly RationalTime[]): RationalTime[] =>
  times.reduce<RationalTime[]>((acc, t) => (acc.some((u) => sameTime(u, t)) ? acc : [...acc, t]), []);

const Playhead = ({ span, sec }: { span: LayerSpan; sec?: number }) =>
  sec !== undefined && inSpan(span, sec) ? (
    <span class="mm-insp-strip-head" style={{ left: `${fraction(span, sec) * PERCENT}%` }} />
  ) : null;

interface SummaryProps {
  span: LayerSpan;
  /** 中のパラメータのキーフレームの時刻。 */
  times: readonly RationalTime[];
  playheadSec?: number;
}

/** 組の見出しの行に出す、中のパラメータのキーフレームをまとめた欄。見るだけで操作は受けない。 */
export const KeyframeSummary = ({ span, times, playheadSec }: SummaryProps) => (
  <div class="mm-insp-strip mm-insp-strip-summary" role="img" aria-label="Keyframes in group">
    <div class="mm-insp-strip-track">
      <Playhead span={span} sec={playheadSec} />
      {uniqueTimes(times)
        .filter((t) => inSpan(span, toSec(t)))
        .map((t) => (
          <span
            key={`${t.num}/${t.den}`}
            class="mm-insp-strip-key"
            data-summary-key=""
            style={{ left: `${fraction(span, toSec(t)) * PERCENT}%` }}
          />
        ))}
    </div>
  </div>
);

export interface KeyframeStripProps {
  target: ParamRef;
  param: Param;
  label: string;
  span: LayerSpan;
  fps: number;
  /** 再生位置（コンポジションの秒）。 */
  playheadSec?: number;
  onOp: (op: Op) => void;
  /** 欄のクリックで再生位置を動かす。 */
  onSeek?: (sec: number) => void;
}

interface Drag {
  key: Keyframe;
  to: RationalTime;
}

const keySec = (k: Keyframe) => toSec(k.time);

type StripArgs = Pick<KeyframeStripProps, "target" | "param" | "span" | "fps" | "onOp">;

/** 選択とドラッグの状態、離したときの操作。ドラッグ中は表示だけを動かし、データは変えない。 */
const useStripDrag = ({ target, param, span, fps, onOp }: StripArgs) => {
  // 時刻と x の対応は、端のひし形がはみ出さないよう内側に取った track の幅で決める。
  const box = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<RationalTime | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const keys = param.keyframes ?? [];

  const frameAt = (clientX: number): RationalTime => {
    const rect = box.current?.getBoundingClientRect();
    const f = rect && rect.width > 0 ? Math.min(1, Math.max(0, (clientX - rect.left) / rect.width)) : 0;
    const t = frameTime(span.start + f * (span.end - span.start), fps);
    const lo = frameTime(span.start, fps);
    const hi = frameTime(span.end, fps);
    return toSec(t) < toSec(lo) ? lo : toSec(t) > toSec(hi) ? hi : t;
  };

  const startDrag = (k: Keyframe) => (e: PointerEvent) => {
    e.stopPropagation();
    box.current?.parentElement?.focus();
    setSelected(k.time);
    setDrag({ key: k, to: k.time });
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
  };
  const moveDrag = (e: PointerEvent) => {
    if (drag) setDrag({ key: drag.key, to: frameAt(e.clientX) });
  };
  const endDrag = (e: PointerEvent) => {
    if (!drag) return;
    const to = frameAt(e.clientX);
    setDrag(null);
    const occupied = keys.some((k) => k !== drag.key && sameTime(k.time, to));
    if (sameTime(to, drag.key.time) || occupied) return;
    setSelected(to);
    onOp({ op: "moveKeyframe", target, from: drag.key.time, keyframe: { ...drag.key, time: to } });
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if ((e.key !== "Delete" && e.key !== "Backspace") || !selected) return;
    e.preventDefault();
    setSelected(null);
    onOp({ op: "removeKeyframe", target, time: selected });
  };
  return { box, selected, drag, setDrag, frameAt, startDrag, moveDrag, endDrag, onKeyDown };
};

/**
 * パラメータの行の右に置く時間の欄（Premiere の Effect Controls の右の欄の形）。
 * ひし形のドラッグ中は表示だけを動かし、離したときに moveKeyframe を 1 回送る（取り消し 1 回で戻る）。
 */
export const KeyframeStrip = (props: KeyframeStripProps) => {
  const { param, label, span, playheadSec, onSeek } = props;
  const s = useStripDrag(props);
  return (
    <div
      class="mm-insp-strip"
      tabIndex={0}
      role="group"
      aria-label={`${label} keyframes`}
      onPointerDown={(e) => onSeek?.(toSec(s.frameAt(e.clientX)))}
      onKeyDown={s.onKeyDown}
    >
      <div class="mm-insp-strip-track" ref={s.box}>
        <Playhead span={span} sec={playheadSec} />
        {(param.keyframes ?? [])
          .filter((k) => inSpan(span, keySec(k)) || s.drag?.key === k)
          .map((k) => {
            const isSelected = s.selected !== null && sameTime(s.selected, k.time);
            const dragging = s.drag?.key === k;
            return (
              <span
                key={`${k.time.num}/${k.time.den}`}
                class="mm-insp-strip-key"
                role="button"
                aria-label={`${label} keyframe at ${keySec(k).toFixed(SEC_DIGITS)}s`}
                aria-pressed={isSelected}
                data-selected={isSelected}
                data-dragging={dragging}
                style={{ left: `${fraction(span, dragging && s.drag ? toSec(s.drag.to) : keySec(k)) * PERCENT}%` }}
                onPointerDown={s.startDrag(k)}
                onPointerMove={s.moveDrag}
                onPointerUp={s.endDrag}
                onPointerCancel={() => s.setDrag(null)}
              />
            );
          })}
      </div>
    </div>
  );
};
