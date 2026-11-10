//! 描画スレッドの、Timeline のツールバーと共有状態の行き来。ボタンをキーと同じコマンドの表で実行し、スライダーの倍率を
//! 描画の 1 周に 1 回だけ適用し、描いた名前の幅を `--play-test` のために置く。

use std::sync::Arc;

use mm_ui_layout::{PointerEvent, ToolbarAction};
use mm_ui_wgpu::Ui;

use super::transport;
use crate::win::state::Shared;

/// 描画の 1 周に溜まったスライダーの倍率。ドラッグ中は動くたびに値が来るので、最後の 1 つだけを適用して、
/// 倍率の変更で Timeline を作り直す回数を 1 周に 1 回にする。
#[derive(Debug, Default, PartialEq)]
pub(super) struct Zoom(Option<f64>);

impl Zoom {
    /// ポインターの入力を `Ui` のツールバーに渡し、ボタンのコマンドは実行し、スライダーの倍率は溜める。
    pub(super) fn input(&mut self, shared: &Arc<Shared>, ui: &mut Ui, ev: PointerEvent) {
        for action in ui.toolbar_pointer(ev) {
            match action {
                ToolbarAction::Command(id) => transport::run_command(shared, id),
                ToolbarAction::Zoom(px_per_sec) => self.0 = Some(px_per_sec),
            }
        }
    }

    /// 溜めた倍率を適用する。拡大の中心は再生位置。
    pub(super) fn apply(self, ui: &mut Ui) {
        if let Some(px_per_sec) = self.0 {
            ui.zoom_to(px_per_sec);
        }
    }
}

/// 今のツールを共有状態に置く。コマンドの表が Selection Tool、Trim Tool、Blade Tool のチェックの状態に読む。
pub(super) fn publish_tool(shared: &Shared, ui: &Ui) {
    *shared.tool.lock().expect("tool") = ui.tool();
}

/// 最後に描いたツールバーの名前の幅を共有状態に置く。名前の幅は字で変わり、ボタンの位置を決めるので、`--play-test` は
/// これでボタンの位置を引く（`Placement::toolbar_button`）。
pub(super) fn publish_names(shared: &Shared, ui: &Ui) {
    let mut slot = shared.toolbar_names.lock().expect("toolbar names");
    if *slot != *ui.toolbar_names() {
        *slot = ui.toolbar_names().clone();
    }
}
