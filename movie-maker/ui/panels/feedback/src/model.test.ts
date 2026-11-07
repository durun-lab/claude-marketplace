import { describe, expect, it } from "vitest";
import replied from "../fixtures/replied.json";
import type { FeedbackItem, ImproveStatus } from "./api.ts";
import { ago, headline, outcomeText, stageOf } from "./model.ts";

const status = (over: Partial<ImproveStatus> = {}): ImproveStatus => ({
  settings: { enabled: true, model: null, effort: null, budgetUsd: 5, wallMinutes: 60, perfThreshold: 0.05 },
  running: false,
  run: null,
  pending: 0,
  waiting: null,
  last: null,
  ...over,
});

const item = (over: Partial<FeedbackItem>): FeedbackItem => ({ ...(replied as FeedbackItem), ...over });

describe("stageOf", () => {
  it("保存された状態と回と最後の返事から 5 つの状態を決める", () => {
    const run = { id: "r1", items: ["a"], startedAt: 0, phase: "agent" as const, limitResetsAt: null };
    const rejected = { at: 1, runId: "r0", text: "落ちた", outcome: { kind: "rejected" as const, reason: "cargo check" } };
    expect(stageOf(item({ status: "done" }), null)).toBe("landed");
    expect(stageOf(item({ status: "held" }), null)).toBe("held");
    expect(stageOf(item({ id: "a", status: "pending", replies: [] }), status({ run }))).toBe("inRun");
    expect(stageOf(item({ id: "b", status: "pending", replies: [rejected] }), status({ run }))).toBe("sentBack");
    expect(stageOf(item({ id: "b", status: "pending", replies: [] }), status({ run }))).toBe("pending");
  });
});

describe("文の組み立て", () => {
  it("結末を 1 行にする", () => {
    expect(outcomeText({ kind: "landed", commit: "3f9c2a1e7b" })).toBe("Landed 3f9c2a1");
    expect(outcomeText({ kind: "pullRequest", url: "u", reason: "hold に触れた" })).toBe("Held at PR: hold に触れた");
    expect(outcomeText({ kind: "rejected", reason: "nextest" })).toBe("Sent back: nextest");
  });

  it("相対時刻", () => {
    expect(ago(0, 30_000)).toBe("just now");
    expect(ago(0, 5 * 60_000)).toBe("5m ago");
    expect(ago(0, 3 * 3_600_000)).toBe("3h ago");
  });

  it("見出しは回、待機、OFF、未対応の順に出す", () => {
    const run = { id: "r1", items: ["a", "b"], startedAt: 0, phase: "gate" as const, limitResetsAt: null };
    expect(headline(status({ run, running: true }), 120_000)).toBe("Run r1: checking on 2 (2m ago)");
    expect(headline(status({ waiting: "検査で落ちた" }), 0)).toBe("Waiting: 検査で落ちた");
    const off = status({ pending: 2 });
    off.settings.enabled = false;
    expect(headline(off, 0)).toBe("Off. 2 pending");
    expect(headline(status(), 0)).toBe("Idle");
  });
});
