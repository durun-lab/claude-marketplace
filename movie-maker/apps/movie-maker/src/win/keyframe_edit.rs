//! Timeline のキーフレームの行の編集。行の添字から `mm-ops` の操作（`moveKeyframe`、`removeKeyframe`、`setKeyframe`）を作り、
//! 選んだパラメータを Inspector と Graph に渡す。行の並びは Timeline のモデルと同じ `mm_ui_wgpu::keyframe_rows` から引く。
//!
//! 操作を作る関数はプロジェクトのデータを見るだけで適用はしないので、単体で試せる。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use mm_doc::{Composition, Ease, Interpolation, Keyframe, Layer, Param, Project};
use mm_eval::eval_param;
use mm_ops::{MoveKeyframe, Op, ParamRef, RemoveKeyframe, SetKeyframe};
use mm_types::{Id, ParamValue, RationalTime};
use mm_ui_wgpu::{keyframe_rows, KeyframeRowSource, ParamSpan};
use serde::Serialize;

/// Inspector と Graph で選ぶパラメータ（`ui/sdk` の `ParamSelection`）。Timeline で選び直すたびに `serial` が進むので、
/// Inspector で別のパラメータを選んだ後に Timeline で同じ行を押しても、選び直したことが伝わる。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParamSelection {
    pub path: String,
    pub serial: u64,
}

/// Timeline のキーフレームの行で選んだもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSelection {
    /// レイヤーの ID。
    pub layer: String,
    /// Inspector に出るパラメータのパス。再フレーミングの行は Inspector に無いので `None`。
    pub path: Option<String>,
    /// キーフレームを選んでいるか（Remove Keyframe を押せるか）。行の見出しだけを選んだときは偽。
    pub keyframe: bool,
}

/// キーフレームの行の選択の置き場所。描画スレッドが置き、コマンドの表と選択のイベントが読む。
#[derive(Default)]
pub struct KeyframeSlot {
    selected: Mutex<Option<RowSelection>>,
    /// Timeline で選び直すたびに進む。
    serial: AtomicU64,
}

impl KeyframeSlot {
    /// Timeline で選んだものを置く（`None` で外す）。置くたびに通し番号を進める。
    pub fn select(&self, selection: Option<RowSelection>) {
        *self.selected.lock().expect("keyframe selection") = selection;
        self.serial.fetch_add(1, Ordering::SeqCst);
    }

    /// 描画スレッドの選択に合わせる（モデルを作り直して、選んだキーフレームや行が無くなったとき）。`picked` は選んで
    /// いればキーフレームを選んでいるか。選び直しではないので通し番号は進めない。
    pub fn match_pick(&self, picked: Option<bool>) {
        let mut selected = self.selected.lock().expect("keyframe selection");
        match picked {
            None => *selected = None,
            Some(keyframe) => {
                if let Some(s) = selected.as_mut() {
                    s.keyframe = keyframe;
                }
            }
        }
    }

    /// キーフレームを選んでいるか。
    pub fn keyframe_selected(&self) -> bool {
        let selected = self.selected.lock().expect("keyframe selection");
        selected.as_ref().is_some_and(|s| s.keyframe)
    }

    /// レイヤー `layer` を選んでいるときの、Inspector と Graph で選ぶパラメータ。
    pub fn param_for(&self, layer: Option<&str>) -> Option<ParamSelection> {
        let selected = self.selected.lock().expect("keyframe selection");
        let s = selected
            .as_ref()
            .filter(|s| Some(s.layer.as_str()) == layer)?;
        Some(ParamSelection {
            path: s.path.clone()?,
            serial: self.serial.load(Ordering::SeqCst),
        })
    }
}

/// レイヤー `layer` のキーフレームの行 `row` と、そのレイヤー。
fn row_of<'a>(
    project: &'a Project,
    comp: &Composition,
    layer: &str,
    row: usize,
) -> Option<(&'a Layer, KeyframeRowSource<'a>)> {
    let (_, layer) = project.layer(&Id::new(layer)).ok()?;
    let row = keyframe_rows(project, comp, layer).into_iter().nth(row)?;
    Some((layer, row))
}

