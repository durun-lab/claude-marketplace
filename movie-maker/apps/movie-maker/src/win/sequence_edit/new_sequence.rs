//! sequence のレイヤーが無いコンポジションへの最初の編集が足す、空の sequence と、それを置く sequence のレイヤー。
//! Premiere の空の Timeline（「Drop media here to create sequence」）と同じく、最初の素材から sequence を作る。
//! Insert と Overwrite（`sequence_edit.rs`）と、Story の Arrange（`story.rs`）が使う。

use mm_doc::{
    Composition, Layer, Node, Project, Sequence, SequenceId, SEQUENCE_KIND, SEQUENCE_PARAM,
};
use mm_ops::{AddLayer, AddSequence, FitLayer, Op};
use mm_types::{Id, LayerId, ParamValue, RationalTime};

/// sequence のノードの種類のバージョン（`mm-nodes-basic` の `KIND_VERSION`）。
const SEQUENCE_KIND_VERSION: u32 = 1;

/// 最初の編集が足す sequence と、それを置く sequence のレイヤーの ID。
#[derive(Debug, Clone, PartialEq)]
pub struct NewSequence {
    pub sequence: SequenceId,
    pub layer: LayerId,
}

impl NewSequence {
    /// コンポジション `comp` に足す sequence（`<コンポジションの ID>-seq`）と sequence のレイヤー（`<sequence の ID>-layer`）の ID。
    /// プロジェクトの同じ種類の ID と重なれば `-2`、`-3` と番号を付ける。
    pub fn ids(project: &Project, comp: &Composition) -> Self {
        let sequence = free_id(&format!("{}-seq", comp.id), |id| {
            project.sequence(&Id::new(id)).is_ok()
        });
        let layer = free_id(&format!("{sequence}-layer"), |id| {
            project.layer(&Id::new(id)).is_ok()
        });
        Self {
            sequence: Id::new(sequence),
            layer: Id::new(layer),
        }
    }

    /// 空の sequence と、それを置く長さ `length` の sequence のレイヤー（コンポジションの先頭から、一番上）を足し、
    /// カットを入れる操作 `fill` を続け、最後の `fitLayer` でコンポジションの長さを追従の決まりに合わせる列。
    /// `length` は `fill` が入れるカットの長さの和にする。レイヤーの長さが sequence の長さと同じなので、後の編集にも追従する。
    /// 新しいレイヤーは操作の前のデータに無く、`with_fitted_layers` が `fitLayer` を足さないので、列に入れておく。
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
        ops.push(
            FitLayer {
                layer: self.layer.clone(),
                out_point: None,
                duration: None,
            }
            .into(),
        );
        ops
    }
}

/// `base` が空いていればそれ、空いていなければ `<base>-2`、`<base>-3` と空くまで番号を進めた ID。
fn free_id(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut id = base.to_owned();
    let mut n = 1;
    while taken(&id) {
        n += 1;
        id = format!("{base}-{n}");
    }
    id
}

#[cfg(test)]
mod tests;
