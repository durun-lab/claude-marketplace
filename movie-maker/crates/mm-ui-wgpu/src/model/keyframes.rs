//! キーフレームの行の元。レイヤーのキーフレームのあるパラメータごとに、行の名前、キーフレームのコンポジションの時刻、
//! 操作の対象（パラメータのパスとパラメータの時間軸の時刻）を作る。Timeline のモデル（`Item::keyframe_rows`）と、
//! アプリが行の添字から操作を作る所（[`keyframe_rows`]）が同じ並びを見る。
//!
//! レイヤーのパラメータのキーフレームはコンポジションの時刻なのでそのまま、再フレーミング（`reframe.<カット>.*`）の
//! キーフレームは素材の時刻なので、カットとレイヤーを通してコンポジションの時刻に変換する。再フレーミングは同じ名前の
//! パラメータを全部のカットで 1 本の行にまとめ、キーフレームはそれぞれのカットの範囲に置く（カットごとに行を作ると、
//! カットの数だけ行が並ぶため）。

use std::sync::Arc;

use mm_doc::{Composition, Cut, Layer, Param, Project, Sequence, Transform};
use mm_eval::{comp_time, cut_source_time, place_cuts, source_time, PlacedCut};
use mm_types::{RationalTime, Result};
use mm_ui_layout::{KeyframeRow, RowKeyframe};

use super::{comp_secs, layer_sequence};

/// 同じ時刻とみなす差（秒）。有理数の時刻を別の計算で浮動小数にした誤差を吸収する。
const SAME_TIME: f64 = 1e-9;

/// キーフレームの行 1 本の元。
pub struct KeyframeRowSource<'a> {
    /// 行の見出しの名前（「Transform › Position」）。
    pub name: String,
    /// キーフレームを置けるパラメータと、その時間の範囲。レイヤーのパラメータは 1 つ、再フレーミングはカットごと。
    pub spans: Vec<ParamSpan<'a>>,
    /// 行のキーフレーム（コンポジションの時刻の順）。
    pub keyframes: Vec<KeyframeSource>,
}

/// キーフレームを置けるパラメータ 1 つ。
pub struct ParamSpan<'a> {
    /// パラメータのパス（`mm-ops` の `ParamRef` の `path`）。
    pub path: String,
    pub param: &'a Param,
    /// 再フレーミングのカットと、その sequence の中の配置。レイヤーのパラメータは `None`。
    pub cut: Option<(&'a Cut, PlacedCut)>,
    /// コンポジションの時刻の範囲 `[start, end)`（秒）。
    pub start: f64,
    pub end: f64,
}

/// 行のキーフレーム 1 つ。
pub struct KeyframeSource {
    /// [`KeyframeRowSource::spans`] の添字。
    pub span: usize,
    /// パラメータの `keyframes` の添字。
    pub index: usize,
    /// コンポジションの時刻（秒）。
    pub comp: f64,
}

impl ParamSpan<'_> {
    /// コンポジションの時刻 `t` の、このパラメータの時間軸の時刻（再フレーミングはカットの素材の時刻）。
    pub fn param_time(&self, layer: &Layer, t: RationalTime) -> Result<RationalTime> {
        match &self.cut {
            None => Ok(t),
            Some((cut, placed)) => cut_source_time(cut, placed, source_time(layer, t)?),
        }
    }

    /// パラメータの時間軸の時刻 `t` の、コンポジションの時刻（秒）。
    fn comp_secs(&self, layer: &Layer, t: RationalTime) -> Option<f64> {
        let Some((cut, placed)) = &self.cut else {
            return Some(t.to_f64());
        };
        let local = t.checked_sub(cut.in_point).ok()?.div_by(cut.speed).ok()?;
        let seq = local.checked_add(placed.range.start).ok()?;
        comp_time(layer, seq).ok().map(RationalTime::to_f64)
    }

    /// コンポジションの時刻 `t`（秒）がこのパラメータの範囲に入るか。
    pub fn contains(&self, t: f64) -> bool {
        t >= self.start - SAME_TIME && t < self.end - SAME_TIME
    }
}

/// レイヤー `layer` のキーフレームの行。キーフレームのあるパラメータごと（Inspector の区画の順）の後に、再フレーミング。
pub fn keyframe_rows<'a>(
    project: &'a Project,
    comp: &Composition,
    layer: &'a Layer,
) -> Vec<KeyframeRowSource<'a>> {
    let animated = layer.comp_time_params().iter().any(|p| p.is_animated());
    let mut rows: Vec<KeyframeRowSource<'a>> = if animated {
        let whole = (0.0, comp.duration.to_f64());
        let params = layer.comp_time_param_paths().into_iter();
        let keyed = params.filter(|(_, p)| p.is_animated());
        keyed
            .map(|(path, param)| param_row(layer, path, param, whole))
            .collect()
    } else {
        Vec::new()
    };
    rows.extend(reframe_rows(project, layer));
    rows
}

/// レイヤーのパラメータ 1 つの行。範囲はコンポジションの全体。
fn param_row<'a>(
    layer: &Layer,
    path: String,
    param: &'a Param,
    (start, end): (f64, f64),
) -> KeyframeRowSource<'a> {
    let keyframes = param.keyframes.iter().enumerate();
    let keyframes = keyframes.map(|(index, k)| KeyframeSource {
        span: 0,
        index,
        comp: k.time.to_f64(),
    });
    KeyframeRowSource {
        name: row_name(layer, &path),
        keyframes: keyframes.collect(),
        spans: vec![ParamSpan {
            path,
            param,
            cut: None,
            start,
            end,
        }],
    }
}

