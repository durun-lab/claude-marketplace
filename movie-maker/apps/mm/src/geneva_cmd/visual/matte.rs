//! クリップの画像のマスク（geneva の `mask.asset`）を、輝度のトラックマットにする。
//!
//! geneva は画像の輝度（線形の光の Rec.709 の係数）とアルファの積を被覆率にし、画像をクリップの矩形
//! （切り抜いた後の素材の画素）に引き伸ばす。マスクはクリップの変形と一緒に動く。movie-maker では、画像のレイヤーを
//! クリップの矩形に重なる変形で置き、切り抜くレイヤーの `track_matte` から輝度（`luma`）で指す。
//! マットのレイヤーはレンダラーが合成から外すので、置く順は切り抜くレイヤーのすぐ上にする（After Effects と同じ並び）。

use std::collections::HashMap;

use mm_doc::{Keyframe, Layer, MatteKind, Node, Param, TrackMatte, Transform};
use mm_types::{Id, ParamValue, RationalTime, Result};
use serde_json::Value;

use super::{FIT_NONE, FIT_PARAM};
use crate::geneva_cmd::media::Media;
use crate::geneva_cmd::report::{Gap, Report};

/// マットの画像のノードの種類と、素材のパラメータ（`mm-nodes-basic` の image）。
const IMAGE_KIND: &str = "image";
const ASSET_PARAM: &str = "asset";
const KIND_VERSION: u32 = 1;

/// クリップに画像のマスクがあれば、切り抜くレイヤーにトラックマットを付け、マットのレイヤーを後ろに足す。
/// 画像の大きさか素材の大きさが分からないマスクは数えて除く。
pub fn split(
    media: &HashMap<String, Media>,
    report: &mut Report,
    clip: &Value,
    mut layer: Layer,
) -> Result<Vec<Layer>> {
    let Some(mask) = clip.get("mask").filter(|m| image_mask(m)) else {
        return Ok(vec![layer]);
    };
    let asset = mask.get(ASSET_PARAM).and_then(Value::as_str).unwrap_or("");
    let size_of = |id: &str| media.get(id).and_then(|m| m.size).map(|s| s.map(f64::from));
    let source = layer
        .source
        .params
        .get(ASSET_PARAM)
        .and_then(|p| p.value.as_asset())
        .map(|id| id.as_str().to_owned());
    let (Some(image), Some(src)) = (size_of(asset), source.as_deref().and_then(size_of)) else {
        report.add(Gap::Mask);
        return Ok(vec![layer]);
    };
    let Some(transform) = box_transform(&layer, src, image) else {
        report.add(Gap::Mask);
        return Ok(vec![layer]);
    };
    let id = format!("{}-matte", layer.id);
    let node = Node::new(IMAGE_KIND, KIND_VERSION)
        .with_param(ASSET_PARAM, ParamValue::AssetRef(Id::new(asset)))
        .with_param(FIT_PARAM, ParamValue::Text(FIT_NONE.to_owned()));
    let mut matte = Layer::new(
        id.clone(),
        node,
        layer.start,
        RationalTime::ZERO,
        layer.duration()?,
    );
    matte.transform = transform;
    layer.track_matte = Some(TrackMatte {
        layer: Id::new(id),
        kind: MatteKind::Luma,
        inverted: mask.get("invert").and_then(Value::as_bool).unwrap_or(false),
    });
    Ok(vec![layer, matte])
}

/// `mask` が画像のマスクか。形のマスク（`shape::layer_mask`）が除いた残りのうち、トラックマットで描けるもの。
pub fn image_mask(mask: &Value) -> bool {
    mask.get(ASSET_PARAM)
        .and_then(Value::as_str)
        .is_some_and(|a| !a.is_empty())
}

/// 画像（`image` の画素）をクリップの矩形に引き伸ばして、切り抜くレイヤーと同じく動かす変形。
/// 切り抜くレイヤーはアンカーが原点なので、画像の点 q は `position + scale * (box.xy + q * box.wh / image)` に行く。
/// 位置と拡大のキーフレームが同じ時刻に並んでいなければ `None`。
fn box_transform(layer: &Layer, src: [f64; 2], image: [f64; 2]) -> Option<Transform> {
    let c = &layer.crop;
    let side = |p: &Option<Param>| p.as_ref().and_then(|p| p.value.as_number()).unwrap_or(0.0);
    let (left, top) = (side(&c.left), side(&c.top));
    let size = [
        src[0] - left - side(&c.right),
        src[1] - top - side(&c.bottom),
    ];
    let k = [size[0] / image[0], size[1] / image[1]];
    let map = |p: [f64; 2], s: [f64; 2]| {
        (
            [p[0] + s[0] * left, p[1] + s[1] * top],
            [s[0] * k[0], s[1] * k[1]],
        )
    };
    let t = &layer.transform;
    let (pos, scl) = (&t.position, &t.scale);
    let (p0, s0) = map(vec2(&pos.value)?, vec2(&scl.value)?);
    let mut out = Transform {
        position: Param::constant(ParamValue::Vec2(p0)),
        scale: Param::constant(ParamValue::Vec2(s0)),
        anchor: Some(Param::constant(ParamValue::Vec2([0.0, 0.0]))),
        ..Transform::default()
    };
    if !pos.is_animated() && !scl.is_animated() {
        return Some(out);
    }
    if pos.keyframes.len() != scl.keyframes.len() {
        return None;
    }
    for (pk, sk) in pos.keyframes.iter().zip(&scl.keyframes) {
        if pk.time != sk.time {
            return None;
        }
        let (p, s) = map(vec2(&pk.value)?, vec2(&sk.value)?);
        out.position.keyframes.push(Keyframe {
            value: ParamValue::Vec2(p),
            ..pk.clone()
        });
        out.scale.keyframes.push(Keyframe {
            value: ParamValue::Vec2(s),
            ..sk.clone()
        });
    }
    Some(out)
}

/// Vec2 か Number（縦横同じ倍率）の値。
fn vec2(v: &ParamValue) -> Option<[f64; 2]> {
    v.as_vec2().or_else(|| v.as_number().map(|n| [n, n]))
}

#[cfg(test)]
mod tests;
