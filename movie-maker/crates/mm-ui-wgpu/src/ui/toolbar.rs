//! `Ui` の Timeline のツールバー。状態（押せるか、Snapping の入/切、ズームの位置）を受け、ポインターの入力からボタンと
//! スライダーの操作を返す。矩形は Timeline の矩形と一緒に描く（`PaintInput::toolbar`）。

use mm_ui_layout::{PointerEvent, TimeView, ToolbarAction, ToolbarView, VIEW_MARGIN};

use super::Ui;

impl Ui {
    /// ツールバーの、今は押せないコマンドの ID（コマンドの表から作ったもの）。
    pub fn set_toolbar_disabled(&mut self, disabled: Vec<&'static str>) {
        self.toolbar.disabled = disabled;
    }

    /// サーフェスの座標（物理ピクセル）のポインターの入力をツールバーに渡し、起きた操作を返す。
    /// スライダーの倍率は動くたびに返るので、呼ぶ側が 1 周に 1 回だけ [`Self::zoom_to`] で使う。
    pub fn toolbar_pointer(&mut self, ev: PointerEvent) -> Vec<ToolbarAction> {
        self.sync_toolbar();
        let t = self.layout.timeline;
        let local = match self.scale.pointer_to_logical(ev) {
            PointerEvent::Down { x, y, mods } => PointerEvent::Down {
                x: x - t.x,
                y: y - t.y,
                mods,
            },
            PointerEvent::Move { x, y } => PointerEvent::Move {
                x: x - t.x,
                y: y - t.y,
            },
            other => other,
        };
        let bar = self.controller.layout().toolbar;
        self.toolbar_controller.handle(local, bar, &self.toolbar)
    }

    /// ツールバーのスライダーが指す倍率（1 秒の論理ピクセル）にする。拡大の中心は再生位置（キーと同じ）。
    pub fn zoom_to(&mut self, px_per_sec: f64) {
        self.controller
            .zoom_to(px_per_sec, self.playhead, self.model.duration);
    }

    /// ツールバーの Snapping の入/切とズームの位置を、今の Timeline の状態にする。入力を受ける前と描く前に呼ぶ。
    pub(super) fn sync_toolbar(&mut self) {
        let width = self.controller.layout().row_area.w;
        self.toolbar.snapping = self.controller.snapping;
        self.toolbar.px_per_sec = self.controller.view.px_per_sec;
        self.toolbar.floor = TimeView::zoom_floor(self.model.duration, width, VIEW_MARGIN);
    }

    /// 描く関数に渡す、ツールバーの状態。
    pub(super) fn toolbar_view(&self) -> ToolbarView<'_> {
        ToolbarView {
            state: &self.toolbar,
            controller: &self.toolbar_controller,
        }
    }
}
