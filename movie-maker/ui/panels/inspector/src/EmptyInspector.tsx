import type { CompositionSummary } from "@mm/sdk";

const DIGITS = 2;
const SECS_PER_MIN = 60;
const MINS_PER_HOUR = 60;
const SECS_PER_HOUR = SECS_PER_MIN * MINS_PER_HOUR;
/** フレームの境目の誤差。`mm-ui-layout` の `header.rs` の `FRAME_EPS` と同じ。 */
const FRAME_EPS = 1e-6;
const FPS_DECIMALS = 100;

const pad = (n: number) => String(n).padStart(DIGITS, "0");

/**
 * 時刻 `sec`（秒）のタイムコード `HH:MM:SS:FF`。Timeline の見出しの列の上の角（`mm-ui-layout` の `header::timecode`）と
 * 同じ決まりで、フレームは `sec` を含むフレーム（切り捨て）、1 秒のフレームの数は `fps` を丸めた数（ノンドロップ）。
 */
export const timecode = (sec: number, fps: number): string => {
  const rate = Math.max(1, fps);
  const perSec = Math.round(rate);
  const frames = Math.floor(Math.max(0, sec) * rate + FRAME_EPS);
  const secs = Math.floor(frames / perSec);
  const fields = [Math.floor(secs / SECS_PER_HOUR), Math.floor(secs / SECS_PER_MIN) % MINS_PER_HOUR, secs % SECS_PER_MIN];
  return [...fields, frames % perSec].map(pad).join(":");
};

/** fps を小数で出す（29.97 は 29.97、30 は 30）。 */
const fpsText = (fps: number): string => String(Math.round(fps * FPS_DECIMALS) / FPS_DECIMALS);

const rows = (c: CompositionSummary): [string, string][] => [
  ["Name", c.name],
  ["Size", `${c.width} × ${c.height}`],
  ["Frame rate", `${fpsText(c.fps)} fps`],
  ["Duration", timecode(c.duration, c.fps)],
  ["Layers", String(c.layers)],
];

/** 何も選んでいない Inspector。コンポジションの要約と、次にすること。 */
export const EmptyInspector = ({ composition }: { composition?: CompositionSummary }) => (
  <div class="mm-insp mm-insp-empty">
    {composition && (
      <dl class="mm-insp-summary" aria-label="Composition">
        {rows(composition).map(([term, value]) => (
          <div key={term} class="mm-insp-summary-row">
            <dt>{term}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    )}
    <p class="mm-insp-hint">Select a layer in the Timeline to see its properties.</p>
  </div>
);
