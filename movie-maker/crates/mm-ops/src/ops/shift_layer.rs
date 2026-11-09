//! `shiftLayer`。レイヤーをコンポジションの時間軸で前後に動かす（Timeline の帯のドラッグ）。

use serde::{Deserialize, Serialize};

use mm_doc::Project;
use mm_types::{LayerId, RationalTime, Result};

use crate::op::{Apply, Op};
use crate::ops::negate;
use crate::touches::Touches;

/// レイヤー `layer` の始まりと、コンポジションの時刻で評価するキーフレームを `by` だけ動かす（After Effects と同じ）。
/// 素材の時刻のもの（イン点とアウト点、再フレーミング）と、レイヤーの始まりからの時刻のタイムリマップは変えない。
/// 始まりが 0 より前になる動かし方も受ける。`trimLayer` も 0 より前の始まりを作り、拒否すると逆操作が通らないため。
/// 接続（`attach_to`）は変えず、接続先のカットとの位置の差が `by` だけ変わる。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShiftLayer {
    pub layer: LayerId,
    pub by: RationalTime,
}

impl Apply for ShiftLayer {
    fn touches(&self, _: &Project) -> Result<Touches> {
        Ok(Touches::of(&self.layer))
    }

    fn apply(&self, project: &mut Project) -> Result<Op> {
        let layer = project.layer_mut(&self.layer)?;
        layer.start = layer.start.checked_add(self.by)?;
        for param in layer.comp_time_params_mut() {
            for k in &mut param.keyframes {
                k.time = k.time.checked_add(self.by)?;
            }
        }
        Ok(ShiftLayer {
            layer: self.layer.clone(),
            by: negate(self.by)?,
        }
        .into())
    }
}
