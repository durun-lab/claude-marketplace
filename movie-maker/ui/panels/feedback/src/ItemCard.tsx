import type { FeedbackItem, ImproveStatus, Reply } from "./api.ts";
import { ago, outcomeText, STAGE_LABEL, stageOf } from "./model.ts";

/** 自動で起票したものの見出し。人の要望は付けない。 */
const KIND_LABEL: Record<FeedbackItem["kind"], string | null> = {
  feedback: null,
  error: "Error",
  slow: "Slow",
};

const ReplyView = ({ reply, nowMs }: { reply: Reply; nowMs: number }) => (
  <li class="mm-feedback-reply" data-outcome={reply.outcome.kind}>
    <p class="mm-feedback-reply-text">{reply.text}</p>
    <p class="mm-feedback-meta">
      {outcomeText(reply.outcome)}
      {reply.outcome.kind === "pullRequest" && (
        <>
          {" "}
          <a href={reply.outcome.url} target="_blank" rel="noreferrer">
            PR
          </a>
        </>
      )}
      {` · ${ago(reply.at, nowMs)}`}
    </p>
  </li>
);

/** 要望 1 件。状態、本文、指した部品の数、回からの返事（新しいものが下）。 */
export const ItemCard = ({ item, status, nowMs }: { item: FeedbackItem; status: ImproveStatus | null; nowMs: number }) => {
  const stage = stageOf(item, status);
  const kind = KIND_LABEL[item.kind];
  return (
    <li class="mm-feedback-card" data-stage={stage}>
      <div class="mm-feedback-card-head">
        <span class="mm-feedback-badge" data-stage={stage}>
          {STAGE_LABEL[stage]}
        </span>
        {kind && <span class="mm-feedback-kind">{kind}</span>}
        <span class="mm-feedback-meta mm-feedback-when">{ago(item.createdAt, nowMs)}</span>
      </div>
      <p class="mm-feedback-card-text">{item.text}</p>
      {item.targets.length > 0 && (
        <p class="mm-feedback-meta">
          {item.targets.length === 1 ? "1 part pointed" : `${item.targets.length} parts pointed`}
          {item.targets.some((t) => t.screenshot) ? " · screenshot" : ""}
        </p>
      )}
      {item.replies.length > 0 && (
        <ul class="mm-feedback-replies" aria-label="Replies">
          {item.replies.map((r) => (
            <ReplyView key={`${r.runId}-${r.at}`} reply={r} nowMs={nowMs} />
          ))}
        </ul>
      )}
    </li>
  );
};
