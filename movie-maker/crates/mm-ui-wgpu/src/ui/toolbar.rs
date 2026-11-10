//! `Ui` の Timeline のツールバー。状態（押せるか、Snapping の入/切、ズームの位置、名前の幅、記号のフォントの有無）を受け、
//! ポインターの入力からボタンとスライダーの操作を返す。矩形は Timeline の矩形と一緒に描く（`PaintInput::toolbar`）。

use mm_ui_layout::palette::Palette;
use mm_ui_layout::{
    toolbar_labels, Label, PointerEvent, TimeView, TimelineController, ToolbarAction,
    ToolbarController, ToolbarNames, ToolbarState, ToolbarView, UiFont, VIEW_MARGIN,
};

use super::Ui;
use crate::fonts::Shaper;

/// ツールバーの状態と、ポインター操作の状態。
#[derive(Debug, Default)]
pub(super) struct Toolbar {
    state: ToolbarState,
    controller: ToolbarController,
}

impl Toolbar {
    /// 名前の幅を文字の仕組みで測り、記号のフォントの有無を見て、Timeline `timeline` のツールバーの文字を置く。
    /// 測った幅は当たり判定と矩形も使う（文字を置いた後に描き、次の入力を受ける）。
    pub(super) fn labels(
        &mut self,
        timeline: &TimelineController,
        palette: &Palette,
        font: &UiFont,
        shaper: &mut Shaper,
    ) -> Vec<Label> {
        self.state.names = ToolbarNames::measure(font, shaper);
        self.state.symbols = shaper.has_symbol_font();
        let bar = timeline.layout().toolbar;
        toolbar_labels(bar, &self.state, &self.controller, palette, font, shaper)
    }

    /// 描く関数に渡す、ツールバーの状態。
    pub(super) fn view(&self) -> ToolbarView<'_> {
        ToolbarView {
            state: &self.state,
            controller: &self.controller,
        }
    }
}

impl Ui {
    /// ツールバーの、今は押せないコマンドの ID（コマンドの表から作ったもの）。
    pub fn set_toolbar_disabled(&mut self, disabled: Vec<&'static str>) {
        self.toolbar.state.disabled = disabled;
    }

    /// 今のツール。アプリがコマンドの表のチェックの状態に使う。
    pub fn tool(&self) -> mm_ui_layout::Tool {
        self.controller.tool
    }

    /// 最後に描いたツールバーの名前の幅。`--play-test` がボタンの位置を引く（[`mm_ui_layout::toolbar_button`]）。
    pub fn toolbar_names(&self) -> &ToolbarNames {
        &self.toolbar.state.names
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
        let Toolbar { state, controller } = &mut self.toolbar;
        controller.handle(local, bar, state)
    }

    /// ツールバーのスライダーが指す倍率（1 秒の論理ピクセル）にする。拡大の中心は再生位置（キーと同じ）。
    pub fn zoom_to(&mut self, px_per_sec: f64) {
        self.controller
            .zoom_to(px_per_sec, self.playhead, self.model.duration);
    }

    /// ツールバーの Snapping の入/切とズームの位置を、今の Timeline の状態にする。入力を受ける前と描く前に呼ぶ。
    pub(super) fn sync_toolbar(&mut self) {
        let width = self.controller.layout().row_area.w;
        let state = &mut self.toolbar.state;
        state.snapping = self.controller.snapping;
        state.tool = self.controller.tool;
        state.px_per_sec = self.controller.view.px_per_sec;
        state.floor = TimeView::zoom_floor(self.model.duration, width, VIEW_MARGIN);
    }

    /// サーフェスの論理ピクセルの点 `(x, y)` がツールバーの上か。ツールバーの上の押下は Timeline に渡さず、選択を変えない
    /// （ツールバーは [`Self::toolbar_pointer`] が受ける）。
    pub(super) fn on_toolbar(&self, x: f32, y: f32) -> bool {
        let t = self.layout.timeline;
        self.controller.layout().toolbar.contains(x - t.x, y - t.y)
    }
}

#[cfg(test)]
mod tests;
