import type { Comment } from "@mm/sdk";
import { useState } from "preact/hooks";
import {
  authorLabel,
  COMMENT_FILTERS,
  commentTimeLabel,
  drawingLabel,
  FILTER_LABELS,
  filterCounts,
  STATE_LABELS,
  visibleComments,
  type CommentFilter,
} from "./commentModel.ts";
import { relativeTime } from "./model.ts";
import type { CommentsState } from "./useComments.ts";

interface CardProps {
  comment: Comment;
  state: CommentsState;
  nowMs: number;
}

const Replies = ({ c, nowMs }: { c: Comment; nowMs: number }) =>
  c.replies.length === 0 ? null : (
    <ul class="mm-review-replies" aria-label="Replies">
      {c.replies.map((r) => (
        <li key={r.id} class="mm-review-reply">
          <span class="mm-review-meta">
            {authorLabel(r.author)} · {relativeTime(r.created_ms, nowMs)}
          </span>
          <span class="mm-review-comment-text">{r.text}</span>
        </li>
      ))}
    </ul>
  );

const ReplyBox = ({ c, state, onDone }: { c: Comment; state: CommentsState; onDone: () => void }) => {
  const [text, setText] = useState("");
  const send = async () => {
    const trimmed = text.trim();
    if (trimmed === "") return;
    if (await state.reply(c, trimmed)) {
      setText("");
      onDone();
    }
  };
  return (
    <div class="mm-review-replybox">
      <textarea
        class="mm-review-input"
        aria-label="Reply"
        rows={2}
        value={text}
        onInput={(e) => setText(e.currentTarget.value)}
      />
      <button type="button" class="mm-review-button mm-review-primary" disabled={text.trim() === ""} onClick={() => void send()}>
        Send
      </button>
    </div>
  );
};

const Decisions = ({ c, state }: { c: Comment; state: CommentsState }) =>
  c.state === "open" || c.state === "fresh" ? (
    <>
      <button type="button" class="mm-review-button mm-review-primary" onClick={() => state.decide(c, "done")}>
        Done
      </button>
      <button type="button" class="mm-review-button" onClick={() => state.decide(c, "wontfix")}>
        Won't fix
      </button>
    </>
  ) : (
    <button type="button" class="mm-review-button" onClick={() => state.decide(c, "open")}>
      Reopen
    </button>
  );

const CommentCard = ({ comment: c, state, nowMs }: CardProps) => {
  const [replying, setReplying] = useState(false);
  const drawing = drawingLabel(c);
  const focused = state.focused === c.id;
  return (
    <li
      class="mm-review-card"
      data-state={c.state}
      data-comment={c.id}
      data-focused={focused ? "" : undefined}
      aria-current={focused ? "true" : undefined}
    >
      <header class="mm-review-head">
        <div class="mm-review-comment-top">
          <button
            type="button"
            class="mm-review-time"
            aria-label={`Seek to ${commentTimeLabel(c)}`}
            onClick={() => state.seek(c)}
          >
            {commentTimeLabel(c)}
          </button>
          <span class="mm-review-badge" data-state={c.state}>
            {STATE_LABELS[c.state]}
          </span>
        </div>
        <div class="mm-review-meta">
          {authorLabel(c.author)} · {relativeTime(c.created_ms, nowMs)}
        </div>
      </header>
      {c.text !== "" && <p class="mm-review-comment-text">{c.text}</p>}
      {drawing && <p class="mm-review-drawing">{drawing}</p>}
      <Replies c={c} nowMs={nowMs} />
      {replying && <ReplyBox c={c} state={state} onDone={() => setReplying(false)} />}
      <div class="mm-review-actions">
        <button type="button" class="mm-review-button" aria-expanded={replying} onClick={() => setReplying(!replying)}>
          Reply
        </button>
        <Decisions c={c} state={state} />
      </div>
    </li>
  );
};

const Filters = ({
  filter,
  counts,
  onChange,
}: {
  filter: CommentFilter;
  counts: Record<CommentFilter, number>;
  onChange: (f: CommentFilter) => void;
}) => (
  <div class="mm-review-tabs" role="group" aria-label="Filter comments">
    {COMMENT_FILTERS.map((f) => (
      <button key={f} type="button" class="mm-review-seg" aria-pressed={filter === f} onClick={() => onChange(f)}>
        {FILTER_LABELS[f]} {counts[f]}
      </button>
    ))}
  </div>
);

/** コメントの一覧。`Fresh`（Claude が対応済みで確認待ち）を先頭に、時刻を押すとそのコメントを選んで再生位置が動き、描き込みが Program に重なる。 */
export const CommentsView = ({ state, nowMs }: { state: CommentsState; nowMs: number }) => {
  const [filter, setFilter] = useState<CommentFilter>("all");
  const all = state.comments ?? [];
  const shown = visibleComments(all, filter);
  return (
    <div class="mm-review-comments">
      {state.error && (
        <p class="mm-review-error" role="alert">
          {state.error}
        </p>
      )}
      <Filters filter={filter} counts={filterCounts(all)} onChange={setFilter} />
      {state.comments && shown.length === 0 && <p class="mm-review-empty">No comments here.</p>}
      {shown.length > 0 && (
        <ul class="mm-review-list" aria-label="Comments">
          {shown.map((c) => (
            <CommentCard key={c.id} comment={c} state={state} nowMs={nowMs} />
          ))}
        </ul>
      )}
    </div>
  );
};
