// @vitest-environment jsdom
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App.tsx";
import { createController } from "./controller.ts";
import { MOCK_PAIR_CODE, createMockServer } from "./mock/mockServer.ts";
import { createMemoryStore } from "./outbox/store.ts";
import { codeFromSearch, guessDeviceName, normalizeCode } from "./pairing/PairScreen.tsx";
import "./testing/stubMedia.ts";

const host = document.createElement("div");
document.body.append(host);
const must = <T,>(v: T | null | undefined): T => {
  if (v == null) throw new Error("element not found");
  return v;
};
afterEach(() => render(null, host));

const setup = (init: { paired: boolean; online: boolean }, search = "") => {
  const server = createMockServer(init);
  const controller = createController({
    ...server.platform,
    onNetworkBack: () => undefined,
    outboxStore: createMemoryStore(),
  });
  const mount = () => act(() => render(<App controller={controller} search={search} userAgent="iPhone" />, host));
  return { server, controller, mount };
};
const settle = () => act(() => new Promise<void>((r) => setTimeout(r, 0)));
const submit = (selector: string) =>
  act(() => void must(host.querySelector<HTMLFormElement>(selector)).dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
const press = (text: string) =>
  act(() => void [...host.querySelectorAll<HTMLElement>("button")].find((b) => b.textContent?.trim().startsWith(text))?.click());
const type = (el: HTMLInputElement | HTMLTextAreaElement, value: string) =>
  act(() => {
    el.value = value;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });

describe("App", () => {
  it("登録済みなら 3 つのタブと接続中の表示を出す", async () => {
    const { controller, mount } = setup({ paired: true, online: true });
    await mount();
    await act(() => controller.connection.start());
    await settle();
    expect([...host.querySelectorAll("[role=tab]")].map((t) => t.textContent)).toEqual(["Watch", "Story", "Proposals"]);
    expect(host.querySelector(".chip")?.textContent).toContain("Connected");
  });

  it("未登録なら登録の画面を出し、符号を入れると登録してタブに進む", async () => {
    const { controller, server, mount } = setup({ paired: false, online: true }, "?code=abcd-2345");
    await mount();
    await act(() => controller.connection.start());
    expect(host.querySelector<HTMLInputElement>('input[aria-label="Pairing code"]')?.value).toBe(MOCK_PAIR_CODE);
    expect(host.querySelector<HTMLInputElement>('input[aria-label="Device name"]')?.value).toBe("iPhone");
    await submit("form.pair");
    await settle();
    expect(server.writes[0]?.path).toBe("/m/pair");
    expect(host.querySelector("[role=tab]")).not.toBeNull();
  });

  it("間違った符号は PC の文言を出し、登録の画面に留まる", async () => {
    const { controller, mount } = setup({ paired: false, online: true });
    await mount();
    await act(() => controller.connection.start());
    await type(must(host.querySelector<HTMLInputElement>('input[aria-label="Pairing code"]')), "zzzzzzzz");
    await submit("form.pair");
    await settle();
    expect(host.querySelector(".error")?.textContent).toContain("wrong or has expired");
    expect(host.querySelector("[role=tab]")).toBeNull();
  });

});

describe("App のコメント", () => {
  it("圏外で書いたコメントは送信待ちに入り、再接続で PC に届く", async () => {
    const { controller, server, mount } = setup({ paired: true, online: true });
    await mount();
    await act(() => controller.connection.start());
    await settle();
    await act(() => void server.setOnline(false));
    expect(host.querySelector(".chip")?.textContent).toContain("Offline");
    await press("Comment");
    await type(must(host.querySelector<HTMLTextAreaElement>("textarea")), "check 0:42");
    await submit("form.compose");
    expect(host.querySelector(".chip")?.textContent).toContain("1 queued");
    expect(host.querySelector(".pending")?.textContent).toContain("Waiting to send");
    server.setOnline(true);
    await act(() => controller.connection.refresh());
    await settle();
    expect(server.comments[0]?.text).toBe("check 0:42");
    expect(controller.outbox.list()).toEqual([]);
  });
});

describe("pairing helpers", () => {
  it("符号は大文字にそろえ、区切りと余りを落とす", () => {
    expect(normalizeCode(" ab-cd 2345xyz")).toBe("ABCD2345");
    expect(codeFromSearch("?code=abcd2345")).toBe("ABCD2345");
    expect(codeFromSearch("")).toBe("");
  });
  it("端末の名前を User-Agent から決める", () => {
    expect(guessDeviceName("Mozilla/5.0 (iPhone; CPU iPhone OS 17)")).toBe("iPhone");
    expect(guessDeviceName("Mozilla/5.0 (Linux; Android 14)")).toBe("Android");
    expect(guessDeviceName("x")).toBe("Mobile");
  });
});
