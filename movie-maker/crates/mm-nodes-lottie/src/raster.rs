//! velato の描画の命令を vello_cpu で CPU の画像にし、ノードの種類が返す形（線形、premultiplied の f32）に直す。

use std::sync::OnceLock;

use velato::model::{fixed, ImageAsset};
use velato::{Composition, RenderSink, Renderer};
use vello_cpu::kurbo::{Affine, BezPath, Shape};
use vello_cpu::peniko::{BlendMode, Brush};
use vello_cpu::{PaintType, Pixmap, RenderContext, Resources};

use mm_types::transfer::bt1886_to_linear;
use mm_types::{MmError, Result, RgbaImage, Size};

/// 円や角丸を折れ線にするときの許容差（出力の画素）。1/10 画素なら縁の揺れは見えない。
const TOLERANCE_PX: f64 = 0.1;
/// 許容差の下限。拡大が極端な変形で 0 に近づき、折れ線の点が増えすぎるのを防ぐ。
const MIN_TOLERANCE: f64 = 1e-6;
/// 8 bit の最大値と段の数。
const MAX8: f32 = 255.0;
const LEVELS: usize = 256;

/// 描く 1 回の入力。
pub(crate) struct DrawRequest<'a> {
    pub composition: &'a Composition,
    pub frame: f64,
    pub size: Size,
    /// アニメーションの座標から画像の画素への変形。
    pub transform: Affine,
}

/// 描画の道具の組。画布と作業の領域を使い回し、毎フレームの確保を避ける。
#[derive(Default)]
pub(crate) struct Raster {
    renderer: Renderer,
    resources: Resources,
    canvas: Option<(RenderContext, Pixmap)>,
}

impl Raster {
    /// `req` を描く。大きさが vello_cpu の上限（一辺 65535）を超えれば `InvalidArgument`。
    pub(crate) fn draw(&mut self, req: &DrawRequest<'_>) -> Result<RgbaImage> {
        let (w, h) = side(req.size)?;
        let (ctx, pixmap) = match &mut self.canvas {
            Some((ctx, pixmap)) if (ctx.width(), ctx.height()) == (w, h) => {
                ctx.reset();
                (ctx, pixmap)
            }
            slot => {
                let fresh = slot.insert((RenderContext::new(w, h), Pixmap::new(w, h)));
                (&mut fresh.0, &mut fresh.1)
            }
        };
        let mut sink = CpuSink::new(ctx, req.transform);
        self.renderer
            .append(req.composition, req.frame, req.transform, 1.0, &mut sink);
        ctx.flush();
        // 使い回す画像に前のフレームが残らないよう、描く前に透明にする。
        pixmap.data_as_u8_slice_mut().fill(0);
        ctx.render(&mut *pixmap, &mut self.resources);
        RgbaImage::from_raw(req.size, premul8_to_linear(pixmap.data_as_u8_slice()))
    }
}

/// 画像の一辺を vello_cpu の型（u16）にする。
fn side(size: Size) -> Result<(u16, u16)> {
    let fit = |v: u32| u16::try_from(v).ok().filter(|v| *v > 0);
    match (fit(size.width), fit(size.height)) {
        (Some(w), Some(h)) => Ok((w, h)),
        _ => Err(MmError::InvalidArgument(format!(
            "lottie: {}x{} は描けない大きさ",
            size.width, size.height
        ))),
    }
}

/// velato の命令を vello_cpu の `RenderContext` に流す。
struct CpuSink<'a> {
    ctx: &'a mut RenderContext,
    tolerance: f64,
}

impl<'a> CpuSink<'a> {
    fn new(ctx: &'a mut RenderContext, transform: Affine) -> Self {
        let scale = transform.determinant().abs().sqrt();
        Self {
            ctx,
            tolerance: (TOLERANCE_PX / scale).max(MIN_TOLERANCE),
        }
    }

    fn path(&self, shape: &impl Shape) -> BezPath {
        shape.to_path(self.tolerance)
    }
}

