//! `Ui` の、ポインターの下にあるものと、そこから決まる手がかり（ステータスバー、ツールチップ、カーソル）。
//! UI のスレッド（`WM_SETCURSOR`、ツールチップ、右クリックのメニュー）は、描画のスレッドが [`Ui::hint`] で作った [`Hint`] を
//! 1 つの関数（アプリの `HintSlot::current`）で引く。

use mm_ui_layout::{
    status_left, status_right, tooltip_of, HintText, Pointee, PointerEvent, TooltipContent, Under,
};

use super::{Region, Ui};

/// ポインターの下にあるものと、ツールチップの中身、ポインターの位置。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hint {
    pub pointee: Pointee,
    /// ツールチップを出す部品なら、その中身。
    pub tooltip: Option<TooltipContent>,
    /// ポインターの位置（サーフェスのクライアント座標、物理ピクセル）。サーフェスの外なら `None`。
    pub at: Option<(f32, f32)>,
}

/// ステータスバーに出した文（左、右）。`--play-test` が確かめる。
pub type StatusText = (String, String);

/// ステータスバーとツールチップの状態。
#[derive(Debug, Default)]
pub(super) struct HintState {
    /// 文に使う、コマンドの表とキーの表から取った語。
    pub(super) text: HintText,
    /// 最後のポインターの位置（物理ピクセル）。ポインターが出たら `None`。
    pub(super) at: Option<(f32, f32)>,
    /// 最後に描いたステータスバーの文。
    pub(super) shown: StatusText,
}

impl Ui {
    /// ステータスバーとツールチップに使う語（アプリのコマンドの表とキーの表から作ったもの）。
    pub fn set_hint_text(&mut self, text: HintText) {
        self.hint.text = text;
    }

    /// 最後のポインターの位置を置き直す（`pointer` が全部の入力で呼ぶ）。
    pub(super) fn note_pointer(&mut self, ev: &PointerEvent) {
        match ev {
            PointerEvent::Move { x, y } | PointerEvent::Down { x, y, .. } => {
                self.hint.at = Some((*x, *y));
            }
            PointerEvent::Leave => self.hint.at = None,
            _ => {}
        }
    }

    /// ポインターの下にあるもの。ツールバーと再生の操作のバーの部品は、それぞれの当たり判定が持つ。
    pub fn pointee(&self) -> Pointee {
        let base = self.controller.pointee(&self.model, self.playhead);
        let Some((x, y)) = self.hint.at else {
            return Pointee {
                under: Under::Outside,
                ..base
            };
        };
        let under = match self.region_at(x, y) {
            Region::Timeline
                if self.on_toolbar(self.scale.to_logical(x), self.scale.to_logical(y)) =>
            {
                self.toolbar.hover().map_or(Under::Outside, Under::Toolbar)
            }
            Region::Timeline => base.under,
            Region::Source | Region::Program => self
                .transport_controller
                .hover()
                .map_or(Under::Outside, |(viewer, part)| {
                    Under::Transport(viewer, part)
                }),
            Region::Outside => Under::Outside,
        };
        Pointee { under, ..base }
    }

    /// 描画のスレッドが UI のスレッドへ渡す、ポインターの下の手がかり。
    pub fn hint(&self) -> Hint {
        let pointee = self.pointee();
        let tooltip = if pointee.dragging.is_some() {
            None
        } else {
            tooltip_of(pointee.under, &self.model)
        };
        Hint {
            pointee,
            tooltip,
            at: self.hint.at,
        }
    }

    /// ステータスバーの文（左、右）。左はポインターの下でできること、右は Snapping の入/切と今のツール。
    pub(super) fn status_strings(&self) -> StatusText {
        let left = status_left(&self.pointee(), self.model.fps, &self.hint.text);
        let right = status_right(
            self.controller.snapping,
            self.controller.tool,
            &self.hint.text,
        );
        (left, right)
    }

    /// 最後に描いたステータスバーの文。
    pub fn status_text(&self) -> &StatusText {
        &self.hint.shown
    }
}
