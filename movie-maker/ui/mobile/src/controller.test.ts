import { describe, expect, it } from "vitest";
import { PAIRING_NOT_KEPT } from "./api/errors.ts";
import { PAIR_PATH } from "./api/http.ts";
import { createController } from "./controller.ts";
import { MOCK_PAIR_CODE, createMockServer } from "./mock/mockServer.ts";
import { createMemoryStore } from "./outbox/store.ts";

/** SSE は開いたまま、送信だけが失敗する回線（電波の弱い所。Playwright の setOffline も同じ形になる）。 */
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

/** PC の登録は済むが、端末がクッキーを保存しない（Safari のクッキーを全部止める設定など）。 */
const setupPairing = (keepsCookie: boolean) => {
  const server = createMockServer({ paired: false, online: true });
  return createController({
    ...server.platform,
    fetch: async (path, init) => {
      const res = await server.platform.fetch(path, init);
      if (path === PAIR_PATH && !keepsCookie) server.setPaired(false);
      return res;
    },
    onNetworkBack: () => undefined,
    outboxStore: createMemoryStore(),
  });
};

describe("controller の登録", () => {
  it("資格が効けばつながる", async () => {
    const controller = setupPairing(true);
    await controller.pair(MOCK_PAIR_CODE, "iPhone");
    expect(controller.connection.get().state).toBe("online");
  });

  it("登録の応答の後に資格が効かなければ、黙らずに知らせる", async () => {
    const controller = setupPairing(false);
    const failure = controller.pair(MOCK_PAIR_CODE, "iPhone");
    await expect(failure).rejects.toMatchObject({ code: PAIRING_NOT_KEPT });
    await expect(failure).rejects.toThrow(/did not keep the sign-in cookie/);
    expect(controller.connection.get().state).toBe("unpaired");
  });
});
