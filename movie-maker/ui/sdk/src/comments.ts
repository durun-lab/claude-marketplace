import type { RationalTime } from "./generated/ops.ts";

/** コメントの状態。`fresh` は Claude の対応（`responds_to` つきの適用か提案の承認）でだけなり、人は決められない。 */
export type CommentState = "open" | "fresh" | "done" | "wontfix";

/** 描き込みの種類。`stroke` は点の列、`rect` は対角の 2 点、`point` は 1 点。 */
export type ShapeKind = "stroke" | "rect" | "point";

/** 描き込み 1 つ。座標はコマの左上を (0, 0)、右下を (1, 1) に正規化する。 */
export type CommentShape = { kind: ShapeKind; points: [number, number][] };

/** 誰が書いたか。`kind` は操作経路の種類（`gui`、`mobile`、`mcp`、`cli`）。 */
export type CommentAuthor = { kind: string; name: string };

export type CommentReply = { id: string; author: CommentAuthor; text: string; created_ms: number };

/** コメント 1 件。`at`（時刻）か `range`（範囲）のどちらか。範囲のコメントは `at` が null。 */
export type Comment = {
  id: string;
  comp: string;
  at: RationalTime | null;
  range?: { in: RationalTime; out: RationalTime };
  text: string;
  drawing: CommentShape[];
  author: CommentAuthor;
  replies: CommentReply[];
  state: CommentState;
  created_ms: number;
  /** 端末が付けた ID。再送で二重に書かれない。 */
  client_id?: string;
  /** `fresh` にした Claude の対応のトランザクション。 */
  responded_txn?: number;
};

/** `POST /comments` の本体（デスクトップの待ち受けでは Claude は書けない。人の待ち受け `/m/api` で使う）。 */
export type NewComment = {
  comp: string;
  at?: RationalTime;
  range?: { in: RationalTime; out: RationalTime };
  text?: string;
  drawing?: CommentShape[];
  client_id?: string;
};

/** `GET /comments` のクエリ。`from` と `to`（秒。`1.5`、`3/2`）は両方渡す。 */
export type CommentQuery = { state?: CommentState; comp?: string; from?: string; to?: string };

/** `GET /events` の `comment` の中身。 */
export type CommentEvent = { id: string; state: CommentState };
