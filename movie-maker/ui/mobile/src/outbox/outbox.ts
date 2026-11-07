import { ApiError, isRetryable } from "../api/http.ts";
import type { CommentDraft } from "../api/types.ts";
import type { OutboxStore } from "./store.ts";

/** 送信待ちのコメント。`error` があるものは PC に拒否されたので再送せず、人が消すまで残す。 */
export interface PendingComment extends CommentDraft {
  queued_at: number;
  error?: string;
}

export interface Outbox {
  list: () => PendingComment[];
  add: (draft: Omit<CommentDraft, "client_id">) => PendingComment;
  remove: (clientId: string) => void;
  /** 先頭から順に送る。再送で解消し得る失敗に当たったら、そこで止めて残りを残す。 */
  flush: (send: (draft: CommentDraft) => Promise<unknown>) => Promise<void>;
  subscribe: (fn: () => void) => () => void;
  /** 保存した送信待ちを読み終えたら解決する。 */
  ready: Promise<void>;
}

/** 端末の記録（`queued_at`、`error`）を除き、PC に送る本文だけにする。 */
const toDraft = (item: PendingComment): CommentDraft => {
  const draft: Partial<PendingComment> = { ...item };
  delete draft.queued_at;
  delete draft.error;
  return draft as CommentDraft;
};

const RADIX = 36;
const RANDOM_SKIP = 2;

/** randomUUID は http の文脈では使えない端末があるので、無ければ時刻と乱数で作る。 */
const newId = (): string =>
  globalThis.crypto?.randomUUID?.() ?? `c${Date.now().toString(RADIX)}${Math.random().toString(RADIX).slice(RANDOM_SKIP)}`;

/** 保存の失敗（非公開モード、容量の上限）はこのページの間のキューとして働かせ、画面を止めない。 */
const quietly = (p: Promise<void>): void => void p.catch(() => undefined);

/** 読み込みの前に足した分と、保存してあった分を、入れた順に 1 つの並びにする。 */
const mergeLoaded = (now: PendingComment[], loaded: PendingComment[]): PendingComment[] => {
  const known = new Set(now.map((c) => c.client_id));
  return [...loaded.filter((c) => !known.has(c.client_id)), ...now].sort((a, b) => a.queued_at - b.queued_at);
};

/** 画面に出す並びと保存先を、1 件ずつ揃えて書き換える。 */
const createItems = (store: OutboxStore) => {
  let items: PendingComment[] = [];
  const listeners = new Set<() => void>();
  const set = (next: PendingComment[]): void => {
    items = next;
    listeners.forEach((fn) => fn());
  };
  return {
    get: () => items,
    save: (item: PendingComment): void => {
      const known = items.some((c) => c.client_id === item.client_id);
      set(known ? items.map((c) => (c.client_id === item.client_id ? item : c)) : [...items, item]);
      quietly(store.put(item));
    },
    drop: (clientId: string): void => {
      set(items.filter((c) => c.client_id !== clientId));
      quietly(store.delete(clientId));
    },
    ready: store.load().then(
      (loaded) => set(mergeLoaded(items, loaded)),
      () => undefined,
    ),
    subscribe: (fn: () => void): (() => void) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
};

/** コメントの送信待ちのキュー。通信できない間も書け、再接続で送る。ページを閉じても残す。 */
export const createOutbox = (store: OutboxStore, now: () => number = Date.now): Outbox => {
  const items = createItems(store);
  let flushing = false;
  const sendOne = async (send: (draft: CommentDraft) => Promise<unknown>, item: PendingComment): Promise<boolean> => {
    try {
      await send(toDraft(item));
      items.drop(item.client_id);
    } catch (e) {
      if (isRetryable(e)) return false;
      items.save({ ...item, error: e instanceof ApiError ? e.message : String(e) });
    }
    return true;
  };
  return {
    list: items.get,
    add: (draft) => {
      const item: PendingComment = { ...draft, client_id: newId(), queued_at: now() };
      items.save(item);
      return item;
    },
    remove: items.drop,
    flush: async (send) => {
      if (flushing) return;
      flushing = true;
      try {
        await items.ready;
        for (const item of items.get().filter((c) => !c.error)) {
          if (!(await sendOne(send, item))) return;
        }
      } finally {
        flushing = false;
      }
    },
    subscribe: items.subscribe,
    ready: items.ready,
  };
};
