// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import type { CommandsApi, Layer, UiCommand } from "@mm/sdk";
import type { DeliverApi } from "../../panels/deliver/src/api.ts";
import type { FeedbackApi } from "../../panels/feedback/src/api.ts";
import type { MediaApi } from "../../panels/media/src/api.ts";
import type { MixerApi } from "../../panels/mixer/src/api.ts";
import type { TransitionApi } from "../../panels/transition/src/api.ts";
import type { ReviewApi } from "../../panels/review/src/api.ts";
import type { StoryApi } from "../../panels/story/src/api.ts";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LAYER_SELECTED_EVENT, LAYER_SELECTION_REQUEST_EVENT, Shell } from "./Shell.tsx";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => render(null, host));

const t0 = { num: 0, den: 1 };
const layer = {
  id: "l1",
  start: t0,
  in: t0,
  out: t0,
  opacity: { value: { type: "number", value: 1 } },
  source: { kind: "solid", version: 1 },
} as Layer;

const fakeMedia = (over: Partial<MediaApi> = {}): MediaApi => ({
  list: vi.fn().mockResolvedValue([]),
  thumbnail: vi.fn().mockResolvedValue(new ArrayBuffer(0)),
  waveform: vi.fn().mockResolvedValue({ duration: 0, peaks: [] }),
  pickFiles: vi.fn().mockResolvedValue([]),
  importFiles: vi.fn().mockResolvedValue(undefined),
  cacheInfo: vi.fn().mockResolvedValue({ total_bytes: 0, folders: [] }),
  clearCache: vi.fn().mockResolvedValue(0),
  onProgress: vi.fn().mockResolvedValue(() => {}),
  onDrop: vi.fn().mockResolvedValue(() => {}),
  ...over,
});

const fakeStory = (): StoryApi => ({
  overview: vi.fn().mockResolvedValue({ assets: [], sequences: [] }),
  transcript: vi.fn().mockResolvedValue({ asset: "", words: [], topics: [], selections: [] }),
  range: vi.fn(),
  search: vi.fn().mockResolvedValue([]),
  timelineTime: vi.fn().mockResolvedValue(null),
  cutTime: vi.fn().mockResolvedValue(null),
  apply: vi.fn().mockResolvedValue({ ok: true, value: {} }),
  arrange: vi.fn().mockResolvedValue({ ok: true, value: {} }),
  seek: vi.fn(),
});

const fakeDeliver = (): DeliverApi => ({
  overview: vi.fn().mockResolvedValue({ targets: [], compositions: [], sequences: [], open_composition: null, can_verify: false }),
  status: vi.fn().mockResolvedValue({ running: false, items: [] }),
  applyOps: vi.fn().mockResolvedValue(undefined),
  start: vi.fn().mockResolvedValue(undefined),
  cancel: vi.fn().mockResolvedValue(undefined),
  reveal: vi.fn().mockResolvedValue(undefined),
  pickOutput: vi.fn().mockResolvedValue(null),
  onProgress: vi.fn().mockResolvedValue(() => {}),
});

const fakeMixer = (): MixerApi => ({
  state: vi.fn().mockResolvedValue({ composition: "main", mixer: null, tracks: [], roles: ["voice", "bgm", "sfx"] }),
  applyOps: vi.fn().mockResolvedValue(undefined),
  onMeters: vi.fn().mockResolvedValue(() => {}),
});

const fakeReview = (): ReviewApi => ({
  list: vi.fn().mockResolvedValue([]),
  approve: vi.fn(),
  reject: vi.fn(),
  reopen: vi.fn(),
  undo: vi.fn(),
  preview: vi.fn().mockResolvedValue(undefined),
  onChanged: vi.fn().mockResolvedValue(() => {}),
  comments: vi.fn().mockResolvedValue([]),
  replyComment: vi.fn(),
  setCommentState: vi.fn(),
  focusComment: vi.fn().mockResolvedValue(undefined),
  seek: vi.fn().mockResolvedValue(undefined),
  onCommentsChanged: vi.fn().mockResolvedValue(() => {}),
});

