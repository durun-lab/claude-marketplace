//! sequence の自動作成。sequence のレイヤーが無いコンポジションへの編集が、空の sequence と、それを置く sequence のレイヤーを足す。
//! Premiere の空の Timeline（「Drop media here to create sequence」）と同じく、最初の素材から sequence を作る。
//! Insert と Overwrite（`sequence_edit.rs`）と、Story の Arrange（`story.rs`）が使う。

use mm_doc::{
    Composition, Layer, Node, Project, Sequence, SequenceId, SEQUENCE_KIND, SEQUENCE_PARAM,
};
use mm_ops::{AddLayer, AddSequence, FitLayer, Op};
use mm_types::{Id, LayerId, ParamValue, RationalTime};

/// sequence のノードの種類のバージョン（`mm-nodes-basic` の `KIND_VERSION`）。
const SEQUENCE_KIND_VERSION: u32 = 1;
/// 自動で作る sequence の名前。Premiere の新しいシーケンス（Sequence 01）と Resolve の新しいタイムライン（Timeline 1）と同じく、後ろに番号を付ける。
const SEQUENCE_NAME: &str = "Sequence";

/// 自動で作る sequence と、それを置く sequence のレイヤーの ID。
#[derive(Debug, Clone, PartialEq)]
pub struct NewSequence {
    pub sequence: SequenceId,
    pub layer: LayerId,
}

impl NewSequence {
    /// sequence（`Sequence <番号>`）と sequence のレイヤー（`Sequence <番号> layer`）の ID。ID は Timeline の見出し、Story の並べ先、
    /// Mixer に名前として出る。番号は 1 から数え、どちらの ID もプロジェクトのほかの ID と重複しない最初のものにする。
    /// 触る所（`Touches`）は ID の種類を区別しないので、種類の違う ID とも重複させない。
    pub fn ids(project: &Project) -> Self {
        let mut n = 1;
        loop {
            let sequence = format!("{SEQUENCE_NAME} {n}");
            let layer = format!("{sequence} layer");
            if !is_used(project, &sequence) && !is_used(project, &layer) {
                return Self {
                    sequence: Id::new(sequence),
                    layer: Id::new(layer),
                };
            }
            n += 1;
        }
    }

    /// 空の sequence と、それを置く長さ `length` の sequence のレイヤー（コンポジションの先頭から、一番上）を足し、
    /// カットを入れる操作 `fill` を続け、最後の `fitLayer` でコンポジションの長さを決める並び。`length` は `fill` が入れるカットの長さの和。
    /// レイヤーの長さが sequence の長さと同じなので、後の編集にも追従する。新しいレイヤーは操作の前のデータに無く、
    /// `with_fitted_layers` が `fitLayer` を足さないので、並びに入れておく。
    pub fn ops(&self, comp: &Composition, length: RationalTime, fill: Vec<Op>) -> Vec<Op> {
        let source = Node::new(SEQUENCE_KIND, SEQUENCE_KIND_VERSION)
            .with_param(SEQUENCE_PARAM, ParamValue::Text(self.sequence.to_string()));
        let zero = RationalTime::ZERO;
        let sequence = Sequence {
            id: self.sequence.clone(),
            cuts: Vec::new(),
            origin: None,
        };
        let mut ops: Vec<Op> = vec![
            AddSequence { sequence }.into(),
            AddLayer {
                composition: comp.id.clone(),
                layer: Layer::new(self.layer.as_str(), source, zero, zero, length),
                index: None,
            }
            .into(),
        ];
        ops.extend(fill);
        // レイヤーの無いコンポジションの長さは中身から決めたものでないので、Premiere の sequence と同じく入れたカットの長さにする。
        // ほかのレイヤーがあれば、その置き場所が長さに依るので、追従の決まり（伸ばすだけ）にする。
        let duration = comp.layers.is_empty().then_some(length);
        ops.push(
            FitLayer {
                layer: self.layer.clone(),
                out_point: None,
                duration,
            }
            .into(),
        );
        ops
    }
}

/// `id` が、プロジェクトのコンポジション、レイヤー、sequence、カットのどれかの ID か。
fn is_used(project: &Project, id: &str) -> bool {
    let in_comps = project
        .compositions
        .iter()
        .any(|c| c.id.as_str() == id || c.layers.iter().any(|l| l.id.as_str() == id));
    in_comps
        || project
            .sequences
            .iter()
            .any(|s| s.id.as_str() == id || s.cuts.iter().any(|c| c.id.as_str() == id))
}

#[cfg(test)]
mod tests;
