//! `transition.wgsl`: 利用者が書いた WGSL で 2 つの入力（出ていくカットと入ってくるカット）を混ぜるトランジションの種類。
//!
//! 名前の約束は gl-transitions（`getFromColor`、`getToColor`、`progress`、`ratio`）に揃える。利用者のコードは
//! `fn mm_transition(uv: vec2<f32>) -> vec4<f32>` を定義し、線形の premultiplied の色を返す。`uv` は左上が (0, 0)
//! （gl-transitions は左下が原点なので、上下を返す移植では `1.0 - uv.y` に読み替える）。使える名前:
//! - `mm_from(uv)`、`mm_to(uv)`: 入力 0 と 1 を双線形で引く（範囲の外は端の色）。
//! - `u.size`（出力の画素数）、`u.progress`（0..1）、`u.ratio`（幅 / 高さ）、`u.time`（トランジションの中の秒）、
//!   `u.duration`（トランジションの長さの秒）、`u.scale`（画面上の倍率）、`u.p0`（`p0`..`p3`）、`u.p1`（`p4`..`p7`）。
//!
//! `progress` と `duration` はレンダラーが境の中の位置から書く。`rewind` が正なら、レンダラーは出ていくカットを
//! 掛かり始めから逆向きにその倍率で描く（[`TRANSITION_REWIND_PARAM`]）。

use mm_gpu::WgpuGpu;
use mm_node_api::{Cost, FrameCx, FrameRef, GpuTexture, InputSchema, InputSpec, NodeKind, Traits};
use mm_types::{MmError, ParamSchema, ParamSpec, ParamType, ParamValue, Params, Result, Size};

use crate::gpu_pass::{input_texture, InputTexture, Pass, PassCache};
use crate::transition::TRANSITION_PROGRESS_PARAM;
use crate::util::{finite_number, migrate_same_version};
use crate::KIND_VERSION;

/// トランジションの長さ（秒）のパラメータの名前。レンダラーが書く。
pub const TRANSITION_DURATION_PARAM: &str = "duration";
/// 出ていくカットを逆向きに描く倍率のパラメータの名前。0 なら普通に描く。レンダラーが読む。
pub const TRANSITION_REWIND_PARAM: &str = "rewind";

/// 数のパラメータの数。`p0`..`p7`。
const NUMBER_PARAMS: usize = 8;
/// `duration` の既定（秒）。
const DEFAULT_DURATION: f64 = 1.0;
/// `progress` の既定。
const DEFAULT_PROGRESS: f64 = 0.5;

/// コードの既定。線形の光の上のクロスディゾルブ。
const DEFAULT_CODE: &str = "fn mm_transition(uv: vec2<f32>) -> vec4<f32> {\n    return mix(mm_from(uv), mm_to(uv), u.progress);\n}\n";

const PRELUDE: &str = r"
struct U {
    size: vec2<f32>,
    progress: f32,
    ratio: f32,
    time: f32,
    duration: f32,
    scale: f32,
    pad: f32,
    p0: vec4<f32>,
    p1: vec4<f32>,
}

fn mm_from(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(mm_t0, mm_samp, uv, 0.0);
}

fn mm_to(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(mm_t1, mm_samp, uv, 0.0);
}

@fragment
fn mm_fs(in: MmVsOut) -> @location(0) vec4<f32> {
    return mm_transition(in.uv);
}
";

/// 利用者の WGSL で 2 つの入力を混ぜる種類。
#[derive(Debug, Default)]
pub struct TransitionWgslKind {
    cache: PassCache,
}

/// uniform（`U`）に詰める値。
struct Uniform {
    size: Size,
    progress: f32,
    duration: f32,
    scale: f32,
    numbers: [f32; NUMBER_PARAMS],
}

impl Uniform {
    /// パラメータから作る。有限でない数は既定値に落とし、進み具合は 0..1、長さは 0 以上に収める。
    fn of(params: &Params, size: Size, scale: f32) -> Self {
        Self {
            size,
            progress: finite_number(params, TRANSITION_PROGRESS_PARAM, DEFAULT_PROGRESS)
                .clamp(0.0, 1.0),
            duration: finite_number(params, TRANSITION_DURATION_PARAM, DEFAULT_DURATION).max(0.0),
            scale,
            numbers: std::array::from_fn(|i| finite_number(params, &format!("p{i}"), 0.0)),
        }
    }

    /// `size`、`progress`、`ratio`、`time`、`duration`、`scale`、詰め物、`p0`..`p7` の順で 16 個。
    fn bytes(&self) -> Vec<u8> {
        let (w, h) = (self.size.width as f32, self.size.height as f32);
        let ratio = if h > 0.0 { w / h } else { 1.0 };
        let mut v = [0.0_f32; 16];
        v[..8].copy_from_slice(&[
            w,
            h,
            self.progress,
            ratio,
            self.progress * self.duration,
            self.duration,
            self.scale,
            0.0,
        ]);
        v[8..].copy_from_slice(&self.numbers);
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }
}

