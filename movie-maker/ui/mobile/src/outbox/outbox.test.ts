import { describe, expect, it } from "vitest";
import { ApiError } from "../api/http.ts";
import { createOutbox } from "./outbox.ts";
import { createMemoryStore, type OutboxStore } from "./store.ts";

describe("outbox", () => {
  it("ページを開き直しても送信待ちが残る", async () => {
    const store = createMemoryStore();
    const first = createOutbox(store);
    await first.ready;
    first.add({ comp: "c1", text: "one" });
    const reopened = createOutbox(store);
    await reopened.ready;
    expect(reopened.list().map((c) => c.text)).toEqual(["one"]);
  });

  it("読み込みの前に足した分は、保存してあった分の後ろに並ぶ", async () => {
    const store = createMemoryStore([{ client_id: "old", comp: "c1", text: "old", queued_at: 1 }]);
    const outbox = createOutbox(store, () => 2);
    outbox.add({ comp: "c1", text: "new" });
    await outbox.ready;
    expect(outbox.list().map((c) => c.text)).toEqual(["old", "new"]);
  });

  it("再接続で入れた順に送り、送れたものを消す", async () => {
    const store = createMemoryStore();
    const outbox = createOutbox(store);
    outbox.add({ comp: "c1", text: "a" });
    outbox.add({ comp: "c1", text: "b" });
    const sent: string[] = [];
    await outbox.flush(async (d) => void sent.push(d.text ?? ""));
    expect(sent).toEqual(["a", "b"]);
    expect(outbox.list()).toEqual([]);
    expect(store.items.size).toBe(0);
  });
});

describe("outbox の本文", () => {
  it("範囲と描き込みを保ったまま、端末の記録を除いた本文を送る", async () => {
    const store = createMemoryStore();
    const drawing = [{ kind: "rect" as const, points: [[0.1, 0.2], [0.3, 0.4]] as [number, number][] }];
    const range = { in: { num: 1, den: 1 }, out: { num: 2, den: 1 } };
    const queued = createOutbox(store).add({ comp: "c1", range, drawing });
    const sent: unknown[] = [];
    await createOutbox(store).flush(async (d) => void sent.push(d));
    expect(sent).toEqual([{ comp: "c1", range, drawing, client_id: queued.client_id }]);
  });

  it("通信の失敗で止まり、残りを残す", async () => {
    const outbox = createOutbox(createMemoryStore());
    outbox.add({ comp: "c1", text: "a" });
    outbox.add({ comp: "c1", text: "b" });
    let calls = 0;
    await outbox.flush(async () => {
      calls++;
      throw new ApiError(0, "network", "offline");
    });
    expect(calls).toBe(1);
    expect(outbox.list()).toHaveLength(2);
  });

  it("PC に拒否されたコメントは error を付けて残し、次を送る", async () => {
    const store = createMemoryStore();
    const outbox = createOutbox(store);
    outbox.add({ comp: "c1", text: "bad" });
    outbox.add({ comp: "c1", text: "good" });
    await outbox.flush(async (d) => {
      if (d.text === "bad") throw new ApiError(400, "invalid", "too long");
    });
    expect(outbox.list().map((c) => [c.text, c.error])).toEqual([["bad", "The PC could not apply this change."]]);
    expect([...store.items.values()].map((c) => c.error)).toEqual(["The PC could not apply this change."]);
    await outbox.flush(async () => {
      throw new Error("not called for rejected");
    });
    expect(outbox.list()).toHaveLength(1);
  });
});

describe("outbox の失敗への備え", () => {
  it("保存が使えない端末でも、そのページの間はキューとして働く", async () => {
    const denied = (): Promise<never> => Promise.reject(new Error("denied"));
    const store: OutboxStore = { load: denied, put: denied, delete: denied };
    const outbox = createOutbox(store);
    outbox.add({ comp: "c1", text: "a" });
    await outbox.ready;
    expect(outbox.list()).toHaveLength(1);
    const sent: string[] = [];
    await outbox.flush(async (d) => void sent.push(d.text ?? ""));
    expect(sent).toEqual(["a"]);
  });

  it("同時に flush しても二重に送らない", async () => {
    const outbox = createOutbox(createMemoryStore());
    outbox.add({ comp: "c1", text: "a" });
    let calls = 0;
    const slow = async () => {
      calls++;
      await Promise.resolve();
    };
    await Promise.all([outbox.flush(slow), outbox.flush(slow)]);
    expect(calls).toBe(1);
  });
});
