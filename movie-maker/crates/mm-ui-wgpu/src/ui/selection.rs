//! `Ui` の Timeline の選択。アプリが置くレイヤーとカットの選択と、キーフレームの行で選んだもの。

use mm_ui_layout::RowPick;

use super::Ui;

impl Ui {
    /// 選択を置く（`None` で外す）。添字は `TimelineModel::items` のもの。
    pub fn select(&mut self, index: Option<usize>) {
        self.controller.selected = index.filter(|i| *i < self.model.items.len());
    }

    /// 選んだカットを置く（`None` で外す）。添字は `TimelineModel::items` とそのカットのもの。
    pub fn select_cut(&mut self, cut: Option<(usize, usize)>) {
        self.controller.selected_cut = cut.filter(|(i, c)| {
            self.model
                .items
                .get(*i)
                .is_some_and(|item| *c < item.cuts.len())
        });
    }

    /// キーフレームの行で選んだもの。モデルを作り直した後は、作り直したモデルで引き直したもの。
    pub fn keyframe_pick(&self) -> Option<RowPick> {
        self.controller.keyframe_pick()
    }
}
