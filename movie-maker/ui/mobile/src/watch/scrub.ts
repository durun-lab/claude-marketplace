/** スクラブバーの上の位置（0〜1）から時刻（秒）を求める。 */
export const timeAt = (clientX: number, box: { left: number; width: number }, duration: number): number => {
  if (box.width <= 0 || duration <= 0) return 0;
  return Math.min(1, Math.max(0, (clientX - box.left) / box.width)) * duration;
};

/** 時刻（秒）のスクラブバーの上の位置（0〜1）。尺が分からないうちは 0。 */
export const fractionOf = (seconds: number, duration: number): number =>
  duration > 0 ? Math.min(1, Math.max(0, seconds / duration)) : 0;

/** 目盛りの位置（0〜1）の配列。尺の外の時刻は捨てる。 */
export const markFractions = (times: number[], duration: number): number[] =>
  duration > 0 ? times.filter((t) => t >= 0 && t <= duration).map((t) => t / duration) : [];
