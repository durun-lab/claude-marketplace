import { render, type ComponentChild } from "preact";
import { act } from "preact/test-utils";
import { createController } from "../controller.ts";
import { createMockServer } from "../mock/mockServer.ts";
import { createMemoryStore } from "../outbox/store.ts";
import "./stubMedia.ts";

type MockInit = Parameters<typeof createMockServer>[0];

/** 画面のテストの土台。偽の PC につないだ controller と、描画・操作の関数。 */
export const createHarness = (init: Partial<MockInit> = {}) => {
  const host = document.createElement("div");
  document.body.append(host);
  const server = createMockServer({ paired: true, online: true, ...init });
  const controller = createController({
    ...server.platform,
    onNetworkBack: () => undefined,
    outboxStore: createMemoryStore(),
  });
  const settle = (): Promise<void> => act(() => new Promise<void>((r) => setImmediate(r)));
  const buttons = (): HTMLElement[] => [...host.querySelectorAll<HTMLElement>("button")];
  return {
    host,
    server,
    controller,
    settle,
    /** 描画して接続し、最初の読み込みが終わるまで待つ。 */
    mount: async (node: ComponentChild): Promise<void> => {
      await act(() => render(node, host));
      await act(() => controller.connection.start());
      await settle();
      await settle();
    },
    unmount: (): void => {
      render(null, host);
      host.remove();
    },
    /** 文言の先頭が合うボタンを押す。 */
    press: async (text: string): Promise<void> => {
      const button = buttons().find((b) => b.textContent?.trim().startsWith(text)) ?? buttons().find((b) => b.textContent?.includes(text));
      if (!button) throw new Error(`button not found: ${text}`);
      await act(() => void button.click());
      await settle();
    },
    type: (selector: string, value: string): Promise<void> =>
      act(() => {
        const el = host.querySelector<HTMLInputElement>(selector);
        if (!el) throw new Error(`not found: ${selector}`);
        el.value = value;
        el.dispatchEvent(new Event("input", { bubbles: true }));
      }),
    submit: async (selector: string): Promise<void> => {
      await act(() => void host.querySelector(selector)?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
      await settle();
    },
    texts: (selector: string): string[] => [...host.querySelectorAll(selector)].map((e) => e.textContent ?? ""),
  };
};
