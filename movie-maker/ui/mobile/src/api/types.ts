import type { RationalTime } from "@mm/sdk";

/** `GET /m/api/project` の応答。素材のパスは含まれない。 */
export interface ProjectInfo {
  version: number;
  compositions: { id: string; name: string }[];
  sequences: { id: string; name: string }[];
  open_composition: string | null;
}

/** コメントの状態。`fresh` は Claude が対応を適用した直後。 */
export type CommentState = "open" | "fresh" | "done" | "wontfix";

/** `GET /m/api/comments` の 1 件。 */
export interface CommentItem {
  id: string;
  text: string;
  state: CommentState;
  at: RationalTime | null;
  author: string;
}

/** `POST /m/api/comments` の本文。`client_id` は再送で二重に書かれないための端末側の ID。 */
export interface CommentDraft {
  client_id: string;
  text: string;
  at: RationalTime | null;
}

/** 端末の登録に成功したときの応答。資格そのものはクッキーで届き、ここには含まれない。 */
export interface PairResult {
  device_name: string;
}