const fakeFeedback = (): FeedbackApi => ({
  status: vi.fn().mockResolvedValue({
    settings: { enabled: false, model: null, effort: null, budgetUsd: 5, wallMinutes: 60, perfThreshold: 0.05 },
    running: false,
    run: null,
    pending: 0,
    waiting: null,
    last: null,
  }),
  list: vi.fn().mockResolvedValue([]),
  get: vi.fn(),
  submit: vi.fn(),
  settings: vi.fn(),
  stop: vi.fn(),
  capture: vi.fn(),
  reportError: vi.fn().mockResolvedValue(undefined),
  onChanged: vi.fn().mockResolvedValue(() => {}),
});

const click = (el: Element | null | undefined) =>
  act(() => {
    (el as HTMLElement).click();
  });

const command = (id: string, over: Partial<UiCommand> = {}): UiCommand => ({
  id,
  name: id,
  keys: [],
  key_label: null,
  description: null,
  menu: null,
  panel: null,
  enabled: true,
  checked: null,
  ...over,
});

/** アプリの UI のコマンドの表の代わり。`push` で押せるかが変わったイベントを、`fromApp` でメニューやキーから実行したパネルのコマンドを届ける。 */
const fakeCommands = (table: UiCommand[]) => {
  let push: (next: UiCommand[]) => void = () => undefined;
  let fromApp: (id: string) => void = () => undefined;
  const api: CommandsApi = {
    table: vi.fn().mockResolvedValue(table),
    run: vi.fn().mockResolvedValue(null),
    onChanged: vi.fn(async (handler: (next: UiCommand[]) => void) => {
      push = handler;
      return () => undefined;
    }),
    onPanelCommand: vi.fn(async (handler: (id: string) => void) => {
      fromApp = handler;
      return () => undefined;
    }),
  };
  return { api, push: (next: UiCommand[]) => push(next), fromApp: (id: string) => fromApp(id) };
};

const TABLE = [
  command("clip.split", { name: "Split Clip", key_label: "B" }),
  command("export.start", { name: "Export…", panel: "deliver" }),
  command("media.import", { name: "Import Media…", panel: "media" }),
];

const settle = () => act(() => new Promise<void>((ok) => setTimeout(ok, 0)));

const taskLink = (label: string) => [...host.querySelectorAll(".mm-tasks-link")].find((b) => b.textContent === label);

describe("Shell の左の列", () => {
  it("Media を渡さないときはタブを出さず Tasks だけ", () => {
    act(() => render(<Shell sendOp={vi.fn()} />, host));
    expect(host.querySelector(".mm-shell-tabs")).toBeNull();
    expect(host.querySelector(".mm-tasks")).not.toBeNull();
  });

  it("タブで Tasks と Media を切り替え、どちらも作ったまま残す", () => {
    act(() => render(<Shell sendOp={vi.fn()} apis={{ media: fakeMedia() }} />, host));
    const panels = host.querySelectorAll<HTMLElement>(".mm-shell-left-body");
    expect([panels[0]?.hidden, panels[1]?.hidden]).toEqual([false, true]);
    click(host.querySelectorAll(".mm-shell-tab")[1]);
    expect([panels[0]?.hidden, panels[1]?.hidden]).toEqual([true, false]);
    expect(host.querySelector(".mm-media")).not.toBeNull();
  });

  it("Story を渡すと Story のタブが出て、開くたびに読み直す", async () => {
    const story = fakeStory();
    act(() => render(<Shell sendOp={vi.fn()} apis={{ media: fakeMedia(), story }} />, host));
    const tabs = host.querySelectorAll(".mm-shell-tab");
    expect([...tabs].map((t) => t.textContent)).toEqual(["Tasks", "Media", "Story"]);
    const panels = host.querySelectorAll<HTMLElement>(".mm-shell-left-body");
    expect(panels[2]?.hidden).toBe(true);
    click(tabs[2]);
    await act(() => new Promise<void>((ok) => setTimeout(ok, 0)));
    expect(panels[2]?.hidden).toBe(false);
    expect(host.querySelector(".mm-story")).not.toBeNull();
    expect(story.overview).toHaveBeenCalledTimes(2);
  });

  it("Deliver を渡すとタブが出て、Tasks の書き出しの項目から開ける", async () => {
    const { api } = fakeCommands(TABLE);
    act(() => render(<Shell sendOp={vi.fn()} apis={{ deliver: fakeDeliver() }} commands={api} />, host));
    await settle();
    const tabs = host.querySelectorAll(".mm-shell-tab");
    expect([...tabs].map((t) => t.textContent)).toEqual(["Tasks", "Deliver"]);
    const panels = host.querySelectorAll<HTMLElement>(".mm-shell-left-body");
    expect(panels[1]?.hidden).toBe(true);
    click(taskLink("Export to a file"));
    await settle();
    expect(panels[1]?.hidden).toBe(false);
    expect(host.querySelector(".mm-deliver")).not.toBeNull();
    expect(api.run).not.toHaveBeenCalled();
  });
});

