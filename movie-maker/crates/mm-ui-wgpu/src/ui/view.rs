//! Timeline の表示の状態（ズーム、スクロール、行の高さ）。開いたときの最初の表示、`.mm/view.json` との出し入れ、
//! キーとメニューのコマンドと再生中のページ送り。

use mm_ui_layout::{
    Action, ItemKind, RowHeights, Rows, TimeView, TimelineCommand, ViewState, VIEW_MARGIN,
};

use super::Ui;

impl Ui {
    /// キーとメニューの Timeline のコマンドを受けて、起きた操作を返す（前後の帯の端へは再生位置のシーク）。
    pub fn command(&mut self, command: TimelineCommand) -> Vec<Action> {
        self.controller.command(command, &self.model, self.playhead)
    }

    /// 再生中か。再生中は、再生位置が行の領域の右端を越えたら 1 画面ぶん送る（Premiere の Page Scroll）。
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    /// 全体が収まる表示にする。
    pub fn fit(&mut self) {
        let w = self.controller.layout().row_area.w;
        if w > 0.0 && self.model.duration > 0.0 {
            self.controller.view = TimeView::fit(self.model.duration, w, VIEW_MARGIN);
        }
    }

    /// 開いたときに戻す表示の状態（アプリが `.mm/view.json` から読んだもの）。行の高さはすぐに、ズームと
    /// スクロールは最初に Timeline の大きさが決まったときに、全体表示と sequence の行を見せる代わりに当てる。
    pub fn restore_view(&mut self, state: ViewState) {
        self.heights = state.heights.clone();
        self.restore = Some(state);
        self.opened = false;
        self.relayout_rows();
    }

    /// 行の高さ（表示の状態）を置き直す。
    pub fn set_row_heights(&mut self, heights: RowHeights) {
        self.heights = heights;
        self.relayout_rows();
    }

    /// 今の表示の状態。最初の表示を当てる前は `None`（まだ戻していない既定の値を書き出さないため）。
    pub fn view_state(&self) -> Option<ViewState> {
        self.opened.then(|| ViewState {
            time: self.controller.view,
            scroll_y: self.controller.scroll_y,
            heights: self.heights.clone(),
        })
    }

    /// モデルか行の高さが変わったら、行の並びと Timeline の割り付けを作り直す。
    pub(super) fn relayout_rows(&mut self) {
        self.controller
            .set_rows(Rows::of(&self.model.items, &self.heights));
        self.resize(self.size.0 as u32, self.size.1 as u32);
    }

    /// 最初の表示。戻す表示の状態があればそれを当て、無ければ全体表示にして、sequence の行
    /// （無ければいちばん下の行）が見える所までスクロールする。Timeline の大きさが決まるまでは待つ。
    pub(super) fn open_view(&mut self) {
        let area = self.controller.layout().row_area;
        if area.w <= 0.0 || area.h <= 0.0 {
            return;
        }
        if let Some(saved) = self.restore.take() {
            self.controller.view = saved.time;
            self.controller.scroll_to(saved.scroll_y);
        } else if self.model.duration > 0.0 {
            self.fit();
            let items = &self.model.items;
            let main = items
                .iter()
                .position(|item| item.kind == ItemKind::Sequence)
                .or(items.len().checked_sub(1));
            if let Some(i) = main {
                self.controller.reveal(i);
            }
        } else {
            return;
        }
        self.opened = true;
    }
}
