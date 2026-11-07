import { describe, expect, it } from "vitest";
import type { CommentItem } from "../api/types.ts";
import { buildDraft, commentEventId, countByFilter, DEFAULT_FILTERS, nextStates, upsertComment, visibleComments } from "./commentsView.ts";
import { composeReducer, emptyCompose } from "./compose.ts";
import { chooseEngine } from "./hls.ts";
import { fractionOf, markFractions, timeAt } from "./scrub.ts";
import { draftShape, normalizePoint, rectShape, strokeShape } from "./shapes.ts";
import { commentWhen, fromSeconds, mmss } from "./time.ts";

const comment = (id: string, state: CommentItem["state"], sec: number): CommentItem => ({
  id,
  comp: "c1",
  at: fromSeconds(sec),
  text: id,
  drawing: [],
  author: { kind: "mobile", name: "iPhone" },
  replies: [],
  state,
  created_ms: 0,
});

describe("コメントの一覧", () => {
  const items = [comment("a", "open", 9), comment("b", "fresh", 20), comment("c", "open", 3), comment("d", "wontfix", 1)];

  it("既定は Open と Fresh を出し、Fresh を先頭に、同じ状態の中は時刻の順に並べる", () => {
    expect(visibleComments(items, DEFAULT_FILTERS).map((c) => c.id)).toEqual(["b", "c", "a"]);
  });

  it("Done の絞り込みは done と wontfix の両方を含む", () => {
    expect(visibleComments(items, ["done"]).map((c) => c.id)).toEqual(["d"]);
    expect(countByFilter(items)).toEqual({ open: 2, fresh: 1, done: 1 });
  });

  it("状態の操作は、済みなら Reopen だけ、そうでなければ Done と Won't fix（fresh には決められない）", () => {
    expect(nextStates("done").map((s) => s.to)).toEqual(["open"]);
    expect(nextStates("fresh").map((s) => s.to)).toEqual(["done", "wontfix"]);
  });

  it("SSE の comment は ID だけを読み、読み直したコメントで置き換えるか末尾に足す", () => {
    expect(commentEventId({ id: "a", state: "done" })).toBe("a");
    expect(commentEventId(null)).toBeNull();
    expect(commentEventId({ id: 3 })).toBeNull();
    const next = { ...comment("a", "done", 1), text: "edited" };
    expect(upsertComment(items, next).find((c) => c.id === "a")?.text).toBe("edited");
    expect(upsertComment(items, comment("zz", "open", 0)).at(-1)?.id).toBe("zz");
  });

  it("時刻と範囲の表記", () => {
    expect(commentWhen(comment("a", "open", 65))).toBe("1:05");
    expect(commentWhen({ ...comment("a", "open", 0), at: null, range: { in: fromSeconds(8), out: fromSeconds(11) } })).toBe("0:08–0:11");
    expect(mmss(-3)).toBe("0:00");
  });
});

describe("送信する本文", () => {
  const base = { comp: "c1", time: 4.5, text: " hi ", range: null, shapes: [] };

  it("文字も描き込みも無ければ作らない", () => {
    expect(buildDraft({ ...base, text: "  " })).toBeNull();
  });

  it("描き込みだけでも送れる。時刻は止めているフレーム", () => {
    const shapes = [rectShape([0.1, 0.1], [0.5, 0.5])];
    expect(buildDraft({ ...base, text: "", shapes })).toEqual({ comp: "c1", drawing: shapes, at: { num: 4500, den: 1000 } });
  });

  it("範囲は昇順にそろえ、終わりが無ければ始めの時刻のコメントにする", () => {
    expect(buildDraft({ ...base, range: { in: 9, out: 3 } })).toEqual({
      comp: "c1",
      text: "hi",
      range: { in: { num: 3000, den: 1000 }, out: { num: 9000, den: 1000 } },
    });
    expect(buildDraft({ ...base, range: { in: 2, out: null } })).toEqual({ comp: "c1", text: "hi", at: { num: 2000, den: 1000 } });
  });
});

describe("書く途中の状態", () => {
  it("Range は 1 回目で始め、2 回目で終わり、3 回目でやり直す", () => {
    const a = composeReducer(emptyCompose(), { type: "range", time: 2 });
    const b = composeReducer(a, { type: "range", time: 5 });
    expect(b.range).toEqual({ in: 2, out: 5 });
    expect(composeReducer(b, { type: "range", time: 7 }).range).toEqual({ in: 7, out: null });
  });

  it("描き込みは追加、1 つ戻す、全部消す", () => {
    const s1 = composeReducer(emptyCompose(), { type: "shape", shape: rectShape([0, 0], [1, 1]) });
    const s2 = composeReducer(s1, { type: "shape", shape: strokeShape([[0, 0], [0.5, 0.5]]) });
    expect(s2.shapes).toHaveLength(2);
    expect(composeReducer(s2, { type: "undoShape" }).shapes).toHaveLength(1);
    expect(composeReducer(s2, { type: "clearShapes" }).shapes).toEqual([]);
  });
});

describe("描き込みの座標", () => {
  const box = { left: 10, top: 20, width: 200, height: 100 };

  it("面の左上を (0,0)、右下を (1,1) にし、はみ出しは縁に寄せる", () => {
    expect(normalizePoint(110, 70, box)).toEqual([0.5, 0.5]);
    expect(normalizePoint(-50, 500, box)).toEqual([0, 1]);
    expect(normalizePoint(1, 1, { left: 0, top: 0, width: 0, height: 0 })).toEqual([0, 0]);
  });

  it("矩形は左上と右下の順にそろう", () => {
    expect(rectShape([0.8, 0.9], [0.2, 0.1]).points).toEqual([[0.2, 0.1], [0.8, 0.9]]);
  });

  it("線は近すぎる点を間引き、点 1 つなら point にする", () => {
    expect(strokeShape([[0.1, 0.1], [0.1001, 0.1], [0.5, 0.5]]).points).toEqual([[0.1, 0.1], [0.5, 0.5]]);
    expect(strokeShape([[0.3, 0.3]])).toEqual({ kind: "point", points: [[0.3, 0.3]] });
    expect(draftShape("rect", [])).toBeNull();
  });
});

describe("スクラブの棒", () => {
  it("押した位置から時刻を求め、範囲の外は端に寄せる", () => {
    expect(timeAt(150, { left: 50, width: 200 }, 12)).toBe(6);
    expect(timeAt(900, { left: 50, width: 200 }, 12)).toBe(12);
    expect(timeAt(1, { left: 0, width: 0 }, 12)).toBe(0);
  });

  it("目盛りは尺の中だけを 0〜1 にする", () => {
    expect(markFractions([0, 4, 8, 13], 12)).toEqual([0, 4 / 12, 8 / 12]);
    expect(markFractions([1], 0)).toEqual([]);
    expect(fractionOf(3, 12)).toBe(0.25);
  });
});

describe("再生の方法", () => {
  it("iOS の Safari はネイティブ、Android の Chrome は hls.js、どちらも無ければ none", () => {
    expect(chooseEngine({ ios: true, nativeHls: true, mse: false })).toBe("native");
    expect(chooseEngine({ ios: false, nativeHls: true, mse: true })).toBe("hlsjs");
    expect(chooseEngine({ ios: false, nativeHls: false, mse: true })).toBe("hlsjs");
    expect(chooseEngine({ ios: false, nativeHls: false, mse: false })).toBe("none");
  });
});
