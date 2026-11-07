//! レイヤーのマスクを足す・外す操作の種。

use super::build::{arb_id, layer_id, pick, varied_mask};
use mm_doc::{MaskId, MaskRect, Project};
use mm_ops::*;
use proptest::prelude::*;

/// マスクの操作の種。
#[derive(Debug, Clone)]
pub enum MaskSpec {
    /// レイヤーの番号、マスクの ID、差し込む位置（大きければ末尾）、角の半径、組み合わせ方・反転・ぼかしの種。
    Add(u8, String, u8, u8, u8),
    /// プロジェクトの中のマスクの番号。
    Remove(u8),
}

pub fn arb() -> impl Strategy<Value = MaskSpec> {
    let s = || any::<u8>();
    prop_oneof![
        (s(), arb_id(), s(), s(), s()).prop_map(|(l, id, at, r, v)| MaskSpec::Add(l, id, at, r, v)),
        s().prop_map(MaskSpec::Remove),
    ]
}

pub fn op(spec: &MaskSpec, p: &Project) -> Option<Op> {
    Some(match spec {
        MaskSpec::Add(i, id, at, radius, variant) => {
            let layer = layer_id(p, *i)?;
            let count = p.layer(&layer).ok()?.1.masks.len();
            let rect = MaskRect::new([0.0, 0.0], [32.0, 18.0], f64::from(*radius % 10));
            AddMask {
                layer,
                mask: varied_mask(format!("{id}-mask"), rect, *variant),
                index: (usize::from(*at) <= count).then_some(usize::from(*at)),
            }
            .into()
        }
        MaskSpec::Remove(i) => {
            let masks: Vec<MaskId> = p
                .compositions
                .iter()
                .flat_map(|c| c.layers.iter())
                .flat_map(|l| l.masks.iter().map(|m| m.id.clone()))
                .collect();
            RemoveMask {
                mask: pick(&masks, *i)?.clone(),
            }
            .into()
        }
    })
}
