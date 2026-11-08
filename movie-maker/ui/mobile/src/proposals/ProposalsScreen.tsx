import { useState } from "preact/hooks";
import type { Controller } from "../controller.ts";
import { useStore } from "../hooks.ts";
import { ProposalCard } from "./ProposalCard.tsx";
import { ProposalDetail } from "./ProposalDetail.tsx";
import { useProposals } from "./useProposals.ts";
import "./proposals.css";

/** Proposals のタブ。一覧（新しい順）と、1 件の画面。決めた後の 5 秒は `Undo` を下に出す。 */
export const ProposalsScreen = ({ controller, now = Date.now() }: { controller: Controller; now?: number }) => {
  const proposals = useProposals(controller);
  const project = useStore(controller.connection.subscribe, controller.connection.get).project;
  const comp = project?.open_composition ?? undefined;
  const [openId, setOpenId] = useState<number | null>(null);
  const opened = proposals.items.find((p) => p.id === openId) ?? null;
  return (
    <>
      {opened ? (
        <ProposalDetail
          key={opened.id}
          proposal={opened}
          comp={comp}
          api={controller.api}
          version={project?.version ?? 0}
          onBack={() => setOpenId(null)}
          onApprove={(ops) => proposals.approve(opened, ops)}
          onReject={() => proposals.reject(opened)}
          onReopen={() => proposals.reopen(opened)}
        />
      ) : (
        <section class="screen proposals">
          {proposals.error && <p class="error">{proposals.error}</p>}
          <ul class="list">
            {proposals.items.map((p) => (
              <ProposalCard key={p.id} p={p} now={now} onOpen={() => setOpenId(p.id)} />
            ))}
            {proposals.items.length === 0 && <li class="empty">No proposals yet.</li>}
          </ul>
        </section>
      )}
      {proposals.offer && (
        <div class="undo" role="status">
          <span>{proposals.offer.label}</span>
          <button type="button" class="link" onClick={() => void proposals.undo()}>
            Undo
          </button>
        </div>
      )}
    </>
  );
};
