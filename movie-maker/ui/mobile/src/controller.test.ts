import { describe, expect, it } from "vitest";
import { createController } from "./controller.ts";
import { createMockServer } from "./mock/mockServer.ts";
import { createMemoryStore } from "./outbox/store.ts";

/** SSE は開いたまま、送信だけが落ちる回線（電波の弱い所。Playwright の setOffline も同じ形になる）。 */
const setup = () => {
  const server = createMockServer({ paired: true, online: true });
  let cut = false;
  let networkBack = (): void => undefined;
  const controller = createController({
    ...server.platform,
    fetch: (path, init) => (cut ? Promise.reject(new TypeError("Failed to fetch")) : server.platform.fetch(path, init)),
    onNetworkBack: (fn) => {
      const before = networkBack;
      networkBack = () => {
        before();
        fn();
      };
    },
    outboxStore: createMemoryStore(),
  });
  return { server, controller, setCut: (v: boolean) => (cut = v), back: () => networkBack() };
};
const settle = () => new Promise<void>((r) => setTimeout(r, 0));

describe("controller の送信待ち", () => {
  it("SSE が開いたまま送れなかったコメントは、接続を offline にし、回線が戻ったら送る", async () => {
    const { server, controller, setCut, back } = setup();
    await controller.connection.start();
    await settle();
    expect(controller.connection.get().state).toBe("online");
    setCut(true);
    await controller.comment({ comp: "main", at: { num: 1, den: 1 }, text: "weak signal" });
    expect(controller.outbox.list()).toHaveLength(1);
    expect(controller.connection.get().state).toBe("offline");
    setCut(false);
    back();
    await settle();
    await settle();
    expect(server.comments.map((c) => c.text)).toEqual(["weak signal"]);
    expect(controller.outbox.list()).toEqual([]);
    expect(controller.connection.get().state).toBe("online");
  });
});
