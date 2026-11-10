//! 空の状態の案内のどれを出すかを、プロジェクトのデータから決める。

use mm_doc::{Composition, Project};
use mm_ui_layout::EmptyState;

use crate::model::layer_sequence;

/// 空の状態の案内のどれを出すか。コンポジションの中身で決め、Timeline の行の見え方（Shy で隠した行）では決めない。
/// 中身が無いのは、レイヤーが無いか、どのレイヤーもカットの無いシーケンスを置いた sequence のレイヤーのとき。
/// 無効のレイヤーとカットは中身に数える。
pub fn empty_state(project: &Project, comp: &Composition) -> EmptyState {
    let blank = comp
        .layers
        .iter()
        .all(|l| layer_sequence(project, l).is_some_and(|s| s.cuts.is_empty()));
    EmptyState {
        // 素材を取り込んだ後は、取り込みはもう次にすることではない。
        program: blank && project.assets.is_empty(),
        // Insert はカットを sequence のレイヤーに入れるので、手順はその入る先があるときだけ出す。
        timeline: blank && !comp.layers.is_empty(),
    }
}

#[cfg(test)]
mod tests;
