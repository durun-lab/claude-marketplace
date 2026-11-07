import { useCallback, useEffect, useState } from "preact/hooks";
import type { CommentItem } from "../api/types.ts";
import type { Controller } from "../controller.ts";
import type { LinkState } from "../connection.ts";
import { useStore } from "../hooks.ts";
import type { PendingComment } from "../outbox/outbox.ts";

const pendingLabel = (c: PendingComment, link: LinkState): string => {
  if (c.error) return `Rejected: ${c.error}`;
  return link === "online" ? "Sending" : "Waiting to send";
};

const Pending = ({ c, link, onDelete }: { c: PendingComment; link: LinkState; onDelete: () => void }) => (
  <li class={`item pending${c.error ? " rejected" : ""}`}>
    <p>{c.text}</p>
    <span class="meta">
      {pendingLabel(c, link)}
      <button type="button" class="link" onClick={onDelete}>
        Delete
      </button>
    </span>
  </li>
);

const Sent = ({ c }: { c: CommentItem }) => (
  <li class="item">
    <p>{c.text}</p>
    <span class="meta">
      <span class={`state ${c.state}`}>{c.state}</span> {c.author}
    </span>
  </li>
);

/** 母艦のコメントの一覧。つながったとき、送信待ちが減ったときに読み直す。 */
const useComments = (controller: Controller, link: LinkState, pending: number): CommentItem[] => {
  const [items, setItems] = useState<CommentItem[]>([]);
  const load = useCallback(() => {
    controller.listComments().then(setItems, () => undefined);
  }, [controller]);
  useEffect(() => {
    if (link === "online") load();
  }, [link, pending, load]);
  return items;
};

/** コメントの一覧と、親指の届く下端の入力欄。通信できない間の入力は送信待ちの列に入る。 */
export const CommentsScreen = ({ controller }: { controller: Controller }) => {
  const { outbox, connection } = controller;
  const link = useStore(connection.subscribe, connection.get).state;
  const pending = useStore(outbox.subscribe, outbox.list);
  const items = useComments(controller, link, pending.length);
  const [text, setText] = useState("");
  const send = (e: Event): void => {
    e.preventDefault();
    const body = text.trim();
    if (!body) return;
    setText("");
    void controller.comment(body);
  };
  return (
    <section class="screen comments">
      <ul class="list">
        {pending.map((c) => (
          <Pending key={c.client_id} c={c} link={link} onDelete={() => outbox.remove(c.client_id)} />
        ))}
        {items.map((c) => (
          <Sent key={c.id} c={c} />
        ))}
        {pending.length === 0 && items.length === 0 && <li class="empty">No comments yet.</li>}
      </ul>
      <form class="composer" onSubmit={send}>
        <input
          value={text}
          onInput={(e) => setText(e.currentTarget.value)}
          placeholder="Write a comment"
          enterkeyhint="send"
          aria-label="Comment"
        />
        <button type="submit" class="primary" disabled={text.trim() === ""}>
          Send
        </button>
      </form>
    </section>
  );
};
