use mm_gpu::WgpuGpu;
use mm_node_api::{FrameCx, FrameRef, GpuTexture, InputSchema, InputSpec, NodeKind, Traits};
use mm_types::{
    ParamSchema, ParamSpec, ParamType, ParamValue, Params, Result, Rgba, RgbaImage, Size,
};

use crate::gpu_pass::{input_texture, Pass, PassCache};
use crate::util::{finite_number, migrate_same_version};
use crate::KIND_VERSION;

/// 標準偏差（レイヤーの座標の画素）のパラメータの名前。
pub const BLUR_RADIUS_PARAM: &str = "radius";
/// これより小さい標準偏差（出力の画素）は描いても変わらないので、入力をそのまま返す。
const MIN_SIGMA: f32 = 0.01;
/// 重みを数える範囲（標準偏差の倍数）。3 倍の外の重みは全体の 0.3% 未満。
const KERNEL_SIGMAS: f32 = 3.0;

/// 1 方向のガウスぼかし。`u.dir` の向きに `-half..=half` の画素を重みで足す。画像の外は透明として数える
/// （CSS の `blur()` と同じ）ので、縁は透明へ向かって薄くなる。
const BLUR_WGSL: &str = r"
struct U { dir: vec2<f32>, sigma: f32, half: f32 }

@fragment
fn mm_fs(in: MmVsOut) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(mm_t0));
    let p = vec2<i32>(in.pos.xy);
    let d = vec2<i32>(u.dir);
    let n = i32(u.half);
    let k = -0.5 / (u.sigma * u.sigma);
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -n; i <= n; i++) {
        let w = exp(k * f32(i * i));
        total += w;
        let q = p + d * i;
        if (all(q >= vec2<i32>(0)) && all(q < size)) {
            sum += textureLoad(mm_t0, q, 0) * w;
        }
    }
    return sum / total;
}
";

/// ガウスぼかし（効果）。`radius` は標準偏差で、レイヤーの座標の画素で持ち、描く画素へは `cx.scale` を掛けて移す。
/// 横と縦の 2 回に分けて掛ける。
#[derive(Debug, Default)]
pub struct GaussianBlurKind {
    cache: PassCache,
}

/// 出力の画素の標準偏差と、片側の画素の数。
#[derive(Debug, Clone, Copy)]
struct Kernel {
    sigma: f32,
    half: u32,
}

impl Kernel {
    fn of(params: &Params, scale: f32) -> Option<Self> {
        let sigma = finite_number(params, BLUR_RADIUS_PARAM, 0.0).max(0.0) * scale;
        (sigma >= MIN_SIGMA).then(|| Self {
            sigma,
            half: (sigma * KERNEL_SIGMAS).ceil() as u32,
        })
    }

    fn weight(self, i: i64) -> f32 {
        (-0.5 * (i * i) as f32 / (self.sigma * self.sigma)).exp()
    }

    fn uniform(self, dir: [f32; 2]) -> Vec<u8> {
        [dir[0], dir[1], self.sigma, self.half as f32]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }
}

impl NodeKind for GaussianBlurKind {
    fn id(&self) -> &'static str {
        "blur.gaussian"
    }

    fn version(&self) -> u32 {
        KIND_VERSION
    }

    fn params(&self) -> ParamSchema {
        ParamSchema::new(vec![ParamSpec::new(
            BLUR_RADIUS_PARAM,
            ParamType::Number {
                min: Some(0.0),
                max: None,
            },
            ParamValue::Number(0.0),
        )])
    }

    fn inputs(&self) -> InputSchema {
        InputSchema {
            inputs: vec![InputSpec {
                name: "source".into(),
                optional: false,
            }],
        }
    }

    fn traits(&self) -> Traits {
        Traits::PIXEL
    }

    fn migrate(&self, old: serde_json::Value, from: u32) -> Result<Params> {
        migrate_same_version(self, old, from)
    }

    fn is_identity(&self, cx: &FrameCx<'_>) -> bool {
        Kernel::of(cx.params, cx.scale).is_none()
    }

    fn render(&self, cx: &mut FrameCx<'_>) -> Result<FrameRef> {
        let t = cx.time;
        let input = cx.inputs.frame(0, t)?;
        let Some(kernel) = Kernel::of(cx.params, cx.scale) else {
            return Ok(input);
        };
        if matches!(input, FrameRef::Transparent { .. }) {
            return Ok(input);
        }
        match cx.gpu.as_any().downcast_mut::<WgpuGpu>() {
            Some(gpu) => self.render_gpu(gpu, kernel, input),
            None => {
                let image = cx.gpu.download(&input)?;
                Ok(FrameRef::cpu(blur_cpu(&image, kernel)))
            }
        }
    }
}

impl GaussianBlurKind {
    fn render_gpu(&self, gpu: &mut WgpuGpu, kernel: Kernel, input: FrameRef) -> Result<FrameRef> {
        let src = input_texture(gpu, input)?;
        let pass = |gpu: &mut WgpuGpu, from: u64, dir: [f32; 2]| {
            self.cache.run(
                gpu,
                &Pass {
                    body: BLUR_WGSL,
                    uniform: &kernel.uniform(dir),
                    inputs: &[from],
                    lut: None,
                    size: src.size,
                },
            )
        };
        let across = pass(gpu, src.id, [1.0, 0.0]);
        if src.made {
            gpu.release_id(src.id);
        }
        let across = across?;
        let down = pass(gpu, across, [0.0, 1.0]);
        gpu.release_id(across);
        Ok(FrameRef::Gpu(GpuTexture {
            id: down?,
            size: src.size,
        }))
    }
}

/// CPU の経路。GPU の経路と同じ重みと縁の扱い。
fn blur_cpu(image: &RgbaImage, kernel: Kernel) -> RgbaImage {
    let size = image.size();
    let across = blur_axis(size, kernel, |x, y| image.pixel(x, y), true);
    blur_axis(size, kernel, |x, y| across.pixel(x, y), false)
}

fn blur_axis(
    size: Size,
    kernel: Kernel,
    at: impl Fn(u32, u32) -> Rgba,
    horizontal: bool,
) -> RgbaImage {
    let mut out = RgbaImage::transparent(size);
    let half = i64::from(kernel.half);
    let total: f32 = (-half..=half).map(|i| kernel.weight(i)).sum();
    let (w, h) = (i64::from(size.width), i64::from(size.height));
    let (dx, dy) = if horizontal { (1, 0) } else { (0, 1) };
    for y in 0..size.height {
        for x in 0..size.width {
            let sum = (-half..=half)
                .map(|i| (i, i64::from(x) + dx * i, i64::from(y) + dy * i))
                .filter(|(_, qx, qy)| (0..w).contains(qx) && (0..h).contains(qy))
                .map(|(i, qx, qy)| at(qx as u32, qy as u32).scaled(kernel.weight(i)))
                .fold(Rgba::TRANSPARENT, |s, p| {
                    Rgba::new(s.r + p.r, s.g + p.g, s.b + p.b, s.a + p.a)
                });
            out.set_pixel(x, y, sum.scaled(1.0 / total));
        }
    }
    out
}
