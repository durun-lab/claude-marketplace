import type { Comment, CommentState, NewComment } from "@mm/sdk";

export type { CommentState };

/** `GET /m/api/project` の応答。素材のパスは含まれない。 */
export interface ProjectInfo {
  version: number;
  /** `width` と `height` はコンポジションの大きさ（ピクセル）。描き込みの映像の矩形の縦横比に使う。 */
  compositions: { id: string; name: string; width?: number; height?: number }[];
  sequences: { id: string; name: string }[];
  open_composition: string | null;
  /** 開いているコンポジションのカットの境（秒）。スクラブバーの目盛りに使う。 */
  cuts?: number[];
}

/** `GET /m/api/comments` の 1 件。型は ui/sdk の `Comment`。 */
export type CommentItem = Comment;

/** `POST /m/api/comments` の本文。`client_id` は再送で二重に書かれないための端末側の ID。 */
export type CommentDraft = NewComment & { client_id: string };

/** 端末の登録に成功したときの応答。資格そのものはクッキーで届き、ここには含まれない。 */
export interface PairResult {
  device_name: string;
}
