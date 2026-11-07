import type { PendingComment } from "./outbox.ts";

/** 送信待ちのキューを置く所。1 件ずつ書くので、描き込みつきのコメントが溜まっても全体を書き直さない。 */
export interface OutboxStore {
  load: () => Promise<PendingComment[]>;
  put: (item: PendingComment) => Promise<void>;
  delete: (clientId: string) => Promise<void>;
}

const DB_NAME = "mm-mobile";
const DB_VERSION = 1;
const STORE = "outbox";

const opened = (factory: IDBFactory): Promise<IDBDatabase> =>
  new Promise((resolve, reject) => {
    const req = factory.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: "client_id" });
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error("IndexedDB is not available."));
  });

/** 書き込みはトランザクションの完了（ディスクに届いた）まで待つ。 */
const write = (db: IDBDatabase, fn: (store: IDBObjectStore) => void): Promise<void> =>
  new Promise((resolve, reject) => {
    const tx = db.transaction(STORE, "readwrite");
    fn(tx.objectStore(STORE));
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });

const readAll = (db: IDBDatabase): Promise<PendingComment[]> =>
  new Promise((resolve, reject) => {
    const req = db.transaction(STORE, "readonly").objectStore(STORE).getAll();
    req.onsuccess = () => resolve(req.result as PendingComment[]);
    req.onerror = () => reject(req.error);
  });

/**
 * IndexedDB の送信待ちのキュー。localStorage は 5 MB 前後で上限に当たると書けず、毎回全体を書き直すため使わない。
 * 開けない端末（古い非公開モード）では全部の呼び出しが失敗し、`createOutbox` がメモリだけで続ける。
 */
export const createIdbStore = (factory: IDBFactory | undefined = globalThis.indexedDB): OutboxStore => {
  let db: Promise<IDBDatabase> | null = null;
  const open = (): Promise<IDBDatabase> => {
    if (!factory) return Promise.reject(new Error("IndexedDB is not available."));
    db ??= opened(factory);
    return db;
  };
  return {
    load: async () => readAll(await open()),
    put: async (item) => write(await open(), (s) => void s.put(item)),
    delete: async (clientId) => write(await open(), (s) => void s.delete(clientId)),
  };
};

/** メモリだけのキュー。テストとデモが使う。`seed` は開き直す前から保存してあった分。 */
export const createMemoryStore = (seed: PendingComment[] = []): OutboxStore & { items: Map<string, PendingComment> } => {
  const items = new Map(seed.map((c) => [c.client_id, c]));
  return {
    items,
    load: async () => [...items.values()],
    put: async (item) => void items.set(item.client_id, item),
    delete: async (clientId) => void items.delete(clientId),
  };
};
