import { useCallback, useEffect, useState } from "preact/hooks";
import type { CommentItem, CommentState } from "../api/types.ts";
import type { Controller } from "../controller.ts";
import { useStore } from "../hooks.ts";
import { applyCommentEvent } from "./commentsView.ts";
import { listComments, replyToComment, setCommentState } from "./commentsApi.ts";

const replaceItem = (items: CommentItem[], next: CommentItem): CommentItem[] =>
  items.map((c) => (c.id === next.id ? next : c));

/**
 * 母艦のコメントの一覧。つながったとき、送信待ちが減ったとき、SSE の `comment` で更新する。
 * 返信と状態は、母艦が返した最新のコメントで置き換える。
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
        setItems((now) => {
          const patched = applyCommentEvent(now, event);
          if (!patched) load();
          return patched ?? now;
        });
      }),
    [connection, load],
  );
  const run = useCallback(async (call: () => Promise<CommentItem>): Promise<void> => {
    try {
      const next = await call();
      setError(null);
      setItems((now) => replaceItem(now, next));
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
