import type { Op } from "@mm/sdk";
import type { StoryHit, StoryOverview, StoryTranscript, WordRange } from "@mm/story/src/api.ts";
import { API_ROOT, ApiError, type Api } from "../api/http.ts";

const STORY_ROOT = `${API_ROOT}/story`;
const CONFLICT = 409;

/** 送った結果。`conflict` は PC のバージョンが進んでいて適用されなかった（読み直して選び直す）。 */
export type SendResult = { kind: "applied" | "proposed" } | { kind: "conflict" } | { kind: "failed"; message: string };

export interface SendOptions {
  /** トランザクションの名前（取り消しの履歴に残る）。 */
  name: string;
  /** 「PC で決める」。即時に適用せず提案として出す。 */
  onPc: boolean;
}

/** Story が PC に頼むこと。形は `StoryApi`（デスクトップ）の読み出しと同じで、操作は HTTP で送る。 */
export interface StoryClient {
  overview: () => Promise<StoryOverview>;
  transcript: (asset: string) => Promise<StoryTranscript>;
  range: (asset: string, first: string, last: string) => Promise<WordRange>;
  search: (query: string) => Promise<StoryHit[]>;
  send: (ops: Op[], opts: SendOptions) => Promise<SendResult>;
  /** この端末が出した直前のトランザクションを取り消す。失敗の理由を返し、成功は `null`。 */
  undo: () => Promise<string | null>;
  audioUrl: (asset: string) => string;
}

const q = encodeURIComponent;

const classify = (e: unknown): SendResult => {
  if (e instanceof ApiError && e.status === CONFLICT) return { kind: "conflict" };
  return { kind: "failed", message: e instanceof Error ? e.message : String(e) };
};

/** `base_version` は送る時点の最後に読んだバージョン。PC が古いと判定したら `conflict`。 */
export const createStoryClient = (api: Api, baseVersion: () => number): StoryClient => ({
  overview: () => api.get(`${STORY_ROOT}/overview`),
  transcript: (asset) => api.get(`${STORY_ROOT}/transcript?asset=${q(asset)}`),
  range: (asset, first, last) => api.get(`${STORY_ROOT}/range?asset=${q(asset)}&first=${q(first)}&last=${q(last)}`),
  search: (query) => api.get(`${STORY_ROOT}/search?q=${q(query)}`),
  send: async (ops, { name, onPc }) => {
    const path = onPc ? `${API_ROOT}/proposals` : `${API_ROOT}/ops`;
    try {
      await api.post(path, { txn: { name, base_version: baseVersion(), ops } });
      return { kind: onPc ? "proposed" : "applied" };
    } catch (e) {
      return classify(e);
    }
  },
  undo: async () => {
    try {
      await api.post(`${API_ROOT}/undo`, {});
      return null;
    } catch (e) {
      return e instanceof Error ? e.message : String(e);
    }
  },
  audioUrl: (asset) => `${API_ROOT}/audio/${q(asset)}.m4a`,
});