describe("Shell の Mixer タブ", () => {
  it("Mixer を渡すと Mixer のタブが出て、開くと Mixer のパネルを見せる", async () => {
    const mixer = fakeMixer();
    act(() => render(<Shell sendOp={vi.fn()} apis={{ mixer }} />, host));
    const tabs = host.querySelectorAll(".mm-shell-tab");
    expect([...tabs].map((t) => t.textContent)).toEqual(["Tasks", "Mixer"]);
    const panels = host.querySelectorAll<HTMLElement>(".mm-shell-left-body");
    click(tabs[1]);
    await act(() => new Promise<void>((ok) => setTimeout(ok, 0)));
    expect(panels[1]?.hidden).toBe(false);
    expect(host.querySelector(".mm-mixer")).not.toBeNull();
    expect(mixer.state).toHaveBeenCalled();
  });
});

describe("Shell の Transition タブ", () => {
  it("Transition を渡すとタブが出て、開くとカットを読む", async () => {
    const transition: TransitionApi = {
      cuts: vi.fn().mockResolvedValue({ sequence: null, cuts: [] }),
      kinds: vi.fn().mockResolvedValue([]),
      set: vi.fn(),
      remove: vi.fn(),
    };
    act(() => render(<Shell sendOp={vi.fn()} apis={{ transition }} />, host));
    const tabs = host.querySelectorAll(".mm-shell-tab");
    expect([...tabs].map((t) => t.textContent)).toEqual(["Tasks", "Transitions"]);
    click(tabs[1]);
    await act(() => new Promise<void>((ok) => setTimeout(ok, 0)));
    expect(host.querySelector(".mm-tr")).not.toBeNull();
    expect(transition.cuts).toHaveBeenCalled();
  });
});

describe("Shell の右の列", () => {
  it("Review を渡さないときはタブを出さず Inspector だけ", () => {
    act(() => render(<Shell sendOp={vi.fn()} />, host));
    expect(host.querySelector(".mm-shell-right-tabs")).toBeNull();
    expect(host.querySelector(".mm-review")).toBeNull();
  });

  it("Review を渡すと Inspector と Review のタブで切り替え、どちらも作ったまま残す", async () => {
    const review = fakeReview();
    act(() => render(<Shell sendOp={vi.fn()} apis={{ review }} />, host));
    const tabs = host.querySelectorAll(".mm-shell-right-tabs .mm-shell-tab");
    expect([...tabs].map((t) => t.textContent)).toEqual(["Inspector", "Review"]);
    const panels = host.querySelectorAll<HTMLElement>(".mm-shell-right-body");
    expect([panels[0]?.hidden, panels[1]?.hidden]).toEqual([false, true]);
    click(tabs[1]);
    await act(() => new Promise<void>((ok) => setTimeout(ok, 0)));
    expect([panels[0]?.hidden, panels[1]?.hidden]).toEqual([true, false]);
    expect(host.querySelector(".mm-review")).not.toBeNull();
    expect(review.list).toHaveBeenCalled();
  });

  it("Feedback を渡すと右の列の最後にタブが出て、開くと状態と一覧を読む", async () => {
    const feedback = fakeFeedback();
    act(() => render(<Shell sendOp={vi.fn()} apis={{ feedback }} />, host));
    const tabs = host.querySelectorAll(".mm-shell-right-tabs .mm-shell-tab");
    expect([...tabs].map((t) => t.textContent)).toEqual(["Inspector", "Feedback"]);
    click(tabs[1]);
    await act(() => new Promise<void>((ok) => setTimeout(ok, 0)));
    expect(host.querySelector(".mm-feedback")).not.toBeNull();
    expect(feedback.status).toHaveBeenCalled();
    expect(feedback.list).toHaveBeenCalledWith("all");
  });
});

