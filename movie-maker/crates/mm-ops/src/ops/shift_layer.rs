//! `shiftLayer`。レイヤーをコンポジションの時間軸で前後に動かす（Timeline の帯のドラッグ）。

use serde::{Deserialize, Serialize};

use mm_doc::{Layer, Param, Project};
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
        for param in comp_time_params(layer) {
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

/// レイヤーのパラメータのキーフレームの時刻（コンポジションの時刻）を、前から順に重複なしで返す。
/// `shiftLayer` が動かすパラメータと同じ一覧（`comp_time_params`）を読む向きでたどる。
/// 再フレーミング（素材の時刻）とタイムリマップは含めない。
pub fn keyframe_times(layer: &Layer) -> Vec<RationalTime> {
    let mut times: Vec<RationalTime> = comp_time_params_ref(layer)
        .into_iter()
        .flat_map(|p| p.keyframes.iter().map(|k| k.time))
        .collect();
    times.sort();
    times.dedup();
    times
}

/// `comp_time_params` の読む向き。一覧を足すときは両方に足す（試験が個数を突き合わせる）。
fn comp_time_params_ref(layer: &Layer) -> Vec<&Param> {
    let mut out: Vec<&Param> = vec![
        &layer.opacity,
        &layer.transform.position,
        &layer.transform.scale,
        &layer.transform.rotation,
    ];
    out.extend(layer.transform.anchor.as_ref());
    let crop = &layer.crop;
    out.extend(
        [&crop.left, &crop.top, &crop.right, &crop.bottom]
            .into_iter()
            .flat_map(Option::as_ref),
    );
    for mask in &layer.masks {
        out.extend([&mask.path, &mask.feather, &mask.opacity]);
        if let Some(rect) = &mask.rect {
            out.extend([&rect.position, &rect.size, &rect.radius]);
        }
    }
    if let Some(audio) = &layer.audio {
        out.extend([&audio.gain_db, &audio.pan]);
    }
    if let Some(caption) = &layer.framing.caption {
        out.extend([&caption.position, &caption.width]);
    }
    let mut nodes = vec![&layer.source];
    nodes.extend(layer.effects.iter().map(|e| &e.node));
    while let Some(node) = nodes.pop() {
        out.extend(node.params.values());
        nodes.extend(node.inputs.iter());
    }
    out
}
