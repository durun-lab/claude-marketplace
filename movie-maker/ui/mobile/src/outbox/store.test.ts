import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import type { PendingComment } from "./outbox.ts";
import { createIdbStore } from "./store.ts";

const STROKE_POINTS = 400;

/** 描き込みつきのコメント。点の多い線を持つ。 */
const drawn = (id: string, queuedAt: number): PendingComment => ({
  client_id: id,
  comp: "c1",
  at: { num: queuedAt, den: 1 },
  drawing: [{ kind: "stroke", points: Array.from({ length: STROKE_POINTS }, (_, i) => [i / STROKE_POINTS, 0.5] as [number, number]) }],
  queued_at: queuedAt,
});

describe("IndexedDB の送信待ちのキュー", () => {
  it("書いたコメントを、開き直した後に描き込みごと読める", async () => {
    const factory = new IDBFactory();
    const first = createIdbStore(factory);
    await first.put(drawn("a", 1));
    await first.put(drawn("b", 2));
    await first.delete("a");
    const loaded = await createIdbStore(factory).load();
    expect(loaded.map((c) => c.client_id)).toEqual(["b"]);
    expect(loaded[0]?.drawing?.[0]?.points).toHaveLength(STROKE_POINTS);
  });

  it("同じ client_id は上書きする（拒否の理由を足すとき）", async () => {
    const store = createIdbStore(new IDBFactory());
    await store.put(drawn("a", 1));
    await store.put({ ...drawn("a", 1), error: "too long" });
    expect((await store.load()).map((c) => c.error)).toEqual(["too long"]);
  });

  it("IndexedDB が無い端末では失敗を返す（呼ぶ側がメモリで続ける）", async () => {
    await expect(createIdbStore(undefined).load()).rejects.toThrow("IndexedDB");
  });
});
