import { useState } from "preact/hooks";
import { approveLabel, describeOp, describeScope } from "@mm/review/src/model.ts";
import type { Proposal } from "./api.ts";
import { FrameCompare } from "./FrameCompare.tsx";
import { allOps, approvalOps, canApprove } from "./view.ts";

export interface ProposalDetailProps {
  proposal: Proposal;
  /** 前後のフレームを描くコンポジション（Watch と同じ）。 */
  comp?: string;
  onBack: () => void;
  onApprove: (ops: number[] | null) => Promise<boolean>;
  onReject: () => Promise<boolean>;
  onReopen: () => Promise<boolean>;
}

const Dock = ({ p, selected, props }: { p: Proposal; selected: Set<number>; props: ProposalDetailProps }) => {
  const done = (ok: boolean): void => {
    if (ok) props.onBack();
  };
  if (p.state === "rejected") {
    return (
      <div class="dock">
        <button type="button" class="primary big" onClick={() => void props.onReopen().then(done)}>
          Reopen
        </button>
      </div>
    );
  }
  if (p.state === "approved") return null;
  return (
    <div class="dock two">
      <button type="button" class="secondary big" onClick={() => void props.onReject().then(done)}>
        Reject
      </button>
      <button
        type="button"
        class="primary big"
        disabled={!canApprove(p, selected)}
        onClick={() => void props.onApprove(approvalOps(p, selected)).then(done)}
      >
        {approveLabel(selected.size, p.ops.length)}
      </button>
    </div>
  );
};

/** 提案 1 件。前後のフレーム、操作の一覧（選べば一部だけ承認）、下の 3 分の 1 に `Reject` と `Approve`。 */
export const ProposalDetail = (props: ProposalDetailProps) => {
  const p = props.proposal;
  const [selected, setSelected] = useState(() => allOps(p));
  const toggle = (index: number): void =>
    setSelected((now) => {
      const next = new Set(now);
      if (!next.delete(index)) next.add(index);
      return next;
    });
  const pending = p.state === "pending";
  return (
    <section class="screen detail">
      <header class="thread-head">
        <button type="button" class="link back" onClick={props.onBack}>
          Back
        </button>
        <b>{p.name}</b>
      </header>
      <FrameCompare proposal={p} ops={approvalOps(p, selected)} comp={props.comp} />
      <div class="scroll">
        {(p.conflict?.length ?? 0) > 0 && <p class="error">Overlaps later edits: {p.conflict?.join(", ")}</p>}
        <ul class="ops">
          {p.ops.map((o) => (
            <li key={o.index}>
              <label>
                <input type="checkbox" checked={selected.has(o.index)} disabled={!pending} onChange={() => toggle(o.index)} />
                <span>{describeOp(o)}</span>
              </label>
            </li>
          ))}
        </ul>
        <p class="hint">{describeScope(p.scope).join(" / ")}</p>
      </div>
      <Dock p={p} selected={selected} props={props} />
    </section>
  );
};
