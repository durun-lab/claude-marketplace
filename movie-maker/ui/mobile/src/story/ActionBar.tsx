import type { Rating } from "@mm/sdk";

export interface ActionBarProps {
  /** 選んだ単語の数。0 のときは押せない。 */
  count: number;
  waiting: boolean;
  playing: boolean;
  /** 選んだ単語がみな消してあれば Delete は戻す操作になる。 */
  restore: boolean;
  onRate: (rating: Rating) => void;
  onTopic: () => void;
  onDelete: () => void;
  onPlay: () => void;
  /** 「PC で決める」。入れておくと、操作を適用せず提案として出す。 */
  onPc: { on: boolean; set: (on: boolean) => void };
}

const hint = (count: number, waiting: boolean): string => {
  if (count === 0) return "Tap a word to start a range";
  if (waiting) return "Tap another word to end the range";
  return `${count} words selected`;
};

/** 画面の下の棒。決める操作は親指の届く下 3 分の 1 に置き、44pt 以上の高さにする。 */
export const ActionBar = (p: ActionBarProps) => {
  const none = p.count === 0;
  return (
    <div class="st-actions" role="toolbar" aria-label="Story actions">
      <div class="st-hintrow">
        <p class="st-hint">{hint(p.count, p.waiting)}</p>
        <label class="st-pc">
          <input type="checkbox" checked={p.onPc.on} onChange={(e) => p.onPc.set(e.currentTarget.checked)} />
          Decide on PC
        </label>
      </div>
      <div class="st-buttons">
        <button type="button" class="keep" disabled={none} onClick={() => p.onRate("favorite")}>
          Keep
        </button>
        <button type="button" class="reject" disabled={none} onClick={() => p.onRate("rejected")}>
          Reject
        </button>
        <button type="button" disabled={none} onClick={() => p.onRate("unrated")}>
          Hold
        </button>
        <button type="button" disabled={none} onClick={p.onTopic}>
          Topic
        </button>
        <button type="button" class="danger" disabled={none} onClick={p.onDelete}>
          {p.restore ? "Restore" : "Delete"}
        </button>
        <button type="button" disabled={none} onClick={p.onPlay}>
          {p.playing ? "Stop" : "Play"}
        </button>
      </div>
    </div>
  );
};
