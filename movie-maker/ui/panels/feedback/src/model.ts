import type { FeedbackItem, ImproveStatus, Outcome, RunOutcome } from "./api.ts";

/** コミットを見分けるのに足りる短い桁数（git の既定の短縮と同じ）。 */
const SHORT_COMMIT = 7;

/** 一覧に出す状態。保存された状態に、回の中と差し戻し（検査に通らず未対応に戻ったもの）を足した 5 つ。 */
export type Stage = "pending" | "inRun" | "landed" | "held" | "sentBack";

export const STAGE_LABEL: Record<Stage, string> = {
  pending: "Pending",
  inRun: "In run",
  landed: "Landed",
  held: "Held at PR",
  sentBack: "Sent back",
};

/** 要望の状態。未対応のうち、回に渡したものは回の中、最後の返事が検査に通らなかったなら差し戻し。 */
export const stageOf = (item: FeedbackItem, status: ImproveStatus | null): Stage => {
  if (item.status === "done") return "landed";
  if (item.status === "held") return "held";
  if (status?.run?.items.includes(item.id)) return "inRun";
  const last = item.replies.at(-1);
  return last?.outcome.kind === "rejected" ? "sentBack" : "pending";
};

/** 返事の結末の 1 行。 */
export const outcomeText = (o: Outcome | RunOutcome): string => {
  switch (o.kind) {
    case "landed":
      return `Landed ${o.commit.slice(0, SHORT_COMMIT)}`;
    case "noChange":
      return "Answered without a code change";
    case "pullRequest":
      return `Held at PR: ${o.reason}`;
    case "rejected":
      return `Sent back: ${o.reason}`;
    case "failed":
      return `Failed: ${o.reason}`;
    case "stopped":
      return "Stopped";
  }
};

const MINUTE_MS = 60_000;
const HOUR_MS = 3_600_000;
const DAY_MS = 86_400_000;

/** 相対時刻（`3m ago` など）。 */
export const ago = (at: number, now: number): string => {
  const d = Math.max(0, now - at);
  if (d < MINUTE_MS) return "just now";
  if (d < HOUR_MS) return `${Math.floor(d / MINUTE_MS)}m ago`;
  if (d < DAY_MS) return `${Math.floor(d / HOUR_MS)}h ago`;
  return `${Math.floor(d / DAY_MS)}d ago`;
};

/** 見出しの 1 行。動いている回、使用制限、待機、未対応の数の順に、いま一番知りたいものを出す。 */
export const headline = (s: ImproveStatus, now: number): string => {
  if (s.run?.limitResetsAt) return `Usage limit: resumes ${new Date(s.run.limitResetsAt).toLocaleTimeString()}`;
  if (s.run) return `Run ${s.run.id}: ${s.run.phase === "gate" ? "checking" : "working"} on ${s.run.items.length} (${ago(s.run.startedAt, now)})`;
  if (s.waiting) return `Waiting: ${s.waiting}`;
  if (!s.settings.enabled) return s.pending > 0 ? `Off. ${s.pending} pending` : "Off";
  return s.pending > 0 ? `${s.pending} pending` : "Idle";
};
