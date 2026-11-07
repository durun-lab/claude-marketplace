import { describe, expect, it } from "vitest";
import type { Topic } from "@mm/sdk";
import type { StoryWord } from "@mm/story/src/api.ts";
import { NO_PICK, moveItem, spanOfTopic, tapWord, titleOf, topicsOver } from "./model.ts";

const word = (i: number): StoryWord => ({
  id: `w${i}`,
  segment: 0,
  start: { num: i, den: 1 },
  end: { num: i + 1, den: 1 },
  text: `t${i}`,
  fixed: null,
  deleted: false,
});
const words = Array.from({ length: 10 }, (_, i) => word(i));
const topic = (id: string, from: number, to: number): Topic => ({
  id,
  asset: "a1",
  title: id,
  in: { num: from, den: 1 },
  out: { num: to, den: 1 },
});

describe("tapWord", () => {
  it("1 回目で始め、別の単語の 2 回目で間を範囲にする", () => {
    const first = tapWord(NO_PICK, 6);
    expect(first).toEqual({ anchor: 6, span: { from: 6, to: 6 } });
    expect(tapWord(first, 2)).toEqual({ anchor: null, span: { from: 2, to: 6 } });
  });

  it("始めの単語をもう一度押すと外す", () => {
    expect(tapWord(tapWord(NO_PICK, 3), 3)).toEqual(NO_PICK);
  });

  it("範囲が決まった後の 1 回目は新しい始めになる", () => {
    const ranged = tapWord(tapWord(NO_PICK, 1), 4);
    expect(tapWord(ranged, 8)).toEqual({ anchor: 8, span: { from: 8, to: 8 } });
  });
});

describe("話題と単語", () => {
  it("話題の範囲に始まりが入る単語の範囲を返し、単語が無ければ null", () => {
    expect(spanOfTopic(words, topic("t", 2, 5))).toEqual({ from: 2, to: 4 });
    expect(spanOfTopic(words, topic("none", 20, 30))).toBeNull();
  });

  it("選んだ範囲に重なる話題だけを返す", () => {
    const topics = [topic("a", 0, 3), topic("b", 3, 6), topic("c", 8, 10)];
    expect(topicsOver(words, topics, { from: 2, to: 3 }).map((t) => t.id)).toEqual(["a", "b"]);
    expect(topicsOver(words, topics, null)).toEqual([]);
  });
});

describe("titleOf", () => {
  const w = (text: string): StoryWord => ({ ...word(0), text });

  it("ASCII の単語の間は空け、日本語は続け、消した単語は含めない", () => {
    expect(titleOf([w("So"), w("today,"), w("we")], 24)).toBe("So today, we");
    expect(titleOf([w("今日"), w("は"), w("晴れ")], 24)).toBe("今日は晴れ");
    expect(titleOf([w("a"), { ...w("b"), deleted: true }, w("c")], 24)).toBe("a c");
  });

  it("最大の文字数で切る", () => {
    expect(titleOf([w("abcdef")], 3)).toBe("abc");
  });
});

describe("moveItem", () => {
  it("要素を指定の位置へ移す", () => {
    expect(moveItem(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"]);
    expect(moveItem(["a", "b", "c", "d"], 3, 1)).toEqual(["a", "d", "b", "c"]);
  });

  it("範囲外は並びを変えない", () => {
    expect(moveItem(["a", "b"], 0, 5)).toEqual(["a", "b"]);
  });
});
