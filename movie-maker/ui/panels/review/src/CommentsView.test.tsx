// @vitest-environment jsdom
import type { Comment, CommentEvent } from "@mm/sdk";
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ReviewApi } from "./api.ts";
import { ReviewPanel } from "./ReviewPanel.tsx";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => render(null, host));

const NOW = 1_000_000_000;

const comment = (id: string, over: Partial<Comment> = {}): Comment => ({
  id,
  comp: "main",
  at: { num: 150, den: 100 },
  text: `text ${id}`,
  drawing: [],
  author: { kind: "mobile", name: "Pixel 9" },
  replies: [],
  state: "open",
  created_ms: NOW - 120_000,
  ...over,
});

const fakeApi = (list: Comment[], over: Partial<ReviewApi> = {}): ReviewApi => ({
  list: vi.fn().mockResolvedValue([]),
  approve: vi.fn(),
  reject: vi.fn(),
  reopen: vi.fn(),
  undo: vi.fn(),
  preview: vi.fn().mockResolvedValue(undefined),
  onChanged: vi.fn().mockResolvedValue(() => undefined),
  comments: vi.fn().mockResolvedValue(list),
  replyComment: vi.fn().mockResolvedValue(list[0]),
  setCommentState: vi.fn().mockResolvedValue(list[0]),
  seek: vi.fn().mockResolvedValue(undefined),
  onCommentsChanged: vi.fn().mockResolvedValue(() => undefined),
  ...over,
});

const settle = () => act(() => new Promise<void>((ok) => setTimeout(ok, 0)));

const button = (name: string | RegExp): HTMLButtonElement | undefined =>
  [...host.querySelectorAll("button")].find((b) =>
    typeof name === "string" ? b.textContent === name : name.test(b.textContent ?? ""),
  );

const click = async (el: Element | null | undefined) => {
  act(() => (el as HTMLElement).click());
  await settle();
};

const mount = async (api: ReviewApi) => {
  act(() => render(<ReviewPanel api={api} nowMs={NOW} />, host));
  await settle();
  await click(button(/^Comments/));
};

const ids = (): (string | undefined)[] => [...host.querySelectorAll("[data-comment]")].map((e) => (e as HTMLElement).dataset.comment);

describe("コメントの一覧", () => {
  it("モバイルの端末の名前と時刻を出し、Fresh を先頭に並べる", async () => {
    await mount(fakeApi([comment("a"), comment("b", { state: "fresh", at: { num: 9, den: 1 } })]));
    expect(ids()).toEqual(["b", "a"]);
    expect(host.textContent).toContain("Pixel 9 (mobile) · 2 min ago");
    expect(host.textContent).toContain("0:01.50");
  });

  it("Fresh があるとタブに件数を出す", async () => {
    await mount(fakeApi([comment("a", { state: "fresh" })]));
    expect(button(/^Comments/)?.textContent).toBe("Comments 1 fresh");
  });

  it("絞り込みで Done に決め済みだけを出す", async () => {
    await mount(fakeApi([comment("a"), comment("b", { state: "wontfix" })]));
    await click(button(/^Done/));
    expect(ids()).toEqual(["b"]);
  });

  it("描き込みがあるコメントにその表示を出す", async () => {
    const drawing = [{ kind: "point" as const, points: [[0.5, 0.5]] as [number, number][] }];
    await mount(fakeApi([comment("a", { drawing })]));
    expect(host.querySelector(".mm-review-drawing")?.textContent).toBe("Drawing · 1 mark");
  });

  it("範囲のコメントは範囲を出す", async () => {
    const range = { in: { num: 1, den: 1 }, out: { num: 3, den: 1 } };
    await mount(fakeApi([comment("a", { at: null, range })]));
    expect(host.textContent).toContain("0:01.00–0:03.00");
  });
});

describe("コメントの操作", () => {
  it("時刻を押すと再生位置を動かす（範囲は始め）", async () => {
    const range = { in: { num: 3, den: 2 }, out: { num: 3, den: 1 } };
    const api = fakeApi([comment("a", { at: null, range })]);
    await mount(api);
    await click(host.querySelector(".mm-review-time"));
    expect(api.seek).toHaveBeenCalledWith(1.5);
  });

  it("返信を送ると入力欄を閉じる", async () => {
    const api = fakeApi([comment("a")]);
    await mount(api);
    await click(button("Reply"));
    const box = host.querySelector("textarea") as HTMLTextAreaElement;
    expect(button("Send")?.disabled).toBe(true);
    act(() => {
      box.value = " thanks ";
      box.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await click(button("Send"));
    expect(api.replyComment).toHaveBeenCalledWith("a", "thanks");
    expect(host.querySelector("textarea")).toBeNull();
  });

  it("Done、Won't fix、Reopen で状態を決める", async () => {
    const api = fakeApi([comment("a"), comment("b", { state: "done", at: { num: 9, den: 1 } })]);
    await mount(api);
    await click(button("Done"));
    await click(button("Won't fix"));
    await click(button("Reopen"));
    expect(api.setCommentState).toHaveBeenNthCalledWith(1, "a", "done");
    expect(api.setCommentState).toHaveBeenNthCalledWith(2, "a", "wontfix");
    expect(api.setCommentState).toHaveBeenNthCalledWith(3, "b", "open");
  });

  it("失敗の理由を出す", async () => {
    const api = fakeApi([comment("a")], { setCommentState: vi.fn().mockRejectedValue(new Error("not found")) });
    await mount(api);
    await click(button("Done"));
    expect(host.querySelector(".mm-review-error")?.textContent).toBe("not found");
  });
});

describe("コメントの通知", () => {
  it("通知で読み直し、外れたら購読を外す", async () => {
    let fire: (e: CommentEvent) => void = () => undefined;
    const off = vi.fn();
    const api = fakeApi([comment("a")], {
      onCommentsChanged: vi.fn(async (h: (e: CommentEvent) => void) => {
        fire = h;
        return off;
      }),
    });
    await mount(api);
    expect(api.comments).toHaveBeenCalledTimes(1);
    await act(async () => fire({ id: "a", state: "fresh" }));
    expect(api.comments).toHaveBeenCalledTimes(2);
    act(() => render(null, host));
    expect(off).toHaveBeenCalled();
  });
});
