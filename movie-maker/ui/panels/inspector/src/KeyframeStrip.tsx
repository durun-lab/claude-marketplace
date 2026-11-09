import { type Keyframe, type Layer, type Op, type Param, type ParamRef, type RationalTime, frameTime, sameTime, toSec } from "@mm/sdk";
import { useEffect, useMemo, useRef, useState } from "preact/hooks";

/** レイヤーのコンポジションの時間軸での範囲（秒）。イン点からアウト点まで。 */
export interface LayerSpan {
  start: number;
  end: number;
}

/**
 * `[start, start + (out - in) / speed)`（mm-doc の `Layer::range`）。タイムリマップがあっても範囲は変わらない。
 * キーフレームの時刻もこの時間軸（コンポジションの時刻）で持つ。
 */
export const layerSpan = (layer: Layer): LayerSpan => {
  const start = toSec(layer.start);
  const speed = layer.speed ? layer.speed.num / layer.speed.den : 1;
  return { start, end: start + (toSec(layer.out) - toSec(layer.in)) / speed };
};

const PERCENT = 100;
const SEC_DIGITS = 2;
/** 離した後、操作の結果が届かなくても動かした先にひし形を見せ続ける上限（ms）。拒否された操作のひし形を残さない（Graph と同じ）。 */
export const SETTLE_TIMEOUT_MS = 1500;
const NO_KEYS: readonly Keyframe[] = [];

const leftOf = (span: LayerSpan, sec: number): string => {
  const length = span.end - span.start;
  return `${(length > 0 ? (sec - span.start) / length : 0) * PERCENT}%`;
};

const inSpan = (span: LayerSpan, sec: number): boolean => sec >= span.start && sec <= span.end;

/** 時刻の順に並べ、同じ時刻を 1 つにする。並べた後は隣とだけ比べればよい。 */
const uniqueTimes = (times: readonly RationalTime[]): RationalTime[] =>
  [...times]
    .sort((a, b) => toSec(a) - toSec(b))
    .filter((t, i, all) => {
      const prev = all[i - 1];
      return prev === undefined || !sameTime(prev, t);
    });

const PlayheadLine = ({ span, sec }: { span: LayerSpan; sec: number }) =>
  inSpan(span, sec) ? <span class="mm-insp-strip-playhead" style={{ left: leftOf(span, sec) }} /> : null;

interface SummaryProps {
  span: LayerSpan;
  /** 区画の中のパラメータのキーフレームの時刻。再生位置が動いても同じ配列を渡すと、ひし形を描き直さない。 */
  times: readonly RationalTime[];
  playheadSec: number;
}

/** 区画の見出しのキーフレームの欄。中のパラメータのキーフレームを時刻ごとに 1 つのひし形（まとめのひし形）にして見せ、操作は受けない。 */
export const KeyframeSummary = ({ span, times, playheadSec }: SummaryProps) => {
  const marks = useMemo(
    () =>
      uniqueTimes(times)
        .filter((t) => inSpan(span, toSec(t)))
        .map((t) => (
          <span key={`${t.num}/${t.den}`} class="mm-insp-strip-key" data-summary-key="" style={{ left: leftOf(span, toSec(t)) }} />
        )),
    [times, span],
  );
  return (
    <div class="mm-insp-strip mm-insp-strip-summary" data-keyed={times.length > 0} role="img" aria-label="Keyframes in section">
      <div class="mm-insp-strip-axis">
        <PlayheadLine span={span} sec={playheadSec} />
        {marks}
      </div>
    </div>
  );
};

export interface KeyframeStripProps {
  target: ParamRef;
  param: Param;
  label: string;
  /** 再生位置が動いても同じオブジェクトを渡すと、ひし形を描き直さない。 */
  span: LayerSpan;
  fps: number;
  /** 再生位置（コンポジションの秒）。 */
  playheadSec: number;
  onOp: (op: Op) => void;
  /** キーフレームの欄のひし形の無い所を押したときに、再生位置を動かす。 */
  onSeek?: (sec: number) => void;
}

/** 動かしているキーフレームの今の時刻と、置き先の時刻。キーフレームは時刻で見分ける（掴んだ後に新しいレイヤーのデータが届いてもずれない）。 */
interface Move {
  from: RationalTime;
  to: RationalTime;
}

/** x を、範囲の中のいちばん近いフレームの先頭の時刻にする。 */
const frameAtX = (axis: HTMLElement | null, span: LayerSpan, fps: number, clientX: number): RationalTime => {
  const rect = axis?.getBoundingClientRect();
  const f = rect && rect.width > 0 ? Math.min(1, Math.max(0, (clientX - rect.left) / rect.width)) : 0;
  const t = frameTime(span.start + f * (span.end - span.start), fps);
  const lo = frameTime(span.start, fps);
  const hi = frameTime(span.end, fps);
  return toSec(t) < toSec(lo) ? lo : toSec(t) > toSec(hi) ? hi : t;
};

/** 押したひし形のキーフレーム。ひし形の外なら undefined。 */
const keyOf = (keys: readonly Keyframe[], target: EventTarget | null): Keyframe | undefined => {
  const index = target instanceof Element ? target.closest<HTMLElement>("[data-index]")?.dataset.index : undefined;
  return index === undefined ? undefined : keys[Number(index)];
};

