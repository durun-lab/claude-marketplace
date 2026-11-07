import { useState } from "preact/hooks";
import type { CommentItem, CommentState } from "../api/types.ts";
import { nextStates } from "./commentsView.ts";
import { commentWhen } from "./time.ts";

export interface ThreadSheetProps {
  comment: CommentItem;
  /** 失敗の文言。次の操作で消える。 */
  error: string | null;
  onClose: () => void;
  onReply: (text: string) => Promise<void>;
  onState: (to: Exclude<CommentState, "fresh">) => Promise<void>;
}

/** コメントのスレッド。返信と、下の 3 分の 1 に `Done`、`Won't fix`、`Reopen`。 */
export const ThreadSheet = ({ comment, error, onClose, onReply, onState }: ThreadSheetProps) => {
  const [text, setText] = useState("");
  const send = (e: Event): void => {
    e.preventDefault();
    const body = text.trim();
    if (!body) return;
    setText("");
    void onReply(body);
  };
  return (
    <section class="sheet thread" aria-label="Comment thread">
      <header class="thread-head">
        <span class="when">{commentWhen(comment)}</span>
        <span class={`state ${comment.state}`}>{comment.state}</span>
        <button type="button" class="link" onClick={onClose}>
          Close
        </button>
      </header>
      <ul class="replies">
        <li>
          <b>{comment.author.name}</b>
          <p>{comment.text || "(drawing only)"}</p>
        </li>
        {comment.replies.map((r) => (
          <li key={r.id}>
            <b>{r.author.name}</b>
            <p>{r.text}</p>
          </li>
        ))}
      </ul>
      {error && <p class="error">{error}</p>}
      <form class="row reply" onSubmit={send}>
        <input value={text} onInput={(e) => setText(e.currentTarget.value)} placeholder="Reply" aria-label="Reply" enterkeyhint="send" />
        <button type="submit" class="secondary" disabled={text.trim() === ""}>
          Reply
        </button>
      </form>
      <div class="row actions">
        {nextStates(comment.state).map((s) => (
          <button key={s.to} type="button" class="primary" onClick={() => void onState(s.to)}>
            {s.label}
          </button>
        ))}
      </div>
    </section>
  );
};
