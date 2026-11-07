const CODE_GROUP = 4;
const SECOND_MS = 1000;
const SECONDS_PER_MINUTE = 60;
const PAD = 2;

/** 8 文字の符号を読みやすく 4 文字ずつに分ける（`ABCD EFGH`）。手で入れるときは空白を入れても読み替えられる。 */
export const formatCode = (code: string): string =>
  code.match(new RegExp(`.{1,${CODE_GROUP}}`, "g"))?.join(" ") ?? code;

/** 期限までの残り（`4:32`）。過ぎていれば `null`。 */
export const remaining = (expiresAtMs: number, nowMs: number): string | null => {
  const left = Math.ceil((expiresAtMs - nowMs) / SECOND_MS);
  if (left <= 0) return null;
  const s = String(left % SECONDS_PER_MINUTE).padStart(PAD, "0");
  return `${Math.floor(left / SECONDS_PER_MINUTE)}:${s}`;
};

/** 登録した日時（`2026-11-08 09:30`、この PC の時刻）。 */
export const formatPairedAt = (ms: number): string => {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(PAD, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
};
