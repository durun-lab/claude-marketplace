import type { RationalTime } from "@mm/sdk";
import { ApiError, isRetryable } from "../api/http.ts";
import type { CommentDraft } from "../api/types.ts";

/** 送信待ちのコメント。`error` があるものは母艦に断られたので再送せず、人が消すまで残す。 */
export interface PendingComment extends CommentDraft {
  queued_at: number;
  error?: string;
}

/** `Storage` の最小の部分。iOS の非公開モードなどで投げるので、呼ぶ側が try で囲む。 */
export interface KeyValueStore {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

export interface Outbox {
  list: () => PendingComment[];
  add: (text: string, at: RationalTime | null) => PendingComment;
  remove: (clientId: string) => void;
  /** 先頭から順に送る。再送で解消し得る失敗に当たったら、そこで止めて残りを残す。 */
  flush: (send: (draft: CommentDraft) => Promise<unknown>) => Promise<void>;
  subscribe: (fn: () => void) => () => void;
}

const STORAGE_KEY = "mm-mobile-outbox";

const read = (store: KeyValueStore, key: string): PendingComment[] => {
  try {
    const parsed: unknown = JSON.parse(store.getItem(key) ?? "[]");
    return Array.isArray(parsed) ? (parsed as PendingComment[]) : [];
  } catch {
    return [];
  }
};

const RADIX = 36;
const RANDOM_SKIP = 2;

/** randomUUID は http の文脈では使えない端末があるので、無ければ時刻と乱数で作る。 */
const newId = (): string =>
  globalThis.crypto?.randomUUID?.() ?? `c${Date.now().toString(RADIX)}${Math.random().toString(RADIX).slice(RANDOM_SKIP)}`;

/** コメントの送信待ちの列。通信できない間も書け、再接続で送る。ページを閉じても残す。 */
export const createOutbox = (store: KeyValueStore, key = STORAGE_KEY, now: () => number = Date.now): Outbox => {
  let items = read(store, key);
  const listeners = new Set<() => void>();
  let flushing = false;
  const commit = (next: PendingComment[]): void => {
    items = next;
    try {
      store.setItem(key, JSON.stringify(items));
    } catch {
      // 保存できない端末でもこのページの間は列として働く
    }
    listeners.forEach((fn) => fn());
  };
  const replace = (clientId: string, patch: Partial<PendingComment>): void =>
    commit(items.map((c) => (c.client_id === clientId ? { ...c, ...patch } : c)));
  return {
    list: () => items,
    add: (text, at) => {
      const item: PendingComment = { client_id: newId(), text, at, queued_at: now() };
      commit([...items, item]);
      return item;
    },
    remove: (clientId) => commit(items.filter((c) => c.client_id !== clientId)),
    flush: async (send) => {
      if (flushing) return;
      flushing = true;
      const sendOne = async (item: PendingComment): Promise<boolean> => {
        try {
          await send({ client_id: item.client_id, text: item.text, at: item.at });
          commit(items.filter((c) => c.client_id !== item.client_id));
        } catch (e) {
          if (isRetryable(e)) return false;
          replace(item.client_id, { error: e instanceof ApiError ? e.message : String(e) });
        }
        return true;
      };
      try {
        for (const item of items.filter((c) => !c.error)) {
          if (!(await sendOne(item))) return;
        }
      } finally {
        flushing = false;
      }
    },
    subscribe: (fn) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
};
