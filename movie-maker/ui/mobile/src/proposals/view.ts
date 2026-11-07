import type { Proposal } from "./api.ts";

/** 一覧は新しい順。同時刻なら ID の大きい方（後から出た提案）を先に。 */
export const sortProposals = (items: Proposal[]): Proposal[] =>
  [...items].sort((a, b) => b.at_ms - a.at_ms || b.id - a.id);

/**
 * 前後のフレームを見る時刻（秒）。触る範囲の最初の始め。範囲が無ければ 0。
 * 範囲はシーケンスの時間軸で、シーケンスのレイヤーがコンポジションの 0 秒から始まるときだけコンポジションの時刻と一致する。
 */
export const frameTime = (p: Proposal): number => p.scope.sequences[0]?.start ?? 0;

/** 承認に渡す操作の番号。全部を選んでいれば null（提案の全体）。 */
export const approvalOps = (p: Proposal, selected: ReadonlySet<number>): number[] | null =>
  selected.size === p.ops.length ? null : [...selected].sort((a, b) => a - b);

/** 承認できる状態か。保留中で、後の編集と重ならず、操作を 1 つ以上選んでいる。 */
export const canApprove = (p: Proposal, selected: ReadonlySet<number>): boolean =>
  p.state === "pending" && (p.conflict?.length ?? 0) === 0 && selected.size > 0;

/** 提案の操作のうち、承認の対象にする番号（初めは全部）。 */
export const allOps = (p: Proposal): Set<number> => new Set(p.ops.map((o) => o.index));

/** 一覧のカードの状態の文字。 */
export const stateLabel = (p: Proposal): string => {
  if (p.state === "pending") return (p.conflict?.length ?? 0) > 0 ? "Conflicts" : "Pending";
  return p.state === "approved" ? "Approved" : "Rejected";
};