describe("Shell の外からの変更", () => {
  it("mm:doc-changed が届くと Story と Media が読み直す", async () => {
    let fire: (e: { payload: unknown }) => void = () => undefined;
    const off = vi.fn();
    const listen = vi.fn(async (_event: string, handler: typeof fire) => {
      fire = handler;
      return off;
    });
    const story = fakeStory();
    const media = fakeMedia();
    act(() => render(<Shell sendOp={vi.fn()} apis={{ media, story }} listen={listen} />, host));
    await act(() => new Promise<void>((ok) => setTimeout(ok, 0)));
    expect(listen.mock.calls[0]?.[0]).toBe("mm:doc-changed");
    const before = [vi.mocked(story.overview).mock.calls.length, vi.mocked(media.list).mock.calls.length];
    await act(async () => {
      fire({ payload: { version: 2, txn_id: "x", kind: "op", name: "n", actor: "cli", touched: [] } });
    });
    expect(vi.mocked(story.overview).mock.calls.length).toBe((before[0] ?? 0) + 1);
    expect(vi.mocked(media.list).mock.calls.length).toBe((before[1] ?? 0) + 1);
    act(() => render(null, host));
    expect(off).toHaveBeenCalled();
  });

  it("Tasks の取り込みの項目は Media を開いてファイルの選択を始める", async () => {
    const media = fakeMedia();
    act(() => render(<Shell sendOp={vi.fn()} apis={{ media }} commands={fakeCommands(TABLE).api} />, host));
    await settle();
    click(taskLink("Import video and audio"));
    await settle();
    expect(host.querySelectorAll<HTMLElement>(".mm-shell-left-body")[1]?.hidden).toBe(false);
    expect(media.pickFiles).toHaveBeenCalled();
  });
});

describe("アプリのメニューとキーから届いたパネルのコマンド", () => {
  it("Import Media…（File のメニューと Ctrl+I）は Media を開いてファイルの選択を始める", async () => {
    const media = fakeMedia();
    const { api, fromApp } = fakeCommands(TABLE);
    act(() => render(<Shell sendOp={vi.fn()} apis={{ media }} commands={api} />, host));
    await settle();
    expect(media.pickFiles).not.toHaveBeenCalled();
    act(() => fromApp("media.import"));
    await settle();
    expect(host.querySelectorAll<HTMLElement>(".mm-shell-left-body")[1]?.hidden).toBe(false);
    expect(media.pickFiles).toHaveBeenCalledTimes(1);
    expect(api.run).not.toHaveBeenCalled();
  });

  it("Export… は Deliver のタブを開く", async () => {
    const { api, fromApp } = fakeCommands(TABLE);
    act(() => render(<Shell sendOp={vi.fn()} apis={{ deliver: fakeDeliver() }} commands={api} />, host));
    await settle();
    const panels = host.querySelectorAll<HTMLElement>(".mm-shell-left-body");
    expect(panels[1]?.hidden).toBe(true);
    act(() => fromApp("export.start"));
    await settle();
    expect(panels[1]?.hidden).toBe(false);
    expect(api.run).not.toHaveBeenCalled();
  });
});

