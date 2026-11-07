import { useCallback, useEffect, useState } from "preact/hooks";
import type { CommentItem, CommentState } from "../api/types.ts";
import type { Controller } from "../controller.ts";
import { useStore } from "../hooks.ts";
import { commentEventId, upsertComment } from "./commentsView.ts";
import { getComment, listComments, replyToComment, setCommentState } from "./commentsApi.ts";

/**
 * PC のコメントの一覧。つながったとき、送信待ちが減ったときに全体を、SSE の `comment` でその 1 件を読み直す。
 * 返信と状態は、PC が返した最新のコメントで置き換える。
 */
export const useComments = (controller: Controller) => {
  const { api, connection, outbox } = controller;
  const link = useStore(connection.subscribe, connection.get).state;
  const pending = useStore(outbox.subscribe, outbox.list);
  const [items, setItems] = useState<CommentItem[]>([]);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(() => {
    listComments(api).then(setItems, () => undefined);
  }, [api]);
  useEffect(() => {
    if (link === "online") load();
  }, [link, pending.length, load]);
  useEffect(
    () =>
      connection.onEvent("comment", (event) => {
        const id = commentEventId(event);
        if (!id) return load();
        getComment(api, id).then((c) => setItems((now) => upsertComment(now, c)), load);
      }),
    [api, connection, load],
  );
  const run = useCallback(async (call: () => Promise<CommentItem>): Promise<void> => {
    try {
      const next = await call();
      setError(null);
      setItems((now) => upsertComment(now, next));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);
  return {
    items,
    pending,
    link,
    error,
    reply: (id: string, text: string) => run(() => replyToComment(api, id, text)),
    setState: (id: string, state: Exclude<CommentState, "fresh">) => run(() => setCommentState(api, id, state)),
  };
};
