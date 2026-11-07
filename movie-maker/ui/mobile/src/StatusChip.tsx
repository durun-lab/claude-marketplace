import type { LinkState } from "./connection.ts";

const LABELS: Record<LinkState, string> = {
  checking: "Connecting",
  unpaired: "Not paired",
  online: "Connected",
  offline: "Offline",
};

/** 接続の状態と、送信待ちのコメントの数。 */
export const StatusChip = ({ state, pending }: { state: LinkState; pending: number }) => (
  <span class={`chip ${state}`} role="status">
    <i aria-hidden="true" />
    {LABELS[state]}
    {pending > 0 && <b>{pending} queued</b>}
  </span>
);
