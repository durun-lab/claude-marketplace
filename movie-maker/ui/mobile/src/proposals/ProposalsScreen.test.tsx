// @vitest-environment jsdom
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { demoProposals } from "../../demo/fixtures.ts";
import { createHarness } from "../testing/harness.ts";
import "../testing/stubMedia.ts";
import { frameUrl } from "./api.ts";
import { ProposalsScreen } from "./ProposalsScreen.tsx";
import { UNDO_WINDOW_MS } from "./useUndo.ts";

let h = createHarness();
afterEach(() => h.unmount());

const open = async () => {
  h = createHarness({ proposals: demoProposals() });
  await h.mount(<ProposalsScreen controller={h.controller} now={Date.now()} />);
};

/** 提案 2 の全部の操作の短い動画の URL（キーは範囲に掛かるもののハッシュ）。 */
const CLIP_ALL = /^\/m\/api\/proposals\/2\/clip\/[0-9a-f]{16}\.mp4\?comp=c1$/;

/** 操作の選択を外し、短い動画の URL を読み直し終えるまで待つ。 */
const uncheck = async (index: number): Promise<void> => {
  await act(() => void h.host.querySelectorAll<HTMLInputElement>('.ops input[type="checkbox"]')[index]?.click());
  await h.settle();
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
  it("前後のフレームは時刻と提案の ID で引き、押すと切り替わる", async () => {
    await open();
    await h.press("Slow the zoom");
    const imgs = [...h.host.querySelectorAll("img")].map((i) => i.getAttribute("src"));
    expect(imgs).toEqual([frameUrl(4, { comp: "c1" }), frameUrl(4, { comp: "c1", proposal: 2 })]);
    expect(h.host.querySelector("img.show")?.getAttribute("alt")).toBe("After");
    await h.press("Before");
    expect(h.host.querySelector("img.show")?.getAttribute("alt")).toBe("Before");
  });
});

describe("提案の区間の短い動画", () => {
  it("Play は押した操作の中で短い動画を再生し、映像が出るまで作っている間の表示を出す", async () => {
    await open();
    await h.press("Slow the zoom");
    const play = vi.spyOn(HTMLMediaElement.prototype, "play");
    await h.press("Play");
    const video = h.host.querySelector<HTMLVideoElement>(".compare video");
    expect(video?.getAttribute("src")).toMatch(CLIP_ALL);
    expect(play).toHaveBeenCalledTimes(1);
    expect(h.host.querySelector(".clip-note")?.textContent).toBe("Rendering clip…");
    expect(video?.classList.contains("on")).toBe(false);
    await act(() => void video?.dispatchEvent(new Event("playing")));
    expect(h.host.querySelector(".clip-note")).toBeNull();
    expect(video?.classList.contains("on")).toBe(true);
    await h.press("After");
    expect(video?.getAttribute("src")).toBeNull();
    expect(h.host.querySelector("img.show")?.getAttribute("alt")).toBe("After");
    play.mockRestore();
  });

  it("一部の操作だけを選ぶとその操作の短い動画にし、読めなければ失敗を表示する", async () => {
    await open();
    await h.press("Slow the zoom");
    await uncheck(1);
    await h.press("Play");
    const video = h.host.querySelector<HTMLVideoElement>(".compare video");
    expect(video?.getAttribute("src")).toMatch(/^\/m\/api\/proposals\/2\/clip\/[0-9a-f]{16}\.mp4\?comp=c1&ops=0,2$/);
    await act(() => void video?.dispatchEvent(new Event("error")));
    expect(h.host.querySelector(".clip-note")?.textContent).toBe("Could not play the clip.");
  });

  it("再生中に選ぶ操作を変えると、前の操作の短い動画を止める", async () => {
    await open();
    await h.press("Slow the zoom");
    await h.press("Play");
    const video = h.host.querySelector<HTMLVideoElement>(".compare video");
    expect(video?.getAttribute("src")).toMatch(CLIP_ALL);
    await uncheck(1);
    expect(video?.getAttribute("src")).toBeNull();
    expect(h.host.querySelector(".clip-note")).toBeNull();
    expect(h.host.querySelector("img.show")?.getAttribute("alt")).toBe("After");
  });
});

describe("短い動画の URL", () => {
  it("URL は範囲に掛かるもののハッシュを含み、編集で読み直して変われば前の動画を止め、次の Play は新しい URL にする", async () => {
    await open();
    await h.press("Slow the zoom");
    await h.press("Play");
    const video = h.host.querySelector<HTMLVideoElement>(".compare video");
    const before = video?.getAttribute("src");
    expect(before).toMatch(CLIP_ALL);
    h.server.project.version += 1;
    h.server.emit("change", { version: h.server.project.version });
    await h.settle();
    await h.settle();
    expect(video?.getAttribute("src")).toBeNull();
    await h.press("Play");
    const after = video?.getAttribute("src");
    expect(after).toMatch(CLIP_ALL);
    expect(after).not.toBe(before);
  });

  it("URL を読む前は Play を押せない", async () => {
    h = createHarness({ proposals: demoProposals() });
    await h.mount(<ProposalsScreen controller={h.controller} now={Date.now()} />);
    const proposal = h.host.querySelectorAll<HTMLElement>(".item")[1];
    await act(() => void proposal?.click());
    const play = [...h.host.querySelectorAll<HTMLButtonElement>(".compare button")].find((b) => b.textContent === "Play");
    expect(play?.disabled).toBe(true);
    await h.settle();
    expect(play?.disabled).toBe(false);
  });

  it("決めた提案には Play を出さない", async () => {
    await open();
    await h.press("Remove the long pause");
    expect(h.texts(".compare button")).not.toContain("Play");
    expect(h.host.querySelector(".compare video")).toBeNull();
  });
});

describe("提案の決定", () => {
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
