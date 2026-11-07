import type { CommentItem } from "../src/api/types.ts";
import type { Proposal } from "../src/proposals/api.ts";

const HOUR_MS = 3_600_000;
const NOW = Date.now();

const you = { kind: "mobile", name: "iPhone" };
const claude = { kind: "mcp", name: "claude" };
const ms = (n: number) => ({ num: n, den: 1000 });

/** デモの映像（demo/media の 12 秒の HLS）の中のコメント。 */
export const demoComments = (): CommentItem[] => [
  {
    id: "k3",
    comp: "c1",
    at: ms(5500),
    text: "Zoom here feels too fast. Slow it down.",
    drawing: [
      { kind: "rect", points: [[0.18, 0.2], [0.62, 0.7]] },
      { kind: "stroke", points: [[0.66, 0.3], [0.74, 0.42], [0.8, 0.36], [0.86, 0.5]] },
    ],
    author: you,
    replies: [{ id: "r1", author: claude, text: "Proposal 2 stretches the zoom to 1.2 s.", created_ms: NOW - HOUR_MS }],
    state: "fresh",
    created_ms: NOW - 2 * HOUR_MS,
    responded_txn: 41,
  },
  { id: "k2", comp: "c1", at: ms(2000), text: "The intro cut feels late.", drawing: [], author: you, replies: [], state: "open", created_ms: NOW - 3 * HOUR_MS },
  {
    id: "k4",
    comp: "c1",
    at: null,
    range: { in: ms(8000), out: ms(11000) },
    text: "Lower the BGM under the narration.",
    drawing: [],
    author: you,
    replies: [],
    state: "open",
    created_ms: NOW - 4 * HOUR_MS,
  },
  { id: "k1", comp: "c1", at: ms(900), text: "Fix the typo in the title card.", drawing: [], author: you, replies: [], state: "done", created_ms: NOW - 9 * HOUR_MS },
];

const ripple = { op: "ripple", cut: "cut_4", edge: "out", by: ms(-400) } as const;

const zoomProposal = (): Proposal => (
  {
    id: 2,
    name: "Slow the zoom on cut 4",
    actor: "mcp",
    actor_name: "claude",
    at_ms: NOW - HOUR_MS,
    responds_to: "k3",
    base_version: 12,
    state: "pending",
    decided_by: null,
    decided_at_ms: null,
    txn_id: null,
    approved_ops: null,
    applied: false,
    undoable: false,
    conflict: null,
    ops: [
      { index: 0, op: ripple, touches: ["cut_4"] },
      { index: 1, op: { op: "slip", cut: "cut_4", by: ms(200) }, touches: ["cut_4"] },
      { index: 2, op: { op: "setParam", target: { cut: "cut_4", path: "zoom.speed" }, value: { type: "number", value: 0.6 } }, touches: ["cut_4"] },
    ],
    scope: { sequences: [{ id: "s1", cuts: ["cut_4"], start: 4, end: 8 }], layers: [], other: [] },
  }
);

const introProposal = (): Proposal => (
  {
    id: 3,
    name: "Tighten the intro",
    actor: "mcp",
    actor_name: "claude",
    at_ms: NOW - 30 * 60_000,
    responds_to: "k2",
    base_version: 12,
    state: "pending",
    decided_by: null,
    decided_at_ms: null,
    txn_id: null,
    approved_ops: null,
    applied: false,
    undoable: false,
    conflict: ["cut_1"],
    ops: [{ index: 0, op: { op: "ripple", cut: "cut_1", edge: "in", by: ms(300) }, touches: ["cut_1"] }],
    scope: { sequences: [{ id: "s1", cuts: ["cut_1"], start: 0, end: 4 }], layers: [], other: [] },
  }
);

const pausedProposal = (): Proposal => (
  {
    id: 1,
    name: "Remove the long pause",
    actor: "mcp",
    actor_name: "claude",
    at_ms: NOW - 5 * HOUR_MS,
    responds_to: null,
    base_version: 10,
    state: "approved",
    decided_by: "mobile",
    decided_at_ms: NOW - 4 * HOUR_MS,
    txn_id: 38,
    approved_ops: null,
    applied: true,
    undoable: false,
    conflict: null,
    ops: [{ index: 0, op: { op: "lift", cut: "cut_2" }, touches: ["cut_2"] }],
    scope: { sequences: [{ id: "s1", cuts: ["cut_2"], start: 8, end: 12 }], layers: [], other: [] },
  }
);

/** デモの提案。保留 2 件（1 件は複数の操作、1 件は後の編集と重なる）と、承認済み 1 件。 */
export const demoProposals = (): Proposal[] => [zoomProposal(), introProposal(), pausedProposal()];
