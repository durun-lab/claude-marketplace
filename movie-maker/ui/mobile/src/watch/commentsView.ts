import type { CommentShape } from "@mm/sdk";
import type { CommentDraft, CommentItem, CommentState } from "../api/types.ts";
import { commentStart, fromSeconds } from "./time.ts";

/** 一覧の絞り込み。`done` は `done` と `wontfix` の両方。 */
export type Filter = "open" | "fresh" | "done";
export const FILTERS: readonly Filter[] = ["open", "fresh", "done"];

/** 既定では対応待ちの `open` と、確かめる番の `fresh` を出す。 */
export const DEFAULT_FILTERS: readonly Filter[] = ["open", "fresh"];

const filterOf = (state: CommentState): Filter => (state === "wontfix" ? "done" : state);

/** 絞り込みを通ったコメントを、`fresh` を先頭にして並べる。同じ状態の中は時刻の順。 */
export const visibleComments = (items: CommentItem[], filters: readonly Filter[]): CommentItem[] =>
  items
    .filter((c) => filters.includes(filterOf(c.state)))
    .sort((a, b) => {
      const fresh = Number(b.state === "fresh") - Number(a.state === "fresh");
      return fresh !== 0 ? fresh : commentStart(a) - commentStart(b);
    });

/** 絞り込みのボタンごとの件数。 */
export const countByFilter = (items: CommentItem[]): Record<Filter, number> => {
  const counts: Record<Filter, number> = { open: 0, fresh: 0, done: 0 };
  items.forEach((c) => (counts[filterOf(c.state)] += 1));
  return counts;
};

export interface DraftInput {
  comp: string;
  /** 止めているフレームの時刻（秒）。 */
  time: number;
  text: string;
  /** 範囲の始めと終わり（秒）。終わりがまだなら時刻のコメントにする。 */
  range: { in: number; out: number | null } | null;
  shapes: CommentShape[];
}

/** 送信する本文。文字も描き込みも無ければ null。範囲は始めと終わりを昇順にそろえる。 */
export const buildDraft = (input: DraftInput): Omit<CommentDraft, "client_id"> | null => {
  const text = input.text.trim();
  if (text === "" && input.shapes.length === 0) return null;
  const base = { comp: input.comp, ...(text ? { text } : {}), ...(input.shapes.length ? { drawing: input.shapes } : {}) };
  const range = input.range;
  if (range && range.out !== null && range.out !== range.in) {
    const [from, to] = [Math.min(range.in, range.out), Math.max(range.in, range.out)];
    return { ...base, range: { in: fromSeconds(from), out: fromSeconds(to) } };
  }
  return { ...base, at: fromSeconds(range ? range.in : input.time) };
};

/** 返信と状態の操作で選べる状態。`fresh` は人からは決められない。 */
export const nextStates = (state: CommentState): { label: string; to: Exclude<CommentState, "fresh"> }[] =>
  state === "done" || state === "wontfix"
    ? [{ label: "Reopen", to: "open" }]
    : [
        { label: "Done", to: "done" },
        { label: "Won't fix", to: "wontfix" },
      ];

/**
 * SSE の `comment`（`{ id, state }`）のコメントの ID。読めなければ null。
 * 返信でも同じ通知が来て `state` は変わらないので、中身は通知から作らず 1 件を読み直す。
 */
export const commentEventId = (event: unknown): string | null => {
  const id = (event as { id?: unknown } | null)?.id;
  return typeof id === "string" && id !== "" ? id : null;
};

/** 読み直したコメントを一覧に入れる。知っている ID は置き換え、新しいものは末尾（書いた順）に足す。 */
export const upsertComment = (items: CommentItem[], next: CommentItem): CommentItem[] =>
  items.some((c) => c.id === next.id) ? items.map((c) => (c.id === next.id ? next : c)) : [...items, next];
