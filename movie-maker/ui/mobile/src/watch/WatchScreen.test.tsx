// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { demoComments } from "../../demo/fixtures.ts";
import { createHarness } from "../testing/harness.ts";
import type { Attach } from "./usePlayback.ts";
import { WatchScreen } from "./WatchScreen.tsx";

const attach: Attach = async () => () => undefined;
let h = createHarness();
afterEach(() => h.unmount());

const open = async (init: Parameters<typeof createHarness>[0] = { comments: demoComments() }) => {
  h = createHarness(init);
  await h.mount(<WatchScreen controller={h.controller} attach={attach} />);
};

describe("Watch の一覧", () => {
  it("Open と Fresh を出し、Fresh を先頭にする。Done は絞り込みを押すまで隠す", async () => {
    await open();
    expect(h.texts(".item .state")).toEqual(["fresh", "open", "open"]);
    await h.press("Done");
    expect(h.texts(".item .state")).toEqual(["fresh", "done", "open", "open"]);
  });

  it("SSE の comment で状態が変わると一覧に反映する", async () => {
    await open();
    const k2 = h.server.comments.find((c) => c.id === "k2");
    if (k2) k2.state = "done";
    h.server.emit("comment", { id: "k2", state: "done" });
    await h.settle();
    expect(h.texts(".item .state")).toEqual(["fresh", "open"]);
  });

  it("SSE の comment で、状態の変わらない返信もそのコメントを読み直して出す", async () => {
    await open();
    const k3 = h.server.comments.find((c) => c.id === "k3");
    k3?.replies.push({ id: "r9", author: { kind: "mcp", name: "claude" }, text: "Done in proposal 3.", created_ms: 1 });
    h.server.emit("comment", { id: "k3", state: k3?.state });
    await h.settle();
    await h.press("0:05");
    expect(h.host.querySelector(".thread")?.textContent).toContain("Done in proposal 3.");
  });
});

describe("スレッド", () => {
  it("開くと返信が見え、返信と状態を PC に送る", async () => {
    await open();
    await h.press("0:05");
    expect(h.host.querySelector(".thread")?.textContent).toContain("Proposal 2 stretches the zoom");
    expect(h.host.querySelectorAll(".draw .shape")).toHaveLength(2);
    await h.type('input[aria-label="Reply"]', "Thanks");
    await h.submit(".reply");
    expect(h.server.comments.find((c) => c.id === "k3")?.replies.at(-1)?.text).toBe("Thanks");
    await h.press("Done");
    expect(h.server.comments.find((c) => c.id === "k3")?.state).toBe("done");
    expect(h.host.textContent).toContain("Reopen");
    await h.press("Reopen");
    expect(h.server.comments.find((c) => c.id === "k3")?.state).toBe("open");
  });

  it("Fresh は人から決める選択肢に出さない", async () => {
    await open();
    await h.press("0:05");
    expect(h.texts(".actions button")).toEqual(["Done", "Won't fix"]);
  });
});

describe("コメントを書く", () => {
  it("止めたフレームに文字と範囲を付けて送る。送信待ちのキューを通り、client_id が付く", async () => {
    await open({ comments: [] });
    await h.press("Comment");
    await h.type("textarea", "Cut here");
    await h.press("Range");
    await h.press("End here");
    await h.submit("form.compose");
    const sent = h.server.writes.find((w) => w.path === "/m/api/comments")?.body as Record<string, unknown>;
    expect(sent.text).toBe("Cut here");
    expect(sent.comp).toBe("c1");
    expect(typeof sent.client_id).toBe("string");
    expect(h.controller.outbox.list()).toEqual([]);
    expect(h.server.comments).toHaveLength(1);
  });

  it("描き込みだけでは送れず、文字か描き込みが入ると送れる", async () => {
    await open({ comments: [] });
    await h.press("Comment");
    expect(h.host.querySelector<HTMLButtonElement>('button[type="submit"]')?.disabled).toBe(true);
    await h.type("textarea", "x");
    expect(h.host.querySelector<HTMLButtonElement>('button[type="submit"]')?.disabled).toBe(false);
    await h.press("Draw");
    expect(h.host.querySelector(".draw.editing")).not.toBeNull();
  });

  it("Cancel で入力欄を閉じ、下書きを捨てる", async () => {
    await open({ comments: [] });
    await h.press("Comment");
    await h.type("textarea", "x");
    await h.press("Cancel");
    await h.press("Comment");
    expect(h.host.querySelector("textarea")?.value).toBe("");
  });

  it("コンポジションが分かる前は Comment を押せない", async () => {
    await open({ comments: [], online: false });
    expect(h.host.querySelector<HTMLButtonElement>("button.big")?.disabled).toBe(true);
  });
});
