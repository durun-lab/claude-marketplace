//! `shiftLayer`。レイヤーを時間の上で動かす（Timeline の帯のドラッグ）。

use serde::{Deserialize, Serialize};

use mm_doc::{Layer, Param, Project};
use mm_types::{LayerId, RationalTime, Result};

use crate::op::{Apply, Op};
use crate::ops::{invalid, negate};
use crate::touches::Touches;

/// レイヤー `layer` を、コンポジションの時間軸で `by` だけ動かす。
/// `start` と、コンポジションの時刻で持つキーフレーム（不透明度、変形、切り抜き、マスク、音、効果、ノード、字幕の置き場所）を
/// 一緒に動かし、素材のイン点とアウト点は変えない（キーフレームが元の時刻に取り残されないため）。
/// 素材の時刻で持つ再フレーミングと、レイヤーの始まりからの時刻のタイムリマップは動かさない。
/// 動かした先の `start` が負になる動かし方は拒否する。
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
        let slot = project.layer_mut(&self.layer)?;
        let start = slot.start.checked_add(self.by)?;
        if start < RationalTime::ZERO {
            return Err(invalid(format!(
                "Layer {} would start before 0",
                self.layer
            )));
        }
        // 複製の上で動かし、時刻の桁あふれがあれば元のレイヤーを書き換えずに拒否する。
        let mut moved = slot.clone();
        moved.start = start;
        for param in comp_time_params(&mut moved) {
            for k in &mut param.keyframes {
                k.time = k.time.checked_add(self.by)?;
            }
        }
        *slot = moved;
        Ok(ShiftLayer {
            layer: self.layer.clone(),
            by: negate(self.by)?,
        }
        .into())
    }
}

/// コンポジションの時刻でキーフレームを持つパラメータ（`mm-render` が `CompCx::t` で評価するもの）。
fn comp_time_params(layer: &mut Layer) -> Vec<&mut Param> {
    let mut out: Vec<&mut Param> = vec![
        &mut layer.opacity,
        &mut layer.transform.position,
        &mut layer.transform.scale,
        &mut layer.transform.rotation,
    ];
    out.extend(layer.transform.anchor.as_mut());
    let crop = &mut layer.crop;
    out.extend(
        [
            &mut crop.left,
            &mut crop.top,
            &mut crop.right,
            &mut crop.bottom,
        ]
        .into_iter()
        .flat_map(Option::as_mut),
    );
    for mask in &mut layer.masks {
        out.extend([&mut mask.path, &mut mask.feather, &mut mask.opacity]);
        if let Some(rect) = &mut mask.rect {
            out.extend([&mut rect.position, &mut rect.size, &mut rect.radius]);
        }
    }
    if let Some(audio) = &mut layer.audio {
        out.extend([&mut audio.gain_db, &mut audio.pan]);
    }
    if let Some(caption) = &mut layer.framing.caption {
        out.extend([&mut caption.position, &mut caption.width]);
    }
    let mut nodes = vec![&mut layer.source];
    nodes.extend(layer.effects.iter_mut().map(|e| &mut e.node));
    while let Some(node) = nodes.pop() {
        out.extend(node.params.values_mut());
        nodes.extend(node.inputs.iter_mut());
    }
    out
}
