import type { Controller } from "../controller.ts";
import { FilterChips, PendingItem, SentItem } from "./CommentList.tsx";
import { visibleComments, type Filter } from "./commentsView.ts";
import { commentStart } from "./time.ts";
import type { useComments } from "./useComments.ts";

export interface CommentsPanelProps {
  comments: ReturnType<typeof useComments>;
  controller: Controller;
  filters: readonly Filter[];
  openId: string | null;
  onToggle: (f: Filter) => void;
  onOpen: (id: string, at: number) => void;
  onCompose: () => void;
  /** コンポジションが分かるまでは書けない（コメントは `comp` に付く）。 */
  canCompose: boolean;
}

/** 絞り込み、コメントの一覧、親指の届く下端の大きな `Comment`。 */
export const CommentsPanel = ({ comments, controller, filters, openId, onToggle, onOpen, onCompose, canCompose }: CommentsPanelProps) => {
  const shown = visibleComments(comments.items, filters);
  const empty = comments.pending.length === 0 && shown.length === 0;
  return (
    <>
      <FilterChips items={comments.items} active={filters} onToggle={onToggle} />
      <ul class="list">
        {comments.pending.map((c) => (
          <PendingItem key={c.client_id} c={c} link={comments.link} onDelete={() => controller.outbox.remove(c.client_id)} />
        ))}
        {shown.map((c) => (
          <SentItem key={c.id} c={c} selected={c.id === openId} onOpen={() => onOpen(c.id, commentStart(c))} />
        ))}
        {empty && <li class="empty">No comments here.</li>}
      </ul>
      <div class="dock">
        <button type="button" class="primary big" disabled={!canCompose} onClick={onCompose}>
          Comment
        </button>
      </div>
    </>
  );
};