/// 行 `row` が Inspector に出るパラメータ 1 つの行なら、そのパス。
pub fn row_path(project: &Project, comp: &Composition, layer: &str, row: usize) -> Option<String> {
    let (_, r) = row_of(project, comp, layer, row)?;
    match &r.spans[..] {
        [span] if span.cut.is_none() => Some(span.path.clone()),
        _ => None,
    }
}

/// 行 `row` のキーフレーム `keyframe` の、置き場所とキーフレーム。
fn keyframe_of<'a, 'r>(
    r: &'r KeyframeRowSource<'a>,
    keyframe: usize,
) -> Option<(&'r ParamSpan<'a>, &'a Keyframe)> {
    let k = r.keyframes.get(keyframe)?;
    let span = r.spans.get(k.span)?;
    Some((span, span.param.keyframes.get(k.index)?))
}

/// キーフレームを時刻 `to`（コンポジションの秒）へ動かす `moveKeyframe`。時刻が変わらなければ `None`。
pub fn move_op(
    project: &Project,
    comp: &Composition,
    layer: &str,
    (row, keyframe): (usize, usize),
    to: f64,
) -> Option<Op> {
    let (layer, r) = row_of(project, comp, layer, row)?;
    let (span, old) = keyframe_of(&r, keyframe)?;
    let time = span.param_time(layer, frame_time(to, comp)?).ok()?;
    (time != old.time).then(|| {
        MoveKeyframe {
            target: ParamRef::layer(&layer.id, span.path.clone()),
            from: old.time,
            keyframe: Keyframe {
                time,
                ..old.clone()
            },
        }
        .into()
    })
}

/// キーフレームを外す `removeKeyframe`。
pub fn remove_op(
    project: &Project,
    comp: &Composition,
    layer: &str,
    (row, keyframe): (usize, usize),
) -> Option<Op> {
    let (layer, r) = row_of(project, comp, layer, row)?;
    let (span, old) = keyframe_of(&r, keyframe)?;
    Some(
        RemoveKeyframe {
            target: ParamRef::layer(&layer.id, span.path.clone()),
            time: old.time,
        }
        .into(),
    )
}

/// 行 `row` の時刻 `time`（コンポジションの秒）に、その時刻のパラメータの値でキーフレームを打つ `setKeyframe`。
/// 再フレーミングの行は、その時刻のカットに再フレーミングがあるときだけ。その時刻にキーフレームがあれば `None`。
pub fn add_op(
    project: &Project,
    comp: &Composition,
    layer: &str,
    row: usize,
    time: f64,
) -> Option<Op> {
    let (layer, r) = row_of(project, comp, layer, row)?;
    let span = r.spans.iter().find(|s| s.contains(time))?;
    let at = span.param_time(layer, frame_time(time, comp)?).ok()?;
    if span.param.keyframes.iter().any(|k| k.time == at) {
        return None;
    }
    Some(
        SetKeyframe {
            target: ParamRef::layer(&layer.id, span.path.clone()),
            keyframe: new_keyframe(span.param, at),
        }
        .into(),
    )
}

/// 時刻 `at` に打つキーフレーム。値はその時刻の評価値、補間は直前のキーフレームを引き継ぎ、無ければ補間できる値は
/// linear、ほかは hold（Inspector の Add/Remove Keyframe と同じ）。
fn new_keyframe(param: &Param, at: RationalTime) -> Keyframe {
    let value = eval_param(param, at);
    let prev = param.keyframes.iter().rev().find(|k| k.time < at);
    let interpolable = matches!(
        value,
        ParamValue::Number(_) | ParamValue::Vec2(_) | ParamValue::Vec3(_) | ParamValue::Color(_)
    );
    let fallback = if interpolable {
        Interpolation::Linear
    } else {
        Interpolation::Hold
    };
    Keyframe {
        time: at,
        value,
        interpolation: prev.map_or(fallback, |k| k.interpolation),
        ease: prev.map_or(Ease::None, |k| k.ease),
        bezier: prev.and_then(|k| k.bezier),
    }
}

/// コンポジションの秒 `t` の、いちばん近いフレームの先頭の時刻。
fn frame_time(t: f64, comp: &Composition) -> Option<RationalTime> {
    let frames = (t * comp.fps.to_f64()).round() as i64;
    RationalTime::from_frames(frames, comp.fps).ok()
}

#[cfg(test)]
mod tests;