fn number_spec(name: &str, min: Option<f64>, max: Option<f64>, default: f64) -> ParamSpec {
    ParamSpec::new(
        name,
        ParamType::Number { min, max },
        ParamValue::Number(default),
    )
}

fn release(gpu: &mut WgpuGpu, inputs: &[&InputTexture]) {
    for tex in inputs.iter().filter(|t| t.made) {
        gpu.release_id(tex.id);
    }
}

impl TransitionWgslKind {
    fn mix_gpu(
        &self,
        gpu: &mut WgpuGpu,
        (from, to): (FrameRef, FrameRef),
        code: &str,
        uniform: &Uniform,
    ) -> Result<FrameRef> {
        let a = input_texture(gpu, from)?;
        let b = match input_texture(gpu, to) {
            Ok(b) => b,
            Err(e) => {
                release(gpu, &[&a]);
                return Err(e);
            }
        };
        if a.size != b.size {
            release(gpu, &[&a, &b]);
            return Err(MmError::InvalidArgument(
                "トランジションの 2 つの入力の大きさが違う".into(),
            ));
        }
        let body = format!("{PRELUDE}\n{code}");
        let result = self.cache.run(
            gpu,
            &Pass {
                body: &body,
                uniform: &Uniform {
                    size: a.size,
                    ..*uniform
                }
                .bytes(),
                inputs: &[a.id, b.id],
                lut: None,
                size: a.size,
            },
        );
        release(gpu, &[&a, &b]);
        Ok(FrameRef::Gpu(GpuTexture {
            id: result?,
            size: a.size,
        }))
    }
}

impl NodeKind for TransitionWgslKind {
    fn id(&self) -> &'static str {
        "transition.wgsl"
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        let mut params = vec![
            ParamSpec::new(
                "code",
                ParamType::Text,
                ParamValue::Text(DEFAULT_CODE.into()),
            ),
            number_spec(
                TRANSITION_PROGRESS_PARAM,
                Some(0.0),
                Some(1.0),
                DEFAULT_PROGRESS,
            ),
            number_spec(TRANSITION_DURATION_PARAM, Some(0.0), None, DEFAULT_DURATION),
            number_spec(TRANSITION_REWIND_PARAM, Some(0.0), None, 0.0),
        ];
        params.extend((0..NUMBER_PARAMS).map(|i| number_spec(&format!("p{i}"), None, None, 0.0)));
        ParamSchema::new(params)
    }

    fn inputs(&self) -> InputSchema {
        let input = |name: &str| InputSpec {
            name: name.into(),
            optional: false,
        };
        InputSchema {
            inputs: vec![input("from"), input("to")],
        }
    }

    fn traits(&self) -> Traits {
        Traits {
            cost: Cost::Medium,
            ..Traits::PIXEL
        }
    }

    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        migrate_same_version(self, old, from)
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        let code = cx
            .params
            .get("code")
            .and_then(ParamValue::as_text)
            .unwrap_or(DEFAULT_CODE)
            .to_string();
        let uniform = Uniform::of(cx.params, cx.size, cx.scale);
        let t = cx.time;
        let from = cx.inputs.frame(0, t)?;
        let to = cx.inputs.frame(1, t)?;
        let Some(gpu) = cx.gpu.as_any().downcast_mut::<WgpuGpu>() else {
            return Err(MmError::Unsupported(
                "transition.wgsl は wgpu の GPU が要る".into(),
            ));
        };
        self.mix_gpu(gpu, (from, to), &code, &uniform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(values: &[(&str, f64)]) -> Params {
        let mut p = Params::new();
        for (name, v) in values {
            p.set(*name, ParamValue::Number(*v));
        }
        p
    }

    #[test]
    fn 有限でない数は既定値に落としてシェーダーへ渡す() {
        let p = params(&[
            (TRANSITION_PROGRESS_PARAM, f64::NAN),
            (TRANSITION_DURATION_PARAM, f64::INFINITY),
            ("p0", f64::NAN),
            ("p7", f64::NEG_INFINITY),
            ("p3", 2.5),
        ]);
        let u = Uniform::of(&p, Size::new(4, 2), 1.0);
        assert_eq!(u.progress, DEFAULT_PROGRESS as f32);
        assert_eq!(u.duration, DEFAULT_DURATION as f32);
        assert_eq!(u.numbers[0], 0.0);
        assert_eq!(u.numbers[7], 0.0);
        assert_eq!(u.numbers[3], 2.5);
        let bytes = u.bytes();
        assert!(bytes
            .chunks_exact(4)
            .all(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).is_finite()));
    }

    #[test]
    fn 範囲の外の進み具合と負の長さは収める() {
        let p = params(&[
            (TRANSITION_PROGRESS_PARAM, 3.0),
            (TRANSITION_DURATION_PARAM, -1.0),
        ]);
        let u = Uniform::of(&p, Size::new(4, 2), 1.0);
        assert_eq!(u.progress, 1.0);
        assert_eq!(u.duration, 0.0);
    }
}
