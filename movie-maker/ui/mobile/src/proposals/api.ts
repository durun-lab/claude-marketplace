import { API_ROOT, type Api } from "../api/http.ts";
import type { ApproveResult, Proposal } from "@mm/review/src/api.ts";

export type { Proposal };

const PROPOSALS = `${API_ROOT}/proposals`;
const FRAME_WIDTH = 720;

/** 提案の一覧。PC の順（新しい順）のまま返す。 */
export const listProposals = async (api: Api): Promise<Proposal[]> =>
  (await api.get<{ proposals: Proposal[] }>(PROPOSALS)).proposals;

/** 承認する。`ops` は承認する操作の番号で、全部なら省く。 */
export const approveProposal = (api: Api, id: number, ops: number[] | null): Promise<ApproveResult> =>
  api.post(`${PROPOSALS}/${id}/approve`, ops ? { ops } : {});

export const rejectProposal = (api: Api, id: number): Promise<Proposal> => api.post(`${PROPOSALS}/${id}/reject`, {});

/** 却下を保留に戻す。却下の取り消しに使う。 */
export const reopenProposal = (api: Api, id: number): Promise<Proposal> => api.post(`${PROPOSALS}/${id}/reopen`, {});

/** 全体の直前が自分の操作のときだけ効く。承認の取り消しに使う。 */
export const undoLast = (api: Api): Promise<unknown> => api.post(`${API_ROOT}/undo`, {});

/**
 * フレームの静止画の URL。`t` はコンポジションの時間軸の秒。`comp` を省くと PC が既定のコンポジションを選ぶので、
 * Watch と同じものを見るよう渡す。`proposal` を渡すと提案を適用したフレーム、`ops` は一部だけ適用するときの番号。
 */
export const frameUrl = (t: number, opts: { comp?: string; proposal?: number; ops?: number[]; width?: number } = {}): string => {
  const q = new URLSearchParams({ t: String(t), width: String(opts.width ?? FRAME_WIDTH) });
  if (opts.comp) q.set("comp", opts.comp);
  if (opts.proposal !== undefined) q.set("proposal", String(opts.proposal));
  if (opts.ops) q.set("ops", opts.ops.join(","));
  return `${API_ROOT}/frame?${q}`;
};

/**
 * 提案の区間の短い動画（範囲の前後 2 秒、MP4）の URL を提案の応答から読む。`comp` は範囲を見るコンポジション、
 * `ops` は一部だけ適用するときの番号。URL は範囲に掛かるもののハッシュを含むので、範囲の中が編集されると変わる。
 * PC は作り終えるまで応答を待たせるので、`<video>` にそのまま渡せる。出せない提案は null。
 */
export const fetchClipUrl = async (api: Api, id: number, opts: { comp?: string; ops?: number[] | null } = {}): Promise<string | null> => {
  const q = new URLSearchParams();
  if (opts.comp) q.set("comp", opts.comp);
  if (opts.ops) q.set("ops", opts.ops.join(","));
  const query = q.toString();
  const res = await api.get<{ clip_url?: string | null }>(`${PROPOSALS}/${id}${query ? `?${query}` : ""}`);
  return res.clip_url ?? null;
};