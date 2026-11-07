import type { Comment, CommentAuthor, CommentState } from "@mm/sdk";
import { clock, rationalSeconds } from "./model.ts";

/** 一覧の絞り込み。`done` は `wontfix` も含む（人が決め終えたもの）。 */
export type CommentFilter = "all" | "open" | "fresh" | "done";

export const COMMENT_FILTERS: CommentFilter[] = ["all", "open", "fresh", "done"];

export const FILTER_LABELS: Record<CommentFilter, string> = {
  all: "All",
  open: "Open",
  fresh: "Fresh",
  done: "Done",
};

export const STATE_LABELS: Record<CommentState, string> = {
  open: "Open",
  fresh: "Fresh",
  done: "Done",
  wontfix: "Won't fix",
};

/** 先頭に出す順。Claude が対応して人の確認を待つ `fresh` が一番上。 */
const STATE_RANK: Record<CommentState, number> = { fresh: 0, open: 1, done: 2, wontfix: 2 };

/** コメントが指す時刻（秒）。範囲のコメントは始め。どちらも無ければ 0。 */
export const commentSeconds = (c: Comment): number => {
  if (c.at) return rationalSeconds(c.at);
  return c.range ? rationalSeconds(c.range.in) : 0;
};

/** 時刻は `0:01.50`、範囲は `0:01.50–0:04.00`。 */
export const commentTimeLabel = (c: Comment): string => {
  if (c.at) return clock(rationalSeconds(c.at));
  if (c.range) return `${clock(rationalSeconds(c.range.in))}–${clock(rationalSeconds(c.range.out))}`;
  return clock(0);
};

/** 誰が書いたか。モバイルは端末の名前を出し、デスクトップの人の操作は `You`。 */
export const authorLabel = (a: CommentAuthor): string => {
  switch (a.kind) {
    case "mobile":
      return `${a.name} (mobile)`;
    case "gui":
      return "You";
    default:
      return a.name;
  }
};

const inFilter = (c: Comment, filter: CommentFilter): boolean => {
  if (filter === "all") return true;
  if (filter === "done") return c.state === "done" || c.state === "wontfix";
  return c.state === filter;
};

/** 絞り込んで、`fresh`、`open`、決め済みの順に並べる。同じ状態の中は時刻の順（同じ時刻は書いた順）。 */
export const visibleComments = (comments: Comment[], filter: CommentFilter): Comment[] =>
  comments
    .filter((c) => inFilter(c, filter))
    .sort(
      (a, b) =>
        STATE_RANK[a.state] - STATE_RANK[b.state] ||
        commentSeconds(a) - commentSeconds(b) ||
        a.created_ms - b.created_ms,
    );

/** 絞り込みごとの件数。 */
export const filterCounts = (comments: Comment[]): Record<CommentFilter, number> => ({
  all: comments.length,
  open: comments.filter((c) => inFilter(c, "open")).length,
  fresh: comments.filter((c) => inFilter(c, "fresh")).length,
  done: comments.filter((c) => inFilter(c, "done")).length,
});

/** 描き込みがあることを示す文字。無ければ `null`。 */
export const drawingLabel = (c: Comment): string | null => {
  const n = c.drawing.length;
  return n === 0 ? null : `Drawing · ${n} ${n === 1 ? "mark" : "marks"}`;
};
