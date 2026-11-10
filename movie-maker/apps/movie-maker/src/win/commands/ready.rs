//! コマンドを押せるかとチェックの状態を決める今の状態。表を作るたびに 1 度だけ読む。

use mm_doc::SEQUENCE_KIND;
use mm_ui_layout::TimelineCommand;

use super::{shy, Action, Owner, Spec, State};
use crate::win::edit_actions::EditAction;
use crate::win::state::Shared;

/// 押せるかを決める今の状態。
pub(super) struct Ready {
    /// Source に素材を開いている。イン点・アウト点と insert、overwrite は Source の範囲を使う。
    source_open: bool,
    /// 開いているコンポジションにシーケンスのレイヤーがある。split、insert、overwrite はそのカットを編集する。
    sequence: bool,
    /// Timeline でカットを選んでいる。lift と extract は選んだカットを抜く。
    cut_selected: bool,
    /// Timeline のキーフレームの行でキーフレームを選んでいる。Remove Keyframe はそのキーフレームを外す。
    keyframe_selected: bool,
    /// スナップの入/切（描画スレッドが置いた表示の状態）。
    snapping: bool,
    /// Shy の行を隠すかどうか（表示の状態）と、選んでいるレイヤーの Shy のフラグ。
    shy: shy::ShyReady,
}

impl Ready {
    pub(super) fn of(shared: &Shared) -> Self {
        let sequence = shared.doc().is_some_and(|doc| {
            doc.project
                .composition(&doc.comp)
                .is_ok_and(|c| c.layers.iter().any(|l| l.source.kind == SEQUENCE_KIND))
        });
        Self {
            source_open: shared.source.handle().is_some(),
            sequence,
            cut_selected: shared.edit.selected_cut().is_some(),
            keyframe_selected: shared.keyframes.keyframe_selected(),
            snapping: shared.view.snapping(),
            shy: shy::ShyReady::of(shared),
        }
    }

    pub(super) fn state(&self, spec: &Spec) -> State {
        use EditAction::{ClearInOut, Extract, Insert, Lift, MarkIn, MarkOut, Overwrite, Split};
        let enabled = match spec.owner {
            Owner::App(Action::Edit(ClearInOut | MarkIn | MarkOut)) => self.source_open,
            Owner::App(Action::Edit(Insert | Overwrite)) => self.source_open && self.sequence,
            Owner::App(Action::Edit(Split)) => self.sequence,
            Owner::App(Action::Edit(Lift | Extract)) => self.cut_selected,
            Owner::App(Action::Shy(command)) => self.shy.enabled(command),
            Owner::App(Action::Timeline(TimelineCommand::RemoveKeyframe)) => self.keyframe_selected,
            Owner::App(_) | Owner::Panel(_) => true,
        };
        let checked = match spec.owner {
            Owner::App(Action::Shy(command)) => self.shy.checked(command),
            Owner::App(Action::Timeline(TimelineCommand::ToggleSnapping)) => Some(self.snapping),
            Owner::App(_) | Owner::Panel(_) => None,
        };
        State { enabled, checked }
    }
}
