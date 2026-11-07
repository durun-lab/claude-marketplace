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
/// これより小さい標準偏差（描く画素）は描いても変わらないので、入力をそのまま返す。
const MIN_SIGMA: f32 = 0.01;
/// 標準偏差（描く画素）の上限。これより大きいと画像はほぼ一様に薄まり、CPU の経路の重みの数も際限なく増える。
const MAX_SIGMA: f32 = 4096.0;
/// 重みを数える範囲（標準偏差の倍数）。3 倍の外の重みは全体の 0.3% 未満。
const KERNEL_SIGMAS: f32 = 3.0;
/// GPU で縮めずに掛ける標準偏差（描く画素）の上限。これより大きいぼかしは、標準偏差がこれ以下になるまで縮めた
/// 画像に掛けて広げ戻す（geneva、Skia と同じ）。1 画素あたりの重みの数が半径によらず 49 以下に収まる。
const DIRECT_MAX_SIGMA: f32 = 8.0;

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

/// `factor` 四方の画素の平均で縮める。画像の外は透明として数える。
const SHRINK_WGSL: &str = r"
struct U { factor: f32, pad0: f32, pad1: f32, pad2: f32 }

@fragment
fn mm_fs(in: MmVsOut) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(mm_t0));
    let f = i32(u.factor);
    let lo = vec2<i32>(in.pos.xy) * f;
    let hi = min(lo + vec2<i32>(f), size);
    var sum = vec4<f32>(0.0);
    for (var y = lo.y; y < hi.y; y++) {
        for (var x = lo.x; x < hi.x; x++) {
            sum += textureLoad(mm_t0, vec2<i32>(x, y), 0);
        }
    }
    return sum / f32(f * f);
}
";

/// 縮めた画像を双線形で元の大きさへ広げる。`u.inv` は元の 1 画素が縮めた画像の uv でいくつか。
const GROW_WGSL: &str = r"
struct U { inv: vec2<f32>, pad: vec2<f32> }

@fragment
fn mm_fs(in: MmVsOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(mm_t0, mm_samp, in.pos.xy * u.inv, 0.0);
}
";

/// ガウスぼかし（効果）。`radius` は標準偏差で、レイヤーの座標の画素で持ち、描く画素へは `cx.scale` を掛けて移す。
/// 横と縦の 2 回に分けて掛ける。
#[derive(Debug, Default)]
pub struct GaussianBlurKind {
    cache: PassCache,
}

/// 描く画素の標準偏差と、片側の画素の数。
#[derive(Debug, Clone, Copy)]
struct Kernel {
    sigma: f32,
    half: u32,
}

impl Kernel {
    fn of(params: &Params, scale: f32) -> Option<Self> {
        let sigma = finite_number(params, BLUR_RADIUS_PARAM, 0.0).max(0.0) * scale;
        (sigma >= MIN_SIGMA).then(|| Self::with_sigma(sigma.min(MAX_SIGMA)))
    }

    fn with_sigma(sigma: f32) -> Self {
        Self {
            sigma,
            half: (sigma * KERNEL_SIGMAS).ceil() as u32,
        }
    }

    /// GPU で縮める倍率。1 なら縮めない。
    fn factor(self) -> u32 {
        (self.sigma / DIRECT_MAX_SIGMA).ceil().max(1.0) as u32
    }

    /// `factor` で縮めた画像に掛ける重み。縮める平均（分散 f²/12）と双線形で広げる分（f²/6）を差し引く。
    fn shrunk(self, factor: u32) -> Self {
        let f = factor as f32;
        let sigma = (self.sigma * self.sigma - f * f / 4.0).max(0.0).sqrt() / f;
        Self::with_sigma(sigma.max(MIN_SIGMA))
    }

    fn weight(self, i: i64) -> f32 {
        (-0.5 * (i * i) as f32 / (self.sigma * self.sigma)).exp()
    }

    fn uniform(self, dir: [f32; 2]) -> Vec<u8> {
        floats(&[dir[0], dir[1], self.sigma, self.half as f32])
    }
}

