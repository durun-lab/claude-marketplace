import type { Rating, Topic } from "@mm/sdk";
import type { StoryWord } from "@mm/story/src/api.ts";
import { compareTime, spanOf, type Span } from "@mm/story/src/model.ts";

/** 単語の選び方。1 回目で始め、別の単語の 2 回目で範囲にする（ドラッグはスクロールとぶつかる）。 */
export interface Pick {
  /** 1 回目に押した単語。範囲が決まると `null`。 */
  anchor: number | null;
  span: Span | null;
}

export const NO_PICK: Pick = { anchor: null, span: null };

/** 単語を押したときの次の状態。始めの単語をもう一度押すと外し、範囲が決まった後の 1 回目は新しい始めになる。 */
export const tapWord = (pick: Pick, index: number): Pick => {
  if (pick.anchor === null) return { anchor: index, span: { from: index, to: index } };
  if (pick.anchor === index) return NO_PICK;
  return { anchor: null, span: spanOf(pick.anchor, index) };
};

/** 話題の範囲 `[in, out)` に始まりが入る単語の範囲。無ければ `null`。 */
export const spanOfTopic = (words: readonly StoryWord[], topic: Topic): Span | null => {
  let from = -1;
  let to = -1;
  words.forEach((w, i) => {
    if (compareTime(topic.in, w.start) <= 0 && compareTime(w.start, topic.out) < 0) {
      if (from < 0) from = i;
      to = i;
    }
  });
  return from < 0 ? null : { from, to };
};

/** 選んだ単語の範囲に重なる話題。 */
export const topicsOver = (words: readonly StoryWord[], topics: readonly Topic[], span: Span | null): Topic[] => {
  if (!span) return [];
  const first = words[span.from];
  const last = words[span.to];
  if (!first || !last) return [];
  return topics.filter((t) => compareTime(t.in, last.start) <= 0 && compareTime(first.start, t.out) < 0);
};

/** 評価の画面の文言。FCP の Favorite / Reject / 未評価に対応させる。 */
export const RATING_LABELS: Record<Rating, string> = { favorite: "Keep", rejected: "Reject", unrated: "Hold" };

const ASCII_CHAR = /^[\x20-\x7e]$/;

/** 話題の見出しの既定。選んだ単語の文字を先頭から。ASCII どうしの単語の間は空ける（日本語はそのまま続ける）。 */
export const titleOf = (selected: readonly StoryWord[], maxChars: number): string => {
  let title = "";
  for (const w of selected.filter((x) => !x.deleted)) {
    const text = w.fixed ?? w.text;
    const spaced = ASCII_CHAR.test(title.at(-1) ?? "") && ASCII_CHAR.test(text[0] ?? "");
    title += (spaced ? " " : "") + text;
  }
  return title.slice(0, maxChars).trim();
};

/** 並べ替え。`from` の要素を `to` の位置へ移す。範囲外は同じ並びを返す。 */
export const moveItem = <T,>(list: readonly T[], from: number, to: number): T[] => {
  if (from === to || from < 0 || to < 0 || from >= list.length || to >= list.length) return [...list];
  const next = [...list];
  next.splice(to, 0, ...next.splice(from, 1));
  return next;
};
