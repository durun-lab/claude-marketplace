//! Timeline の表示の状態（ズーム、スクロール、行の高さとキーフレームの行の開閉、スナップの入/切）。開いたときの最初の表示と、
//! `.mm/view.json` との出し入れ。

use mm_ui_layout::{
    toggle_open, Action, ItemKind, KeyframeAction, RowHeights, Rows, TimeView, ViewState,
    VIEW_MARGIN,
};

use super::Ui;

impl Ui {
    /// 全体が収まる表示にする。
    pub fn fit(&mut self) {
        let w = self.controller.layout().row_area.w;
        if w > 0.0 && self.model.duration > 0.0 {
            self.controller.view = TimeView::fit(self.model.duration, w, VIEW_MARGIN);
        }
    }

    /// 開いたときに戻す表示の状態（アプリが `.mm/view.json` から読んだもの）。行の高さとスナップの入/切はすぐに、ズームと
    /// スクロールは最初に Timeline の大きさが決まったときに、全体表示と sequence の行を見せる代わりに当てる。
    pub fn restore_view(&mut self, state: ViewState) {
        self.heights = state.heights.clone();
        self.controller.snapping = state.snapping;
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
            snapping: self.controller.snapping,
        })
    }

    /// キーフレームの行の開閉（[`KeyframeAction::ToggleRows`]）を表示の状態に当て、行の並びを作り直す。1 つのレイヤーの
    /// 行を開いたら、その下のキーフレームの行が見える所までスクロールする。閉じた行で選んでいたものは外し、外したことを
    /// 操作の並びに足す（アプリが Remove Keyframe を押せないことにする）。
    pub(super) fn toggle_keyframe_rows(&mut self, actions: &mut Vec<Action>) {
        let toggles: Vec<Option<usize>> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Keyframe(KeyframeAction::ToggleRows(target)) => Some(*target),
                _ => None,
            })
            .collect();
        if toggles.is_empty() {
            return;
        }
        let picked = self.controller.keyframe_pick().is_some();
        let mut opened = None;
        for target in toggles {
            let open = &mut self.heights.keyframe_rows_open;
            let now_open = toggle_open(open, &self.model.items, target);
            opened = target.filter(|_| now_open);
        }
        self.relayout_rows();
        if let Some(i) = opened {
            self.controller.reveal_keyframe_rows(i);
        }
        if picked && self.controller.keyframe_pick().is_none() {
            actions.push(Action::Keyframe(KeyframeAction::Select(None)));
        }
    }

    /// モデルか行の高さが変わったら、行の並びと Timeline の割り付けを作り直す。
    pub(super) fn relayout_rows(&mut self) {
        self.waves.invalidate();
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
