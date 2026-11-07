import type { Comment, CommentState } from "@mm/sdk";
import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import type { ReviewApi } from "./api.ts";
import { commentSeconds } from "./commentModel.ts";

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export interface CommentsState {
  comments: Comment[] | null;
  error: string | null;
  /** 再生位置をコメントの時刻（範囲は始め）へ動かす。 */
  seek: (c: Comment) => void;
  /** 返信を送る。送れたら `true`（入力欄を空にしてよい）。 */
  reply: (c: Comment, text: string) => Promise<boolean>;
  decide: (c: Comment, state: Exclude<CommentState, "fresh">) => void;
}

/** 一覧を読み、`comment` の通知（モバイルや Claude の書き込みも）のたびに読み直す。 */
const useCommentList = (api: ReviewApi, onError: (m: string) => void) => {
  const [comments, setComments] = useState<Comment[] | null>(null);
  // 通知が続けて届くと読み直しが重なる。最後に頼んだ結果だけを使う。
  const loads = useRef(0);
  const reload = useCallback(async () => {
    const id = ++loads.current;
    try {
      const next = await api.comments();
      if (id === loads.current) setComments(next);
    } catch (e) {
      if (id === loads.current) onError(message(e));
    }
  }, [api, onError]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    let off: (() => void) | null = null;
    let alive = true;
    api.onCommentsChanged(() => void reload()).then(
      (f) => {
        if (alive) off = f;
        else f();
      },
      () => undefined,
    );
    return () => {
      alive = false;
      off?.();
    };
  }, [api, reload]);
  return { comments, reload };
};

/** コメントの一覧と、再生位置の移動、返信、状態の決定。失敗の理由は `error` に残す。 */
export const useComments = (api: ReviewApi): CommentsState => {
  const [error, setError] = useState<string | null>(null);
  const { comments, reload } = useCommentList(api, setError);

  const run = async (action: () => Promise<unknown>): Promise<boolean> => {
    setError(null);
    try {
      await action();
      await reload();
      return true;
    } catch (e) {
      setError(message(e));
      await reload();
      return false;
    }
  };

  return {
    comments,
    error,
    seek: (c) => void run(() => api.seek(commentSeconds(c))),
    reply: (c, text) => run(() => api.replyComment(c.id, text)),
    decide: (c, state) => void run(() => api.setCommentState(c.id, state)),
  };
};