/// 再フレーミングの変形のパラメータの名前（`reframe.<カット>.<名前>`）。
const REFRAME_FIELDS: [&str; 4] = ["position", "scale", "rotation", "anchor"];

/// 再フレーミングの行。タイムリマップのあるレイヤーは、Timeline のカットの帯（速さだけで置く）とキーフレームの位置が
/// 食い違うので出さない。
fn reframe_rows<'a>(project: &'a Project, layer: &'a Layer) -> Vec<KeyframeRowSource<'a>> {
    if layer.framing.cuts.is_empty() || layer.time_remap.is_some() {
        return Vec::new();
    }
    let Some(seq) = layer_sequence(project, layer) else {
        return Vec::new();
    };
    let Ok(placed) = place_cuts(seq) else {
        return Vec::new();
    };
    REFRAME_FIELDS
        .iter()
        .filter_map(|field| reframe_row(layer, seq, &placed, field))
        .collect()
}

/// 再フレーミングの変形のパラメータ `field` の行。再フレーミングのある全部のカットを範囲にし、キーフレームはカットの
/// 範囲に入るものだけを置く。キーフレームが 1 つも無ければ `None`。
fn reframe_row<'a>(
    layer: &'a Layer,
    seq: &'a Sequence,
    placed: &[PlacedCut],
    field: &str,
) -> Option<KeyframeRowSource<'a>> {
    let spans: Vec<ParamSpan<'a>> = placed
        .iter()
        .filter_map(|p| {
            let reframe = layer.framing.reframe(&p.id)?;
            Some(ParamSpan {
                path: format!("reframe.{}.{field}", p.id),
                param: reframe_param(&reframe.transform, field)?,
                cut: Some((&seq.cuts[p.index], p.clone())),
                start: comp_secs(layer, p.range.start.to_f64()),
                end: comp_secs(layer, p.range.end.to_f64()),
            })
        })
        .collect();
    let mut keyframes = Vec::new();
    for (span, s) in spans.iter().enumerate() {
        for (index, k) in s.param.keyframes.iter().enumerate() {
            let comp = s.comp_secs(layer, k.time).filter(|t| s.contains(*t));
            keyframes.extend(comp.map(|comp| KeyframeSource { span, index, comp }));
        }
    }
    // 行のキーフレームは前から順（Timeline が二分探索で見えている所を引く）。カットの並びと速さによらずそろえる。
    keyframes.sort_by(|a, b| a.comp.total_cmp(&b.comp));
    (!keyframes.is_empty()).then(|| KeyframeRowSource {
        name: format!("Reframe › {}", title(field)),
        spans,
        keyframes,
    })
}

fn reframe_param<'a>(t: &'a Transform, field: &str) -> Option<&'a Param> {
    match field {
        "position" => Some(&t.position),
        "scale" => Some(&t.scale),
        "rotation" => Some(&t.rotation),
        "anchor" => t.anchor.as_ref(),
        _ => None,
    }
}

/// パスの行の名前。区画は Inspector の区画の名前、決まった名前のパラメータは先頭を大文字にし、ノードのパラメータの
/// 名前（`source.*`、`effects.*`）はそのまま出す（Inspector の行と同じ名前）。
fn row_name(layer: &Layer, path: &str) -> String {
    let (head, rest) = path.split_once('.').unwrap_or((path, ""));
    match head {
        "transform" | "crop" | "caption" => format!("{} › {}", title(head), title(rest)),
        "audio" => format!("Audio › {}", if rest == "gain_db" { "Gain" } else { "Pan" }),
        "source" => format!("Source › {rest}"),
        "effects" => {
            let (id, name) = rest.split_once('.').unwrap_or((rest, ""));
            let kind = layer.effects.iter().find(|e| e.id.as_str() == id);
            format!(
                "Effect: {} › {name}",
                kind.map_or(id, |e| e.node.kind.as_str())
            )
        }
        "masks" => {
            let (id, name) = rest.split_once('.').unwrap_or((rest, ""));
            let name = name.strip_prefix("rect.").unwrap_or(name);
            format!("Mask: {id} › {}", title(name))
        }
        _ => title(path),
    }
}

/// 先頭の 1 字を大文字にする。
fn title(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// Timeline のキーフレームの行。キーフレームを動かせる範囲は、そのパラメータの範囲の最後のフレームの始まりまで。
pub(super) fn layout_rows(rows: &[KeyframeRowSource], fps: f64) -> Vec<KeyframeRow> {
    let frame = if fps > 0.0 { 1.0 / fps } else { 0.0 };
    let row = |r: &KeyframeRowSource| KeyframeRow {
        name: r.name.as_str().into(),
        keyframes: r
            .keyframes
            .iter()
            .map(|k| {
                let s = &r.spans[k.span];
                RowKeyframe {
                    time: k.comp,
                    min: s.start,
                    max: (s.end - frame).max(s.start),
                }
            })
            .collect(),
    };
    rows.iter().map(row).collect()
}

/// 帯のマーカーの時刻。`shiftLayer` が動かすパラメータのキーフレームの時刻（`Layer::keyframe_times`）に、再フレーミングの
/// キーフレームのコンポジションの時刻を足す（前から順、重複なし）。
pub(super) fn marker_times(layer: &Layer, rows: &[KeyframeRowSource]) -> Arc<[f64]> {
    let mut times: Vec<f64> = layer
        .keyframe_times()
        .into_iter()
        .map(RationalTime::to_f64)
        .collect();
    let reframe = rows
        .iter()
        .filter(|r| r.spans.iter().any(|s| s.cut.is_some()));
    times.extend(reframe.flat_map(|r| r.keyframes.iter().map(|k| k.comp)));
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < SAME_TIME);
    times.into()
}

#[cfg(test)]
mod tests;
