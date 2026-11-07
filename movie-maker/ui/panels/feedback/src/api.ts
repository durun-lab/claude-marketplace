import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import type { MediaInvoke, MediaListen } from "@mm/sdk";

/** 起票、設定の変更、回の終わりを知らせる Tauri のイベント名。アプリの `feedback.rs` と同じ。 */
export const FEEDBACK_CHANGED_EVENT = "mm:feedback-changed";

/** 要望の出どころ。`error` と `slow` はアプリが自動で起票したもの。 */
export type FeedbackKind = "feedback" | "error" | "slow";

/** 保存された状態。回の中かどうかは `ImproveStatus.run.items` で分かる。 */
export type FeedbackStatus = "pending" | "done" | "held";

/** 回が要望に付けた結末。 */
export type Outcome =
  | { kind: "landed"; commit: string }
  | { kind: "noChange" }
  | { kind: "pullRequest"; url: string; reason: string }
  | { kind: "rejected"; reason: string };

export interface Reply {
  at: number;
  runId: string;
  text: string;
  outcome: Outcome;
}

/** 画面で指した部品。`rect` は WebView の CSS ピクセル、`screenshot` はウィンドウ全体の PNG のパス。 */
export interface Target {
  selector: string;
  panel: string | null;
  label: string;
  rect: { x: number; y: number; width: number; height: number };
  dpr: number;
  screenshot?: string;
}

/** 保存された要望（アプリの `feedback_list` と `feedback_get`）。 */
export interface FeedbackItem {
  id: string;
  createdAt: number;
  kind: FeedbackKind;
  text: string;
  targets: Target[];
  status: FeedbackStatus;
  replies: Reply[];
  context: unknown;
}

export interface Settings {
  enabled: boolean;
  model: string | null;
  effort: string | null;
  /** 回の予算（ドル）。`null` は上限なし。 */
  budgetUsd: number | null;
  wallMinutes: number;
  perfThreshold: number;
}

export type SettingsPatch = Partial<Omit<Settings, "model" | "effort" | "budgetUsd">> & {
  model?: string;
  effort?: string;
  /** 0 以下で上限なし。 */
  budgetUsd?: number;
};

/** 回の工程。`agent` は `claude -p` が実装中、`gate` は囲いの検査と取り込み。 */
export type Phase = "agent" | "gate";

export interface RunView {
  id: string;
  items: string[];
  startedAt: number;
  phase: Phase;
  /** 使用制限が明ける時刻（ミリ秒）。 */
  limitResetsAt: number | null;
}

/** 回の結末。要望ごとの結末（`Outcome`）に、失敗と人が止めたものが加わる。 */
export type RunOutcome = Outcome | { kind: "failed"; reason: string } | { kind: "stopped" };

/** 終えた回の記録。 */
export interface RunReport {
  runId: string;
  outcome: RunOutcome;
  items: string[];
  done: string[];
  changed: string[];
  finishedAt: number;
}

/** 自動の改善の状態（アプリの `feedback_status`）。 */
export interface ImproveStatus {
  settings: Settings;
  running: boolean;
  run: RunView | null;
  pending: number;
  waiting: string | null;
  last: RunReport | null;
}

export type ListFilter = "all" | "pending" | "done";

export interface NewFeedback {
  text: string;
  targets: Target[];
  context: Record<string, unknown>;
}

/** 画面の例外（`feedback_report_error`）。出どころはアプリが画面に決める。 */
export interface WebError {
  message: string;
  stack?: string;
  where?: string;
}

/** Feedback パネルがアプリに頼むこと。テストでは差し替える。 */
export interface FeedbackApi {
  status: () => Promise<ImproveStatus>;
  list: (filter: ListFilter) => Promise<FeedbackItem[]>;
  get: (id: string) => Promise<FeedbackItem>;
  submit: (feedback: NewFeedback) => Promise<FeedbackItem>;
  settings: (patch: SettingsPatch) => Promise<ImproveStatus>;
  stop: () => Promise<ImproveStatus>;
  /** ウィンドウのスクリーンショットを撮り、置いたパスを返す。 */
  capture: () => Promise<string>;
  reportError: (error: WebError) => Promise<void>;
  onChanged: (handler: () => void) => Promise<() => void>;
}

const asError = (e: unknown): Error => (e instanceof Error ? e : new Error(String(e)));

/** Tauri の IPC で Feedback のコマンドを呼ぶ実装。コマンドの名前と引数は ui/sdk/README.md。 */
export const createIpcFeedbackApi = (call: MediaInvoke = tauriInvoke, listen: MediaListen = tauriListen): FeedbackApi => {
  const run = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    try {
      return (await call(command, args)) as T;
    } catch (e) {
      throw asError(e);
    }
  };
  return {
    status: () => run("feedback_status"),
    list: (filter) => run("feedback_list", { filter }),
    get: (id) => run("feedback_get", { id }),
    submit: (feedback) => run("feedback_submit", { feedback }),
    settings: (patch) => run("feedback_settings", { patch }),
    stop: () => run("feedback_stop"),
    capture: () => run("feedback_capture"),
    reportError: async (error) => {
      await run("feedback_report_error", { error: { source: "webview", ...error } });
    },
    onChanged: (handler) => listen(FEEDBACK_CHANGED_EVENT, () => handler()),
  };
};
