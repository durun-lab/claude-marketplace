import type { CommentItem } from "../api/types.ts";
import type { LinkState } from "../connection.ts";
import type { PendingComment } from "../outbox/outbox.ts";
import { FILTERS, countByFilter, type Filter } from "./commentsView.ts";
import { commentWhen, mmss, toSeconds } from "./time.ts";

const pendingLabel = (c: PendingComment, link: LinkState): string => {
  if (c.error) return `Rejected: ${c.error}`;
  return link === "online" ? "Sending" : "Waiting to send";
};

const pendingWhen = (c: PendingComment): string => {
  if (c.range) return `${mmss(toSeconds(c.range.in))}–${mmss(toSeconds(c.range.out))}`;
  return c.at ? mmss(toSeconds(c.at)) : "";
};

const FILTER_LABEL: Record<Filter, string> = { open: "Open", fresh: "Fresh", done: "Done" };

export const FilterChips = ({ items, active, onToggle }: { items: CommentItem[]; active: readonly Filter[]; onToggle: (f: Filter) => void }) => {
  const counts = countByFilter(items);
  return (
    <div class="chips" role="group" aria-label="Filter comments">
      {FILTERS.map((f) => (
        <button key={f} type="button" class={`chipbtn${active.includes(f) ? " on" : ""}`} aria-pressed={active.includes(f)} onClick={() => onToggle(f)}>
          {FILTER_LABEL[f]} <b>{counts[f]}</b>
        </button>
      ))}
    </div>
  );
};

export const PendingItem = ({ c, link, onDelete }: { c: PendingComment; link: LinkState; onDelete: () => void }) => (
  <li class={`item pending${c.error ? " rejected" : ""}`}>
    <p>{c.text || "(drawing only)"}</p>
    <span class="meta">
      <span>
        {pendingWhen(c)} {pendingLabel(c, link)}
      </span>
      <button type="button" class="link" onClick={onDelete}>
        Delete
      </button>
    </span>
  </li>
);

export const SentItem = ({ c, selected, onOpen }: { c: CommentItem; selected: boolean; onOpen: () => void }) => (
  <li>
    <button type="button" class={`item tap${selected ? " selected" : ""}`} onClick={onOpen}>
      <span class="when">{commentWhen(c)}</span>
      <p>{c.text || "(drawing only)"}</p>
      <span class="meta">
        <span>
          <span class={`state ${c.state}`}>{c.state}</span> {c.author.name}
        </span>
        <span>{c.replies.length > 0 ? `${c.replies.length} ${c.replies.length === 1 ? "reply" : "replies"}` : ""}</span>
      </span>
    </button>
  </li>
);
