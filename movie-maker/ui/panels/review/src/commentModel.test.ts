import type { Comment } from "@mm/sdk";
import { describe, expect, it } from "vitest";
import {
  authorLabel,
  commentSeconds,
  commentTimeLabel,
  drawingLabel,
  filterCounts,
  visibleComments,
} from "./commentModel.ts";

const comment = (id: string, over: Partial<Comment> = {}): Comment => ({
  id,
  comp: "main",
  at: { num: 150, den: 100 },
  text: id,
  drawing: [],
  author: { kind: "gui", name: "app" },
  replies: [],
  state: "open",
  created_ms: 0,
  ...over,
});

describe("コメントの時刻", () => {
  it("時刻は m:ss.cc、範囲は始めと終わり", () => {
    expect(commentTimeLabel(comment("a"))).toBe("0:01.50");
    const range = comment("b", { at: null, range: { in: { num: 150, den: 100 }, out: { num: 4, den: 1 } } });
    expect(commentTimeLabel(range)).toBe("0:01.50–0:04.00");
  });

  it("範囲のコメントは始めへ動く", () => {
    const range = comment("b", { at: null, range: { in: { num: 3, den: 2 }, out: { num: 4, den: 1 } } });
    expect(commentSeconds(range)).toBe(1.5);
  });
});

describe("author", () => {
  it("モバイルは端末の名前、デスクトップの人は You", () => {
    expect(authorLabel({ kind: "mobile", name: "iPhone" })).toBe("iPhone (mobile)");
    expect(authorLabel({ kind: "gui", name: "app" })).toBe("You");
    expect(authorLabel({ kind: "mcp", name: "claude" })).toBe("claude");
  });
});

describe("一覧", () => {
  const list = [
    comment("done", { state: "done", at: { num: 1, den: 1 } }),
    comment("open-late", { at: { num: 9, den: 1 } }),
    comment("fresh", { state: "fresh", at: { num: 20, den: 1 } }),
    comment("open-early", { at: { num: 2, den: 1 } }),
    comment("wontfix", { state: "wontfix", at: { num: 0, den: 1 } }),
  ];

  it("Fresh を先頭に、次に Open、決め済みの順。同じ状態では時刻の順", () => {
    expect(visibleComments(list, "all").map((c) => c.id)).toEqual([
      "fresh",
      "open-early",
      "open-late",
      "wontfix",
      "done",
    ]);
  });

  it("Done は Won't fix も含む", () => {
    expect(visibleComments(list, "done").map((c) => c.id)).toEqual(["wontfix", "done"]);
    expect(visibleComments(list, "fresh").map((c) => c.id)).toEqual(["fresh"]);
  });

  it("件数を絞り込みごとに数える", () => {
    expect(filterCounts(list)).toEqual({ all: 5, open: 2, fresh: 1, done: 2 });
  });
});

describe("描き込みの印", () => {
  it("無ければ null、あれば数を出す", () => {
    expect(drawingLabel(comment("a"))).toBeNull();
    const shape = { kind: "point" as const, points: [[0.5, 0.5]] as [number, number][] };
    expect(drawingLabel(comment("a", { drawing: [shape] }))).toBe("Drawing · 1 mark");
    expect(drawingLabel(comment("a", { drawing: [shape, shape] }))).toBe("Drawing · 2 marks");
  });
});
