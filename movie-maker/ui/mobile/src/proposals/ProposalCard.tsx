import { describeScope, relativeTime } from "@mm/review/src/model.ts";
import type { Proposal } from "./api.ts";
import { stateLabel } from "./view.ts";

const plural = (n: number): string => `${n} op${n === 1 ? "" : "s"}`;

/** 一覧の 1 枚。だれが、名前、操作の数、触る範囲、後の編集と重なるか。押すと開く。 */
export const ProposalCard = ({ p, now, onOpen }: { p: Proposal; now: number; onOpen: () => void }) => (
  <li>
    <button type="button" class="item tap" onClick={onOpen}>
      <span class="meta">
        <b>{p.actor_name}</b>
        <span>{relativeTime(p.at_ms, now)}</span>
      </span>
      <p>{p.name}</p>
      <span class="meta">
        <span>
          {plural(p.ops.length)} · {describeScope(p.scope)[0] ?? "No range"}
        </span>
        <span class={`pill ${p.state}${(p.conflict?.length ?? 0) > 0 ? " conflict" : ""}`}>{stateLabel(p)}</span>
      </span>
    </button>
  </li>
);
