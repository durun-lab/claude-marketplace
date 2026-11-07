// Feedback パネルをモックの API で動かすページ。スクリーンショット用で、アプリには載せない。
import { render } from "preact";
import replied from "../fixtures/replied.json";
import type { FeedbackApi, FeedbackItem, ImproveStatus } from "../src/api.ts";
import { followTheme } from "../../../shell/src/followTheme.ts";
import { FeedbackPanel } from "../src/index.ts";
import "../../../shell/src/shell.css";

/** 相対時刻を固定して、撮るたびに同じ画面にする。 */
const NOW_MS = 1793967600000;
const MINUTE_MS = 60_000;
const HOUR_MS = 3_600_000;

const LANDED = replied as FeedbackItem;

const item = (id: string, over: Partial<FeedbackItem>): FeedbackItem => ({
  ...LANDED,
  id,
  targets: [],
  status: "pending",
  replies: [],
  ...over,
});

let items: FeedbackItem[] = [
  item("f5", {
    createdAt: NOW_MS - 4 * MINUTE_MS,
    text: "Story search should also match words in the transcript topics.",
    targets: LANDED.targets,
  }),
  item("f4", {
    createdAt: NOW_MS - 2 * HOUR_MS,
    text: "The trim handles in Timeline are hard to grab at 100% zoom.",
    status: "held",
    replies: [
      {
        at: NOW_MS - HOUR_MS,
        runId: "r20261107-1003-a9",
        text: "Widened the hit area of the trim handles.",
        outcome: { kind: "pullRequest", url: "https://github.com/example/pull/1", reason: "touches crates/mm-ui-wgpu (needs approval)" },
      },
    ],
  }),
  item("f3", {
    createdAt: NOW_MS - 3 * HOUR_MS,
    kind: "slow",
    text: "preview.render_frame took 41 ms (budget 16.7 ms), 12 times.",
    replies: [
      {
        at: NOW_MS - 2 * HOUR_MS,
        runId: "r20261107-0920-c1",
        text: "Cached the LUT texture between frames.",
        outcome: { kind: "rejected", reason: "mm perf compare: playback p95 +8%" },
      },
    ],
  }),
  { ...LANDED, createdAt: NOW_MS - 5 * HOUR_MS, replies: LANDED.replies.map((r) => ({ ...r, at: NOW_MS - 4 * HOUR_MS })) },
];

let status: ImproveStatus = {
  settings: { enabled: true, model: null, effort: null, budgetUsd: 5, wallMinutes: 60, perfThreshold: 0.05 },
  running: true,
  run: { id: "r20261107-1156-e2", items: ["f5"], startedAt: NOW_MS - 3 * MINUTE_MS, phase: "agent", limitResetsAt: null },
  pending: 1,
  waiting: null,
  last: null,
};

const api: FeedbackApi = {
  status: async () => status,
  list: async (filter) => items.filter((i) => filter === "all" || (filter === "pending") === (i.status === "pending")),
  get: async (id) => items.find((i) => i.id === id) ?? LANDED,
  submit: async (f) => {
    const next = item(`f${items.length + 1}`, { createdAt: NOW_MS, text: f.text, targets: f.targets });
    items = [next, ...items];
    return next;
  },
  settings: async (patch) => (status = { ...status, settings: { ...status.settings, ...patch } }),
  stop: async () => (status = { ...status, running: false, run: null, settings: { ...status.settings, enabled: false } }),
  capture: async () => "C:/shots/demo.png",
  reportError: async () => undefined,
  onChanged: async () => () => undefined,
};

followTheme(document.documentElement, location.search, matchMedia.bind(window));
const root = document.getElementById("root");
if (root) {
  root.style.boxSizing = "border-box";
  root.style.flexDirection = "column";
  root.style.padding = "10px";
  root.style.width = "300px";
  root.style.height = "100vh";
  render(<FeedbackPanel api={api} nowMs={NOW_MS} />, root);
}
