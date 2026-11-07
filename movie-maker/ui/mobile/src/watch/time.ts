import type { RationalTime } from "@mm/sdk";
import type { CommentItem } from "../api/types.ts";

const TICKS_PER_SECOND = 1000;
const SECONDS_PER_MINUTE = 60;
const PAD = 2;

/** 有理数の時刻を秒にする。分母が 0 のときは 0。 */
export const toSeconds = (t: RationalTime): number => (t.den === 0 ? 0 : t.num / t.den);

/** 秒をミリ秒の分母の有理数にする。負は 0 に切る。 */
export const fromSeconds = (seconds: number): RationalTime => ({
  num: Math.round(Math.max(0, seconds) * TICKS_PER_SECOND),
  den: TICKS_PER_SECOND,
});

/** 秒を `m:ss` にする。 */
export const mmss = (seconds: number): string => {
  const whole = Math.floor(Math.max(0, seconds));
  const s = String(whole % SECONDS_PER_MINUTE).padStart(PAD, "0");
  return `${Math.floor(whole / SECONDS_PER_MINUTE)}:${s}`;
};

/** コメントの頭の時刻（秒）。範囲なら始め。 */
export const commentStart = (c: CommentItem): number => {
  if (c.range) return toSeconds(c.range.in);
  return c.at ? toSeconds(c.at) : 0;
};

/** 一覧に出す時刻の表記。範囲は `0:12–0:20`。 */
export const commentWhen = (c: CommentItem): string =>
  c.range ? `${mmss(toSeconds(c.range.in))}–${mmss(toSeconds(c.range.out))}` : mmss(commentStart(c));
