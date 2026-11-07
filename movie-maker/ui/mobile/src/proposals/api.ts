import { API_ROOT, type Api } from "../api/http.ts";
import type { ApproveResult, Proposal } from "../../../panels/review/src/api.ts";

export type { Proposal };

const PROPOSALS = `${API_ROOT}/proposals`;
const FRAME_WIDTH = 720;

/** 提案の一覧。母艦の順（新しい順）のまま返す。 */
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

/** コマの WebP の URL。`proposal` を渡すと提案を適用した絵。`ops` は一部だけ適用するときの番号。 */
export const frameUrl = (t: number, opts: { proposal?: number; ops?: number[]; width?: number } = {}): string => {
  const q = new URLSearchParams({ t: String(t), width: String(opts.width ?? FRAME_WIDTH) });
  if (opts.proposal !== undefined) q.set("proposal", String(opts.proposal));
  if (opts.ops) q.set("ops", opts.ops.join(","));
  return `${API_ROOT}/frame?${q}`;
};

/** 提案の区間の短い動画。 */
export const clipUrl = (id: number): string => `${PROPOSALS}/${id}/clip.mp4`;
