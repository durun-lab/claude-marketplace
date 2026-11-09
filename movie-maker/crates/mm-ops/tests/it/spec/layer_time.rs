//! レイヤーの時間と有効の操作（`shiftLayer`、`setLayerEnabled`）の種。

use super::build::layer_id;
use crate::common::secs;
use mm_doc::Project;
use mm_ops::*;
use proptest::prelude::*;

/// レイヤーの時間と有効の操作の種。
#[derive(Debug, Clone)]
pub enum TimeSpec {
    /// レイヤーの番号と、動かす秒。
    Shift(u8, i64),
    /// レイヤーの番号と、有効にするか。
    Enabled(u8, bool),
}

pub fn arb() -> impl Strategy<Value = TimeSpec> {
    prop_oneof![
        (any::<u8>(), -3i64..4).prop_map(|(a, b)| TimeSpec::Shift(a, b)),
        (any::<u8>(), any::<bool>()).prop_map(|(a, b)| TimeSpec::Enabled(a, b)),
    ]
}

pub fn op(spec: &TimeSpec, p: &Project) -> Option<Op> {
    Some(match spec {
        TimeSpec::Shift(i, by) => ShiftLayer {
            layer: layer_id(p, *i)?,
            by: secs(*by),
        }
        .into(),
        TimeSpec::Enabled(i, enabled) => SetLayerEnabled {
            layer: layer_id(p, *i)?,
            enabled: *enabled,
        }
        .into(),
    })
}