fn floats(values: &[f32; 4]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
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
        let factor = kernel.factor();
        let out = if factor == 1 {
            self.blur(gpu, src.id, src.size, kernel)
        } else {
            self.blur_shrunk(gpu, src.id, src.size, kernel, factor)
        };
        if src.made {
            gpu.release_id(src.id);
        }
        Ok(FrameRef::Gpu(GpuTexture {
            id: out?,
            size: src.size,
        }))
    }

    fn pass(
        &self,
        gpu: &mut WgpuGpu,
        body: &str,
        uniform: &[u8],
        from: u64,
        size: Size,
    ) -> Result<u64> {
        let pass = Pass {
            body,
            uniform,
            inputs: &[from],
            lut: None,
            size,
        };
        self.cache.run(gpu, &pass)
    }

    /// 横と縦の 2 回で掛ける。
    fn blur(&self, gpu: &mut WgpuGpu, from: u64, size: Size, kernel: Kernel) -> Result<u64> {
        let across = self.pass(gpu, BLUR_WGSL, &kernel.uniform([1.0, 0.0]), from, size)?;
        let down = self.pass(gpu, BLUR_WGSL, &kernel.uniform([0.0, 1.0]), across, size);
        gpu.release_id(across);
        down
    }

    /// `factor` で縮めた画像に掛けて、元の大きさへ広げ戻す。
    fn blur_shrunk(
        &self,
        gpu: &mut WgpuGpu,
        from: u64,
        size: Size,
        kernel: Kernel,
        factor: u32,
    ) -> Result<u64> {
        let small = Size::new(size.width.div_ceil(factor), size.height.div_ceil(factor));
        let shrink = floats(&[factor as f32, 0.0, 0.0, 0.0]);
        let shrunk = self.pass(gpu, SHRINK_WGSL, &shrink, from, small)?;
        let blurred = self.blur(gpu, shrunk, small, kernel.shrunk(factor));
        gpu.release_id(shrunk);
        let blurred = blurred?;
        let f = factor as f32;
        let inv = [
            1.0 / (f * small.width as f32),
            1.0 / (f * small.height as f32),
        ];
        let grown = self.pass(
            gpu,
            GROW_WGSL,
            &floats(&[inv[0], inv[1], 0.0, 0.0]),
            blurred,
            size,
        );
        gpu.release_id(blurred);
        grown
    }
}

/// CPU の経路。縮めずに、GPU の経路と同じ重みと縁の扱いで掛ける。
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
    let (len, dx, dy) = if horizontal {
        (i64::from(size.width), 1, 0)
    } else {
        (i64::from(size.height), 0, 1)
    };
    for y in 0..size.height {
        for x in 0..size.width {
            let pos = if horizontal { x } else { y };
            // 画像の外の画素は透明なので、重みの和（total）にだけ入れて足さない。
            let range = (-half).max(-i64::from(pos))..=half.min(len - 1 - i64::from(pos));
            let sum = range
                .map(|i| {
                    let (qx, qy) = (i64::from(x) + dx * i, i64::from(y) + dy * i);
                    at(qx as u32, qy as u32).scaled(kernel.weight(i))
                })
                .fold(Rgba::TRANSPARENT, |s, p| {
                    Rgba::new(s.r + p.r, s.g + p.g, s.b + p.b, s.a + p.a)
                });
            out.set_pixel(x, y, sum.scaled(1.0 / total));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 大きい標準偏差は縮めた画像の重みで同じ広がりにする() {
        let kernel = Kernel::with_sigma(36.0);
        let factor = kernel.factor();
        assert_eq!(factor, 5);
        let small = kernel.shrunk(factor);
        assert!(small.sigma <= DIRECT_MAX_SIGMA, "{small:?}");
        // 縮めた画像の分散に、縮める平均と広げる双線形の分を足すと元の分散に戻る。
        let f = factor as f32;
        let restored = (small.sigma * f).powi(2) + f * f / 4.0;
        assert!((restored - 36.0 * 36.0).abs() < 1e-2, "{restored}");
    }

    #[test]
    fn 小さい標準偏差は縮めない_上限で止める() {
        assert_eq!(Kernel::with_sigma(DIRECT_MAX_SIGMA).factor(), 1);
        let mut params = Params::new();
        params.set(BLUR_RADIUS_PARAM, ParamValue::Number(1e9));
        let kernel = Kernel::of(&params, 1.0).expect("掛ける");
        assert_eq!(kernel.sigma, MAX_SIGMA);
    }
}
