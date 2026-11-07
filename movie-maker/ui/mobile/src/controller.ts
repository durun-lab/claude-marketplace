import type { Transport } from "@mm/sdk";
import { API_ROOT, PAIR_PATH, createApi, type Api, type FetchLike } from "./api/http.ts";
import { createMobileTransport } from "./api/transport.ts";
import type { CommentDraft, PairResult } from "./api/types.ts";
import { createConnection, type Connection, type EventSourceLike } from "./connection.ts";
import { createOutbox, type Outbox } from "./outbox/outbox.ts";
import type { OutboxStore } from "./outbox/store.ts";

export interface Platform {
  fetch: FetchLike;
  openEvents: (url: string) => EventSourceLike;
  onNetworkBack: (fn: () => void) => void;
  /** 送信待ちのキューの保存先。実機は IndexedDB。 */
  outboxStore: OutboxStore;
}

/** 画面が使う部品の束。実機はブラウザーの実物、デモとテストは差し替えを渡す。 */
export interface Controller {
  api: Api;
  connection: Connection;
  outbox: Outbox;
  /** 操作を PC に送る Transport。Story の操作が使う。 */
  transport: Transport;
  /** 登録の符号を送り、成功したら接続をやり直す。 */
  pair: (code: string, deviceName: string) => Promise<PairResult>;
  /** コメントを送信待ちのキューに入れ、つながっていればすぐ送る。 */
  comment: (draft: Omit<CommentDraft, "client_id">) => Promise<void>;
}

const sendDraft = (api: Api) => (draft: CommentDraft) => api.post(`${API_ROOT}/comments`, draft);

/** 画面の部品を組み立て、接続が `online` になるたびに送信待ちのキューを送る。 */
export const createController = (platform: Platform): Controller => {
  const api = createApi(platform.fetch);
  const connection = createConnection({ api, openEvents: platform.openEvents, onNetworkBack: platform.onNetworkBack });
  const outbox = createOutbox(platform.outboxStore);
  const flush = (): Promise<void> => outbox.flush(sendDraft(api));
  let last = connection.get().state;
  connection.subscribe(() => {
    const now = connection.get().state;
    if (now === "online" && last !== "online") void flush();
    last = now;
  });
  return {
    api,
    connection,
    outbox,
    transport: createMobileTransport(api, connection.version),
    pair: async (code, deviceName) => {
      const res = await api.post<PairResult>(PAIR_PATH, { code, device_name: deviceName });
      await connection.start();
      return res;
    },
    comment: async (draft) => {
      outbox.add(draft);
      if (connection.get().state === "online") await flush();
    },
  };
};
