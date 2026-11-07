import { describe, expect, it } from "vitest";
import { ApiError } from "../api/http.ts";
import { createOutbox, type KeyValueStore } from "./outbox.ts";

const memory = (): KeyValueStore => {
  const data = new Map<string, string>();
  return { getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v) };
};

describe("outbox", () => {
  it("ページを開き直しても送信待ちが残る", () => {
    const store = memory();
    createOutbox(store).add("one", null);
    expect(createOutbox(store).list().map((c) => c.text)).toEqual(["one"]);
  });

  it("再接続で入れた順に送り、送れたものを消す", async () => {
    const outbox = createOutbox(memory());
    outbox.add("a", null);
    outbox.add("b", null);
    const sent: string[] = [];
    await outbox.flush(async (d) => void sent.push(d.text));
    expect(sent).toEqual(["a", "b"]);
    expect(outbox.list()).toEqual([]);
  });

  it("通信の失敗で止まり、残りを残す", async () => {
    const outbox = createOutbox(memory());
    outbox.add("a", null);
    outbox.add("b", null);
    let calls = 0;
    await outbox.flush(async () => {
      calls++;
      throw new ApiError(0, "network", "offline");
    });
    expect(calls).toBe(1);
    expect(outbox.list()).toHaveLength(2);
  });

  it("母艦に断られたコメントは error を付けて残し、次を送る", async () => {
    const outbox = createOutbox(memory());
    outbox.add("bad", null);
    outbox.add("good", null);
    await outbox.flush(async (d) => {
      if (d.text === "bad") throw new ApiError(400, "invalid", "too long");
    });
    expect(outbox.list().map((c) => [c.text, c.error])).toEqual([["bad", "too long"]]);
    await outbox.flush(async () => {
      throw new Error("not called for rejected");
    });
    expect(outbox.list()).toHaveLength(1);
  });
});

describe("outbox の失敗への備え", () => {
  it("保存が使えない端末でも、そのページの間は列として働く", () => {
    const denied = () => {
      throw new Error("denied");
    };
    const outbox = createOutbox({ getItem: denied, setItem: denied });
    outbox.add("a", null);
    expect(outbox.list()).toHaveLength(1);
  });

  it("同時に flush しても二重に送らない", async () => {
    const outbox = createOutbox(memory());
    outbox.add("a", null);
    let calls = 0;
    const slow = async () => {
      calls++;
      await Promise.resolve();
    };
    await Promise.all([outbox.flush(slow), outbox.flush(slow)]);
    expect(calls).toBe(1);
  });
});
