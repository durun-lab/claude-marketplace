// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import replied from "../fixtures/replied.json";
import type { FeedbackApi, FeedbackItem, ImproveStatus } from "./api.ts";
import { FeedbackPanel } from "./FeedbackPanel.tsx";

const host = document.createElement("div");
document.body.append(host);
afterEach(() => render(null, host));

const NOW = 1793967600000;
const REPLIED = replied as FeedbackItem;

const idle = (): ImproveStatus => ({
  settings: { enabled: true, model: null, effort: null, budgetUsd: 5, wallMinutes: 60, perfThreshold: 0.05 },
  running: false,
  run: null,
  pending: 0,
  waiting: null,
  last: null,
});

const fakeApi = (items: FeedbackItem[], over: Partial<FeedbackApi> = {}) => {
  let fire: () => void = () => undefined;
  const api: FeedbackApi = {
    status: vi.fn().mockResolvedValue(idle()),
    list: vi.fn(async () => [...items]),
    get: vi.fn(),
    submit: vi.fn(async (f) => ({ ...REPLIED, id: "new", text: f.text, status: "pending" as const, replies: [] })),
    settings: vi.fn(async (p) => ({ ...idle(), settings: { ...idle().settings, ...p } })),
    stop: vi.fn(async () => ({ ...idle(), settings: { ...idle().settings, enabled: false } })),
    capture: vi.fn().mockResolvedValue("C:/shots/1.png"),
    reportError: vi.fn().mockResolvedValue(undefined),
    onChanged: vi.fn(async (h: () => void) => {
      fire = h;
      return () => undefined;
    }),
    ...over,
  };
  return { api, fire: () => fire() };
};

const flush = () => act(() => new Promise<void>((ok) => setTimeout(ok, 0)));

const input = (el: Element | null, value: string) =>
  act(() => {
    const t = el as HTMLTextAreaElement;
    t.value = value;
    t.dispatchEvent(new Event("input", { bubbles: true }));
  });

const click = (el: Element | null | undefined) =>
  act(() => {
    (el as HTMLElement).click();
  });

const byText = (text: string) => [...host.querySelectorAll("button")].find((b) => b.textContent === text);

describe("FeedbackPanel", () => {
  it("要望と状態、返事を出す", async () => {
    const { api } = fakeApi([REPLIED]);
    act(() => render(<FeedbackPanel api={api} nowMs={NOW} />, host));
    await flush();
    expect(host.querySelector(".mm-feedback-badge")?.textContent).toBe("Landed");
    expect(host.querySelector(".mm-feedback-reply-text")?.textContent).toBe(REPLIED.replies[0]?.text);
    expect(host.querySelector(".mm-feedback-reply .mm-feedback-meta")?.textContent).toBe("Landed 3f9c2a1 · 10m ago");
    expect(host.querySelector(".mm-feedback-headline")?.textContent).toBe("Idle");
  });

  it("起票すると送り、回が終わった知らせで返事の付いた要望を読み直す", async () => {
    const items: FeedbackItem[] = [];
    const { api, fire } = fakeApi(items);
    act(() => render(<FeedbackPanel api={api} nowMs={NOW} />, host));
    await flush();
    expect(host.querySelector(".mm-feedback-empty")).not.toBeNull();
    await input(host.querySelector("textarea"), "  Make the send button easier to see  ");
    click(byText("Send"));
    await flush();
    expect(api.submit).toHaveBeenCalledWith({ text: "Make the send button easier to see", targets: [], context: { viewport: expect.any(Object) } });
    expect(host.querySelector("textarea")?.value).toBe("");
    items.push(REPLIED);
    fire();
    await flush();
    expect(host.querySelector(".mm-feedback-reply-text")?.textContent).toBe(REPLIED.replies[0]?.text);
  });

  it("画面の部品を指すと、スクリーンショットつきで添える", async () => {
    const { api } = fakeApi([]);
    act(() => render(<FeedbackPanel api={api} nowMs={NOW} />, host));
    await flush();
    click(byText("Point at UI"));
    await flush();
    click(byText("All"));
    await flush();
    expect(api.capture).toHaveBeenCalled();
    expect(host.querySelector(".mm-feedback-chip-text")?.textContent).toBe("Feedback: All (screenshot)");
    expect(api.list).toHaveBeenCalledTimes(1);
    await input(host.querySelector("textarea"), "This filter is confusing");
    click(byText("Send"));
    await flush();
    const sent = vi.mocked(api.submit).mock.calls[0]?.[0];
    expect(sent?.targets[0]).toMatchObject({ panel: "Feedback", label: "All", screenshot: "C:/shots/1.png" });
  });

});

describe("FeedbackPanel の設定", () => {
  it("ON / OFF、予算、すぐ止めるをアプリに送る", async () => {
    const { api } = fakeApi([]);
    act(() => render(<FeedbackPanel api={api} nowMs={NOW} />, host));
    await flush();
    const budget = host.querySelector<HTMLInputElement>('input[type="number"]') ?? document.createElement("input");
    act(() => {
      budget.value = "1";
      budget.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await flush();
    expect(api.settings).toHaveBeenLastCalledWith({ budgetUsd: 1 });
    act(() => {
      budget.value = "0";
      budget.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await flush();
    expect(api.settings).toHaveBeenCalledTimes(1);
    click(byText("Stop now"));
    await flush();
    expect(api.stop).toHaveBeenCalled();
    expect(byText("Stop now")?.disabled).toBe(true);
    click(host.querySelector('input[role="switch"]'));
    await flush();
    expect(api.settings).toHaveBeenLastCalledWith({ enabled: true });
  });

  it("指すのをやめるボタンは指さずに効き、枠を消す", async () => {
    const { api } = fakeApi([]);
    act(() => render(<FeedbackPanel api={api} nowMs={NOW} />, host));
    await flush();
    click(byText("Point at UI"));
    await flush();
    click(byText("Cancel pointing"));
    await flush();
    expect(api.capture).not.toHaveBeenCalled();
    expect(byText("Point at UI")).toBeDefined();
    expect(host.querySelector(".mm-feedback-highlight")).toBeNull();
  });

  it("アプリが使えないときは理由を出す", async () => {
    const { api } = fakeApi([], { status: vi.fn().mockRejectedValue(new Error("Auto-improve is unavailable")) });
    act(() => render(<FeedbackPanel api={api} nowMs={NOW} />, host));
    await flush();
    expect(host.querySelector('[role="alert"]')?.textContent).toBe("Auto-improve is unavailable");
  });
});
