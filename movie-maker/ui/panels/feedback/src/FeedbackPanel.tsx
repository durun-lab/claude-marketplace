import type { FeedbackApi, ListFilter } from "./api.ts";
import { Composer } from "./Composer.tsx";
import { ItemCard } from "./ItemCard.tsx";
import { SettingsBar } from "./SettingsBar.tsx";
import { useFeedback } from "./useFeedback.ts";

export interface FeedbackPanelProps {
  api: FeedbackApi;
  /** 増えるたびに読み直す（タブを開いたとき）。 */
  refreshKey?: number;
  /** 相対時刻の基準（ミリ秒）。無ければ現在時刻。スクリーンショットとテストで固定する。 */
  nowMs?: number;
}

const FILTERS: { id: ListFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "pending", label: "Pending" },
  { id: "done", label: "Done" },
];

/** 改善要望の受け付け。上から、自動の改善の設定と状態、要望の入力、要望の一覧と返事。 */
export const FeedbackPanel = ({ api, refreshKey = 0, nowMs }: FeedbackPanelProps) => {
  const fb = useFeedback(api, refreshKey);
  const now = nowMs ?? Date.now();
  return (
    <section class="mm-feedback" aria-label="Feedback">
      {fb.error && (
        <p class="mm-feedback-error" role="alert">
          {fb.error}
        </p>
      )}
      {fb.status && <SettingsBar status={fb.status} busy={fb.busy} nowMs={now} onSettings={(p) => void fb.settings(p)} onStop={() => void fb.stop()} />}
      <Composer busy={fb.busy} capture={api.capture} onSubmit={fb.submit} />
      <div class="mm-feedback-filters" role="group" aria-label="Filter">
        {FILTERS.map((f) => (
          <button key={f.id} type="button" class="mm-feedback-seg" aria-pressed={fb.filter === f.id} onClick={() => fb.setFilter(f.id)}>
            {f.label}
          </button>
        ))}
      </div>
      {fb.items && fb.items.length === 0 && <p class="mm-feedback-empty">No feedback yet.</p>}
      {fb.items && fb.items.length > 0 && (
        <ul class="mm-feedback-list" aria-label="Feedback items">
          {fb.items.map((item) => (
            <ItemCard key={item.id} item={item} status={fb.status} nowMs={now} />
          ))}
        </ul>
      )}
    </section>
  );
};
