//! 描画スレッドの、Timeline のツールバーと共有状態の行き来。押せないコマンドを `Ui` に渡し、ボタンをキーと同じ
//! コマンドの表で実行し、スライダーの倍率を描画の 1 周に 1 回だけ適用する。

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
