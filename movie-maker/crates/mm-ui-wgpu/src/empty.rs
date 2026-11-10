//! 空の状態の案内のどれを出すかを、プロジェクトのデータから決める。

use mm_doc::{Composition, Project};
use mm_ui_layout::EmptyState;

use crate::model::layer_sequence;

/// 空の状態の案内のどれを出すか。コンポジションの中身で決め、Timeline の行の見え方（Shy で隠した行）では決めない。
/// 中身が無いのは、レイヤーが無いか、どのレイヤーも空の sequence を置いた sequence のレイヤーのとき。
/// 無効のレイヤーは中身に数え、無効のカットは数えない（全部のカットを無効にした sequence は映らず、置いたレイヤーは
/// 1 フレームに縮むので、カットの無い sequence と同じく Insert の手順を出す）。
pub fn empty_state(project: &Project, comp: &Composition) -> EmptyState {
    let blank = comp
        .layers
        .iter()
        .all(|l| layer_sequence(project, l).is_some_and(|s| s.cuts.iter().all(|c| !c.enabled)));
    EmptyState {
        // 素材を取り込んだ後は、取り込みはもう次にすることではない。
        program: blank && project.assets.is_empty(),
        // sequence のレイヤーが無くても、最初の Insert が sequence とそのレイヤーを作るので、手順は同じ。
        timeline: blank,
    }
}

#[cfg(test)]
mod tests;