describe("Shell と UI のコマンドの表", () => {
  it("アプリが持ち主の項目は run_command で実行する", async () => {
    const { api } = fakeCommands(TABLE);
    act(() => render(<Shell sendOp={vi.fn()} commands={api} />, host));
    await settle();
    expect(taskLink("Split at playhead")?.getAttribute("title")).toBe("Split Clip (B)");
    click(taskLink("Split at playhead"));
    await settle();
    expect(api.run).toHaveBeenCalledWith("clip.split");
  });

  it("実行できなかった理由を画面に出す", async () => {
    const { api } = fakeCommands(TABLE);
    vi.mocked(api.run).mockResolvedValue("Unknown command: clip.split");
    act(() => render(<Shell sendOp={vi.fn()} commands={api} />, host));
    await settle();
    click(taskLink("Split at playhead"));
    await settle();
    expect(host.querySelector(".mm-shell-error")?.textContent).toBe("Unknown command: clip.split");
  });

  it("押せるかが変わったイベントの表で、Tasks の項目が薄くなり、押しても実行しない", async () => {
    const { api, push } = fakeCommands(TABLE);
    act(() => render(<Shell sendOp={vi.fn()} commands={api} />, host));
    await settle();
    expect(taskLink("Split at playhead")?.getAttribute("aria-disabled")).toBeNull();
    expect(taskLink("Remove silent parts")?.getAttribute("aria-disabled")).toBe("true");
    act(() => push(TABLE.map((c) => (c.id === "clip.split" ? { ...c, enabled: false } : c))));
    expect(taskLink("Split at playhead")?.getAttribute("aria-disabled")).toBe("true");
    click(taskLink("Split at playhead"));
    await settle();
    expect(api.run).not.toHaveBeenCalled();
  });

  it("読む前に届いたイベントの表を、後から読み終えた表で上書きしない", async () => {
    const { api, push } = fakeCommands(TABLE);
    let finish: (table: UiCommand[]) => void = () => undefined;
    vi.mocked(api.table).mockReturnValue(new Promise((ok) => (finish = ok)));
    act(() => render(<Shell sendOp={vi.fn()} commands={api} />, host));
    await settle();
    act(() => push([command("clip.split", { enabled: false })]));
    await act(async () => finish(TABLE));
    expect(taskLink("Split at playhead")?.getAttribute("aria-disabled")).toBe("true");
  });
});

describe("Shell", () => {
  it("操作が拒否されたら理由を画面に出す", async () => {
    const sendOp = vi.fn().mockResolvedValue("setParam: no such layer");
    act(() => render(<Shell sendOp={sendOp} />, host));
    act(() => {
      window.dispatchEvent(new CustomEvent(LAYER_SELECTED_EVENT, { detail: { layer } }));
    });
    const opacity = host.querySelector<HTMLInputElement>('.mm-shell-right [aria-label="opacity"]');
    await act(async () => {
      if (opacity) opacity.value = "0.5";
      opacity?.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(host.querySelector(".mm-shell-error")?.textContent).toBe("setParam: no such layer");
  });

  it("右の列の Inspector は選択のイベントでレイヤーを出し、値の変更を sendOp に渡す", () => {
    const sendOp = vi.fn();
    act(() => render(<Shell sendOp={sendOp} />, host));
    expect(host.querySelector(".mm-shell-right")?.textContent).toContain("Select a layer in the Timeline");
    act(() => {
      window.dispatchEvent(new CustomEvent(LAYER_SELECTED_EVENT, { detail: { layer } }));
    });
    const opacity = host.querySelector<HTMLInputElement>('.mm-shell-right [aria-label="opacity"]');
    act(() => {
      if (opacity) opacity.value = "0.5";
      opacity?.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect(sendOp).toHaveBeenCalledWith({
      op: "setParam",
      target: { layer: "l1", path: "opacity" },
      value: { type: "number", value: 0.5 },
    });
  });

  it("選択のイベントを聞き始めてから今の選択を頼み、その答えでレイヤーを出す", () => {
    // 頼みを受けたその場で答える。Shell が聞き始める前に頼むと、この答えは届かない。
    const answer = () => window.dispatchEvent(new CustomEvent(LAYER_SELECTED_EVENT, { detail: { layer } }));
    window.addEventListener(LAYER_SELECTION_REQUEST_EVENT, answer);
    act(() => render(<Shell sendOp={vi.fn()} />, host));
    window.removeEventListener(LAYER_SELECTION_REQUEST_EVENT, answer);
    expect(host.querySelector('.mm-shell-right [aria-label="opacity"]')).not.toBeNull();
  });
});

describe("Shell と Timeline のキーフレームの行", () => {
  it("選択のイベントの、キーフレームの行で選んだパラメータを Inspector と Graph で選ぶ", () => {
    act(() => render(<Shell sendOp={vi.fn()} />, host));
    act(() => {
      const detail = { layer, param: { path: "opacity", serial: 1 } };
      window.dispatchEvent(new CustomEvent(LAYER_SELECTED_EVENT, { detail }));
    });
    expect(host.querySelector(".mm-shell-right .mm-graph-title")?.textContent).toBe("Graph: opacity");
  });
});
