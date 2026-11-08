// @vitest-environment jsdom
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { demoProposals } from "../../demo/fixtures.ts";
import { createHarness } from "../testing/harness.ts";
import { frameUrl } from "./api.ts";
import { ProposalsScreen } from "./ProposalsScreen.tsx";
import { UNDO_WINDOW_MS } from "./useUndo.ts";

let h = createHarness();
afterEach(() => h.unmount());

const open = async () => {
  h = createHarness({ proposals: demoProposals() });
  await h.mount(<ProposalsScreen controller={h.controller} now={Date.now()} />);
};

const posts = (suffix: string) => h.server.writes.filter((w) => w.path.endsWith(suffix));

describe("Proposals の一覧", () => {
  it("新しい順に並べ、状態と重なりを出す", async () => {
    await open();
    expect(h.texts(".item p")).toEqual(["Tighten the intro", "Slow the zoom on cut 4", "Remove the long pause"]);
    expect(h.texts(".pill")).toEqual(["Conflicts", "Pending", "Approved"]);
  });

  it("SSE の proposal で読み直す", async () => {
    await open();
    h.server.proposals.pop();
    h.server.emit("proposal", { id: 1, state: "approved" });
    await h.settle();
    expect(h.texts(".item p")).toHaveLength(2);
  });
});

describe("提案の 1 件", () => {
  it("前後のフレームは時刻と提案の ID で引き、押すと切り替わる。PC が配らない短い動画のボタンは出さない", async () => {
    await open();
    await h.press("Slow the zoom");
    const imgs = [...h.host.querySelectorAll("img")].map((i) => i.getAttribute("src"));
    expect(imgs).toEqual([frameUrl(4, { comp: "c1" }), frameUrl(4, { comp: "c1", proposal: 2 })]);
    expect(h.host.querySelector("img.show")?.getAttribute("alt")).toBe("After");
    await h.press("Before");
    expect(h.host.querySelector("img.show")?.getAttribute("alt")).toBe("Before");
    expect(h.texts(".compare button")).not.toContain("Play");
    expect(h.host.querySelector(".compare video")).toBeNull();
  });

  it("操作を外すと一部だけ承認し、後の編集と重なる提案は承認できない", async () => {
    await open();
    await h.press("Slow the zoom");
    await act(() => void h.host.querySelectorAll<HTMLInputElement>('.ops input[type="checkbox"]')[1]?.click());
    expect(h.host.textContent).toContain("Approve 2 of 3");
    await h.press("Approve 2 of 3");
    expect(posts("/proposals/2/approve")[0]?.body).toEqual({ ops: [0, 2] });
    await h.press("Tighten the intro");
    expect(h.host.querySelector<HTMLButtonElement>(".dock .primary")?.disabled).toBe(true);
  });

  it("全部のまま承認すると操作の指定を付けない", async () => {
    await open();
    await h.press("Slow the zoom");
    await h.press("Approve");
    expect(posts("/proposals/2/approve")[0]?.body).toEqual({});
  });
});

describe("決めた後の Undo", () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] }));
  afterEach(() => vi.useRealTimers());

  it("承認の直後に Undo が出て、押すと全体の取り消しを呼ぶ（確認の画面は出ない）", async () => {
    await open();
    await h.press("Slow the zoom");
    await h.press("Approve");
    expect(h.host.querySelector(".undo")?.textContent).toContain("Approved Slow the zoom");
    await h.press("Undo");
    expect(posts("/api/undo")).toHaveLength(1);
    expect(h.host.querySelector(".undo")).toBeNull();
    expect(h.server.proposals.find((p) => p.id === 2)?.state).toBe("pending");
  });

  it("却下の Undo は保留に戻す", async () => {
    await open();
    await h.press("Slow the zoom");
    await h.press("Reject");
    await h.press("Undo");
    expect(posts("/proposals/2/reopen")).toHaveLength(1);
    expect(h.server.proposals.find((p) => p.id === 2)?.state).toBe("pending");
  });

  it("5 秒たつと Undo は消える", async () => {
    await open();
    await h.press("Slow the zoom");
    await h.press("Reject");
    await act(() => void vi.advanceTimersByTime(UNDO_WINDOW_MS - 1));
    expect(h.host.querySelector(".undo")).not.toBeNull();
    await act(() => void vi.advanceTimersByTime(1));
    expect(h.host.querySelector(".undo")).toBeNull();
  });
});
