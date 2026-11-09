import type { CompositionSummary } from "@mm/sdk";

const DIGITS = 2;
const SECS_PER_MIN = 60;
const MINS_PER_HOUR = 60;
const FPS_DECIMALS = 100;

const pad = (n: number) => String(n).padStart(DIGITS, "0");

/** 時:分:秒:フレーム。画面の時刻の書き方（`00:01:23:12`）。 */
export const timecode = (sec: number, fps: number): string => {
  const rate = Math.max(1, Math.round(fps));
  const total = Math.max(0, Math.round(sec * rate));
  const frames = total % rate;
  const secs = Math.floor(total / rate);
  const mins = Math.floor(secs / SECS_PER_MIN);
  return [Math.floor(mins / MINS_PER_HOUR), mins % MINS_PER_HOUR, secs % SECS_PER_MIN, frames].map(pad).join(":");
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
