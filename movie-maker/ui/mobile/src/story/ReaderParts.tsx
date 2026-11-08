import type { StoryWord } from "@mm/story/src/api.ts";
import { ActionBar } from "./ActionBar.tsx";
import type { Actions, Notice } from "./useActions.ts";
import type { Player } from "./usePlayer.ts";

export interface FooterProps {
  actions: Actions;
  selected: StoryWord[];
  waiting: boolean;
  player: Player;
  onPlay: () => void;
  onPc: { on: boolean; set: (on: boolean) => void };
}

/** 結果の知らせ、「PC で決める」、決める操作の棒。親指の届く下端にまとめる。 */
export const Footer = ({ actions, selected, waiting, player, onPlay, onPc }: FooterProps) => (
  <>
    {actions.notice && <NoticeBar notice={actions.notice} onUndo={() => void actions.undo()} onDismiss={actions.dismiss} />}
    <ActionBar
      count={selected.length}
      waiting={waiting}
      playing={player.playing}
      restore={selected.length > 0 && selected.every((w) => w.deleted)}
      onRate={(r) => void actions.rate(r)}
      onTopic={() => void actions.makeTopic()}
      onDelete={() => void actions.deleteWords()}
      onPlay={onPlay}
      onPc={onPc}
    />
    <audio ref={player.ref} preload="none" />
  </>
);

/** 上の行。一覧へ戻る、素材の名前、話題の並べ替えを開く。 */
export const TopBar = ({ name, canArrange, onBack, onArrange }: { name: string; canArrange: boolean; onBack: () => void; onArrange: () => void }) => (
  <div class="st-top">
    <button type="button" class="st-plain" onClick={onBack}>
      Clips
    </button>
    <strong class="st-name">{name}</strong>
    <button type="button" class="st-plain" disabled={!canArrange} onClick={onArrange}>
      Arrange
    </button>
  </div>
);

/** 結果の知らせ。確認の画面は出さず、成功の知らせに Undo を添える。 */
export const NoticeBar = ({ notice, onUndo, onDismiss }: { notice: Notice; onUndo: () => void; onDismiss: () => void }) => (
  <div class="st-notice" data-tone={notice.tone} role="status">
    <span>{notice.text}</span>
    {notice.undo && (
      <button type="button" onClick={onUndo}>
        Undo
      </button>
    )}
    <button type="button" aria-label="Dismiss" onClick={onDismiss}>
      ×
    </button>
  </div>
);

