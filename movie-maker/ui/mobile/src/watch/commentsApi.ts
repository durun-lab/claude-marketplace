import { API_ROOT, type Api } from "../api/http.ts";
import type { CommentItem, CommentState } from "../api/types.ts";

const COMMENTS = `${API_ROOT}/comments`;

/** コメントの一覧（書いた順）。 */
export const listComments = async (api: Api): Promise<CommentItem[]> =>
  (await api.get<{ comments: CommentItem[] }>(COMMENTS)).comments;

/** コメント 1 件（返信を含む）。 */
export const getComment = (api: Api, id: string): Promise<CommentItem> => api.get(`${COMMENTS}/${encodeURIComponent(id)}`);

/** スレッドに返信する。成功なら更新後のコメント。 */
export const replyToComment = (api: Api, id: string, text: string): Promise<CommentItem> =>
  api.post(`${COMMENTS}/${encodeURIComponent(id)}/replies`, { text });

/** 状態を決める。`fresh` は人からは決められないので型で外す。 */
export const setCommentState = (api: Api, id: string, state: Exclude<CommentState, "fresh">): Promise<CommentItem> =>
  api.post(`${COMMENTS}/${encodeURIComponent(id)}/state`, { state });

/** 編集した結果の HLS の親のプレイリスト。画質は hls.js（Android）と iOS のネイティブがネットワークに合わせて選ぶ。 */
export const streamUrl = (comp: string): string => `${API_ROOT}/stream/${encodeURIComponent(comp)}/index.m3u8`;
