import { useState } from "preact/hooks";
import type { ReviewApi } from "./api.ts";
import { filterCounts } from "./commentModel.ts";
import { CommentsView } from "./CommentsView.tsx";
import { DecidedCard, PendingCard } from "./ProposalCard.tsx";
import { useComments } from "./useComments.ts";
import { useReview, type ReviewState } from "./useReview.ts";

export interface ReviewPanelProps {
  api: ReviewApi;
  /** 増えるたびに一覧を読み直す（`mm:doc-changed` が届いたとき）。 */
  refreshKey?: number;
  /** 見えているか。隠れている間は Program を元のフレームに戻す。 */
  active?: boolean;
  /** 相対時刻の基準（ミリ秒）。無ければ現在時刻。スクリーンショットとテストで固定する。 */
  nowMs?: number;
}

type Tab = "proposals" | "comments";

const Proposals = ({ review, nowMs }: { review: ReviewState; nowMs: number }) => {
  const [showDecided, setShowDecided] = useState(true);
  const pending = review.proposals?.filter((p) => p.state === "pending") ?? [];
  const decided = review.proposals?.filter((p) => p.state !== "pending") ?? [];
  return (
    <>
      {review.error && (
        <p class="mm-review-error" role="alert">
          {review.error}
        </p>
      )}
      {review.proposals && review.proposals.length === 0 && <p class="mm-review-empty">No proposals from Claude yet.</p>}
      {pending.length > 0 && (
        <ul class="mm-review-list" aria-label="Pending proposals">
          {pending.map((p) => (
            <PendingCard key={p.id} proposal={p} review={review} nowMs={nowMs} />
          ))}
        </ul>
      )}
      {decided.length > 0 && (
        <div class="mm-review-decided">
          <button type="button" class="mm-review-fold" aria-expanded={showDecided} onClick={() => setShowDecided(!showDecided)}>
            <span class="mm-review-chevron" aria-hidden="true" />
            Decided ({decided.length})
          </button>
          {showDecided && (
            <ul class="mm-review-list" aria-label="Decided proposals">
              {decided.map((p) => (
                <DecidedCard key={p.id} proposal={p} review={review} nowMs={nowMs} />
              ))}
            </ul>
          )}
        </div>
      )}
    </>
  );
};

/** Claude の編集の提案（承認、却下）と、人とモバイルのコメント（返信、状態）を 1 つのパネルで扱う。 */
export const ReviewPanel = ({ api, refreshKey = 0, active = true, nowMs }: ReviewPanelProps) => {
  const review = useReview(api, refreshKey, active);
  const comments = useComments(api);
  const [tab, setTab] = useState<Tab>("proposals");
  const now = nowMs ?? Date.now();
  const pendingCount = review.proposals?.filter((p) => p.state === "pending").length ?? 0;
  const fresh = filterCounts(comments.comments ?? []).fresh;
  return (
    <section class="mm-review" aria-label="Review">
      <div class="mm-review-tabs" role="group" aria-label="Review view">
        <button type="button" class="mm-review-seg" aria-pressed={tab === "proposals"} onClick={() => setTab("proposals")}>
          Proposals {pendingCount}
        </button>
        <button type="button" class="mm-review-seg" aria-pressed={tab === "comments"} onClick={() => setTab("comments")}>
          Comments {fresh > 0 ? `${fresh} fresh` : (comments.comments?.length ?? 0)}
        </button>
      </div>
      {tab === "proposals" ? <Proposals review={review} nowMs={now} /> : <CommentsView state={comments} nowMs={now} />}
    </section>
  );
};