/** 離した後、操作の結果（新しい param）が届くまで動かした先のひし形を保つ。すぐ戻すと元の位置へ跳ね返って見える。 */
const useHeld = (keyframes: Param["keyframes"]) => {
  const [held, setHeld] = useState<Move | null>(null);
  useEffect(() => setHeld(null), [keyframes]);
  useEffect(() => {
    if (!held) return;
    const timer = setTimeout(() => setHeld(null), SETTLE_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [held]);
  return [held, setHeld] as const;
};

type StripArgs = Pick<KeyframeStripProps, "target" | "param" | "span" | "fps" | "onOp" | "onSeek">;

/**
 * ひし形を押すと選び、ドラッグ中は表示だけを動かして、離したときに moveKeyframe を 1 回送る（取り消し 1 回で戻る）。
 * ひし形の無い所を押すと選択を外して再生位置を動かす。押すのは主ボタンだけ（右クリックはメニューに残す）。
 */
const useStripPointer = ({ target, param, span, fps, onOp, onSeek }: StripArgs) => {
  const axis = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<RationalTime | null>(null);
  const [drag, setDrag] = useState<Move | null>(null);
  const [held, setHeld] = useHeld(param.keyframes);
  const keys = param.keyframes ?? NO_KEYS;
  const frameAt = (clientX: number) => frameAtX(axis.current, span, fps, clientX);
  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    // Delete を受けるためにフォーカスを取る。半分隠れたキーフレームの欄を押しても Inspector をスクロールさせない。
    (e.currentTarget as HTMLElement).focus({ preventScroll: true });
    const key = keyOf(keys, e.target);
    setSelected(key?.time ?? null);
    if (!key) {
      onSeek?.(toSec(frameAt(e.clientX)));
      return;
    }
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
    setDrag({ from: key.time, to: key.time });
  };
  const onPointerMove = (e: PointerEvent) => {
    if (drag) setDrag({ from: drag.from, to: frameAt(e.clientX) });
  };
  const onPointerUp = (e: PointerEvent) => {
    if (!drag) return;
    const to = frameAt(e.clientX);
    const key = keys.find((k) => sameTime(k.time, drag.from));
    setDrag(null);
    // 置き先にキーフレームがある（動かしていないか、別のキーフレームの上）なら送らない。mm-ops も重なりを拒否する。
    if (!key || keys.some((k) => sameTime(k.time, to))) return;
    setSelected(to);
    setHeld({ from: drag.from, to });
    onOp({ op: "moveKeyframe", target, from: key.time, keyframe: { ...key, time: to } });
  };
  return { axis, keys, selected, setSelected, drag, setDrag, moving: drag ?? held, onPointerDown, onPointerMove, onPointerUp };
};

type StripState = ReturnType<typeof useStripPointer>;

/**
 * キーフレームの欄にフォーカスがあるときのキー。Delete と Backspace で選んだキーフレームを外し、Esc でドラッグを取りやめる。
 * 処理したキーは preventDefault し、アプリの Timeline の Lift と Extract に渡さない（`web/transport.js`）。
 */
const stripKeyDown = (s: StripState, target: ParamRef, onOp: (op: Op) => void) => (e: KeyboardEvent) => {
  if (e.key === "Escape" && s.drag) {
    e.preventDefault();
    s.setDrag(null);
    return;
  }
  if (e.key !== "Delete" && e.key !== "Backspace") return;
  const selected = s.selected;
  const key = selected && s.keys.find((k) => sameTime(k.time, selected));
  if (!key) return;
  e.preventDefault();
  s.setSelected(null);
  onOp({ op: "removeKeyframe", target, time: key.time });
};

interface MarksProps {
  keys: readonly Keyframe[];
  span: LayerSpan;
  label: string;
  selected: RationalTime | null;
  /** ドラッグ中か、離して操作の結果を待っているキーフレーム。 */
  moving: Move | null;
}

/** キーフレームのひし形。押したときに見分けるため、並びの位置を data-index に持つ。 */
const KeyMarks = ({ keys, span, label, selected, moving }: MarksProps) => (
  <>
    {keys.map((k, i) => {
      const to = moving && sameTime(k.time, moving.from) ? moving.to : null;
      const sec = toSec(to ?? k.time);
      if (!inSpan(span, sec)) return null;
      const isSelected = selected !== null && sameTime(selected, k.time);
      return (
        <span
          key={`${k.time.num}/${k.time.den}`}
          class="mm-insp-strip-key"
          role="button"
          aria-label={`${label} keyframe at ${toSec(k.time).toFixed(SEC_DIGITS)}s`}
          aria-pressed={isSelected}
          data-index={i}
          data-selected={isSelected}
          data-moving={to !== null}
          style={{ left: leftOf(span, sec) }}
        />
      );
    })}
  </>
);

/**
 * パラメータの行のキーフレームの欄（Premiere の Effect Controls の右の時間の表示の形）。
 * 再生位置の線は毎回描き、ひし形はキーフレーム、範囲、選択、ドラッグが変わったときだけ描き直す（再生中も重くしない）。
 */
export const KeyframeStrip = (props: KeyframeStripProps) => {
  const { target, label, span, playheadSec, onOp } = props;
  const s = useStripPointer(props);
  const marks = useMemo(
    () => <KeyMarks keys={s.keys} span={span} label={label} selected={s.selected} moving={s.moving} />,
    [s.keys, span, label, s.selected, s.moving],
  );
  return (
    <div
      class="mm-insp-strip"
      tabIndex={-1}
      role="group"
      aria-label={`${label} keyframes`}
      data-keyed={s.keys.length > 0}
      onPointerDown={s.onPointerDown}
      onPointerMove={s.onPointerMove}
      onPointerUp={s.onPointerUp}
      onPointerCancel={() => s.setDrag(null)}
      onKeyDown={stripKeyDown(s, target, onOp)}
      onBlur={() => s.setSelected(null)}
    >
      <div class="mm-insp-strip-axis" ref={s.axis}>
        <PlayheadLine span={span} sec={playheadSec} />
        {marks}
      </div>
    </div>
  );
};
