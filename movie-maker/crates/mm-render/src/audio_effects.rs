//! レイヤーの音の効果（`effects` のうち `traits().audio` が真の種類）。音量とパンの前に上から順にかける。
//!
//! パラメータはコンポジションの時間軸の 0 から `AUDIO_PARAM_STEP` フレームごとの格子で評価する。
//! 区間の分け方（書き出しの 1 回、再生の短い区間の連続）に依らず同じ位置で値が変わり、同じサンプルになる。
//! 効果が失敗したら、その効果をかけずに続け、エラーを記録する（ほかのレイヤーの音は止めない）。

use mm_audio_api::AudioBlock;
use mm_doc::{Effect, Layer};
use mm_eval::node_params_at;
use mm_node_api::{AudioEffectCx, NodeKind, NodeRegistry, AUDIO_PARAM_STEP};
use mm_types::{CompositionId, Params, Rational, RationalTime, Result, TimeRange};

use crate::renderer::WgpuRenderer;

/// 効果をかける 1 回分の入力。
pub(crate) struct EffectScope<'a> {
    pub nodes: &'a dyn NodeRegistry,
    pub composition: &'a CompositionId,
    pub layer: &'a Layer,
    /// `block` の区間（コンポジションの時間軸）。
    pub range: TimeRange,
    pub sample_rate: u32,
}

impl WgpuRenderer {
    /// レイヤーの音の効果を `block` に上から順にかける。
    pub(crate) fn apply_audio_effects(&mut self, scope: &EffectScope<'_>, block: &mut AudioBlock) {
        for effect in scope.layer.effects.iter().filter(|e| e.enabled) {
            let Some(kind) = scope.nodes.get(&effect.node.kind) else {
                continue;
            };
            if !kind.traits().audio {
                continue;
            }
            let mut out = block.clone();
            match run_effect(scope, effect, kind.as_ref(), &mut out) {
                Ok(()) => *block = out,
                Err(e) => self.errors.push(format!(
                    "音の効果 {}（{}）をかけられない: {e}",
                    effect.id, effect.node.kind
                )),
            }
        }
    }
}

fn run_effect(
    scope: &EffectScope<'_>,
    effect: &Effect,
    kind: &dyn NodeKind,
    block: &mut AudioBlock,
) -> Result<()> {
    let rate = Rational::integer(scope.sample_rate);
    let start = scope.range.start.frame_index(rate);
    let schema = kind.params();
    let params = grid_points(start, block.frames(), |frame| {
        let t = RationalTime::from_frames(frame, rate)?;
        Ok(node_params_at(&effect.node, t).with_defaults(&schema))
    })?;
    let instance = format!("{}/{}/{}", scope.composition, scope.layer.id, effect.id);
    let cx = AudioEffectCx {
        range: scope.range,
        instance: &instance,
        params: &params,
        sample_rate: scope.sample_rate,
        channels: block.channels,
    };
    kind.process_audio(&cx, block)
}

/// `start` フレームから `frames` フレームの区間の格子の点（区間の先頭からの位置、値）。
/// 先頭は区間の前の格子の時刻で評価し、分けた区間の先頭でも値が変わらないようにする。
fn grid_points(
    start: i64,
    frames: usize,
    eval: impl Fn(i64) -> Result<Params>,
) -> Result<Vec<(usize, Params)>> {
    let step = AUDIO_PARAM_STEP as i64;
    let first = start.div_euclid(step) * step;
    let end = start + frames as i64;
    (first..end.max(first + 1))
        .step_by(step as usize)
        .map(|f| Ok(((f.max(start) - start) as usize, eval(f)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_types::ParamValue;

    fn frame_value(f: i64) -> Result<Params> {
        let mut p = Params::new();
        p.set("f", ParamValue::Number(f as f64));
        Ok(p)
    }

    fn frames_of(points: &[(usize, Params)]) -> Vec<(usize, f64)> {
        points
            .iter()
            .map(|(o, p)| (*o, p.number_or("f", -1.0)))
            .collect()
    }

    #[test]
    fn 格子の点は区間の分け方に依らない位置で値が変わる() {
        let whole = frames_of(&grid_points(0, 200, frame_value).unwrap());
        assert_eq!(whole, [(0, 0.0), (64, 64.0), (128, 128.0), (192, 192.0)]);
        let tail = frames_of(&grid_points(100, 100, frame_value).unwrap());
        assert_eq!(
            tail,
            [(0, 64.0), (28, 128.0), (92, 192.0)],
            "先頭は前の格子の値"
        );
    }

    #[test]
    fn 長さ_0_でも先頭の点を持つ() {
        assert_eq!(grid_points(70, 0, frame_value).unwrap().len(), 1);
    }
}
