import { useRef } from "preact/hooks";
import { fractionOf, markFractions, timeAt } from "./scrub.ts";
import { mmss } from "./time.ts";

const KEY_STEP_SECONDS = 1;
const PERCENT = 100;

const pct = (fraction: number): string => `${fraction * PERCENT}%`;

export interface ScrubberProps {
  time: number;
  duration: number;
  /** カットの境（秒）。 */
  cuts: number[];
  /** コメントの時刻（秒）。 */
  comments: number[];
  onSeek: (seconds: number) => void;
}

/** カットの境とコメントのマーカーの付いたスクラブバー。押して引くとシークする。 */
export const Scrubber = ({ time, duration, cuts, comments, onSeek }: ScrubberProps) => {
  const bar = useRef<HTMLDivElement>(null);
  const seekTo = (e: PointerEvent): void => {
    const box = bar.current?.getBoundingClientRect();
    if (box) onSeek(timeAt(e.clientX, box, duration));
  };
  const dragging = useRef(false);
  const key = (e: KeyboardEvent): void => {
    if (e.key === "ArrowRight") onSeek(Math.min(duration, time + KEY_STEP_SECONDS));
    if (e.key === "ArrowLeft") onSeek(Math.max(0, time - KEY_STEP_SECONDS));
  };
  return (
    <div class="scrub">
      <span class="clock">{mmss(time)}</span>
      <div
        ref={bar}
        class="track"
        role="slider"
        tabIndex={0}
        aria-label="Playback position"
        aria-valuemin={0}
        aria-valuemax={Math.round(duration)}
        aria-valuenow={Math.round(time)}
        onKeyDown={key}
        onPointerDown={(e) => {
          dragging.current = true;
          bar.current?.setPointerCapture(e.pointerId);
          seekTo(e);
        }}
        onPointerMove={(e) => dragging.current && seekTo(e)}
        onPointerUp={() => (dragging.current = false)}
        onPointerCancel={() => (dragging.current = false)}
      >
        <i class="rail" />
        <i class="played" style={{ width: pct(fractionOf(time, duration)) }} />
        {markFractions(cuts, duration).map((f) => (
          <i key={`cut${f}`} class="cut" style={{ left: pct(f) }} />
        ))}
        {markFractions(comments, duration).map((f, i) => (
          <i key={`c${i}`} class="mark" style={{ left: pct(f) }} />
        ))}
        <i class="head" style={{ left: pct(fractionOf(time, duration)) }} />
      </div>
      <span class="clock">{mmss(duration)}</span>
    </div>
  );
};