/// velato の筆（peniko の既定の型）を vello_cpu の塗りにする。画像の筆は velato が出さないので塗らない。
fn paint(brush: &Brush) -> Option<PaintType> {
    match brush {
        Brush::Solid(color) => Some(PaintType::Solid(*color)),
        Brush::Gradient(gradient) => Some(PaintType::Gradient(gradient.clone())),
        Brush::Image(_) => None,
    }
}

impl RenderSink for CpuSink<'_> {
    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        shape: &impl Shape,
    ) {
        let clip = self.path(shape);
        self.ctx.set_transform(transform);
        self.ctx
            .push_layer(Some(&clip), Some(blend.into()), Some(alpha), None, None);
    }

    fn push_clip_layer(&mut self, transform: Affine, shape: &impl Shape) {
        let clip = self.path(shape);
        self.ctx.set_transform(transform);
        self.ctx.push_clip_layer(&clip);
    }

    fn pop_layer(&mut self) {
        self.ctx.pop_layer();
    }

    fn draw(
        &mut self,
        stroke: Option<&fixed::Stroke>,
        transform: Affine,
        brush: &fixed::Brush,
        shape: &impl Shape,
    ) {
        let Some(paint) = paint(brush) else {
            return;
        };
        let path = self.path(shape);
        self.ctx.set_transform(transform);
        self.ctx.set_paint(paint);
        match stroke {
            Some(stroke) => {
                self.ctx.set_stroke(stroke.clone());
                self.ctx.stroke_path(&path);
            }
            None => self.ctx.fill_path(&path),
        }
    }

    /// 埋め込みの画像は扱わない（README の「対応の範囲」）。描かずに飛ばし、残りのレイヤーは描く。
    fn draw_image(&mut self, _image: &ImageAsset, _transform: Affine, _alpha: f64) {}
}

/// premultiplied の符号化値 `c`（alpha `a`）→ 線形の premultiplied の表。添字は `a * 256 + c`。
/// 半透明の画素ごとに `powf` を呼ぶと 1080p で数十 ms かかるので、組の全部を 1 回だけ計算する。
fn premul_table() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = Vec::with_capacity(LEVELS * LEVELS);
        for a in 0..LEVELS {
            let alpha = a as f32 / MAX8;
            for c in 0..LEVELS {
                let straight = if a == 0 {
                    0.0
                } else {
                    (c as f32 / a as f32).min(1.0)
                };
                table.push(bt1886_to_linear(straight) * alpha);
            }
        }
        table
    })
}

/// vello_cpu の画素（RGBA8、premultiplied、符号化値）を線形の premultiplied の f32 にする。
/// 符号化値は素材と同じ Rec.1886 の式で線形にする（`mm_types::transfer`）。半透明の画素は alpha で割ってから線形にし、
/// alpha を掛け直す（符号化の空間で掛けた値をそのまま線形にすると暗くなる）。
pub(crate) fn premul8_to_linear(rgba: &[u8]) -> Vec<f32> {
    let table = premul_table();
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let row = &table[usize::from(px[3]) * LEVELS..][..LEVELS];
        out.extend([
            row[usize::from(px[0])],
            row[usize::from(px[1])],
            row[usize::from(px[2])],
            f32::from(px[3]) / MAX8,
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 不透明な画素は_rec1886_で線形にする() {
        let out = premul8_to_linear(&[255, 128, 0, 255]);
        assert_eq!(out[0], 1.0);
        assert!((out[1] - 0.1913).abs() < 1e-3, "{}", out[1]);
        assert_eq!((out[2], out[3]), (0.0, 1.0));
    }

    #[test]
    fn 半透明の画素は割ってから線形にし掛け直す() {
        let out = premul8_to_linear(&[128, 128, 128, 128, 9, 9, 9, 0]);
        let alpha = 128.0 / MAX8;
        for c in &out[..3] {
            assert!((c - alpha).abs() < 1e-6, "{c}");
        }
        assert_eq!(&out[4..], &[0.0; 4], "透明は色を持たない");
    }

    #[test]
    fn 大きさが_0_か上限を超えれば誤り() {
        assert!(side(Size::new(0, 4)).is_err());
        assert!(side(Size::new(70_000, 4)).is_err());
        assert_eq!(side(Size::new(3, 4)).unwrap(), (3, 4));
    }
}
