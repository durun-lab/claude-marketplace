use std::sync::Arc;

use mm_doc::{MatteKind, TrackMatte};
use mm_gpu::{GpuContext, GpuRequest, WgpuGpu};
use mm_node_api::{FrameRef, GpuHelper};
use mm_types::{Id, Rgba, RgbaImage, Size};

use super::*;

/// 中間の画像は半精度の浮動小数なので、1 前後で 1e-3 程度の丸めが出る。
const TOLERANCE: f32 = 2e-3;

/// マットの画素の並び。透明、半透明、HDR のハイライト、広色域の負の値、原色を含む。
const MATTES: [Rgba; 9] = [
    Rgba::new(0.0, 0.0, 0.0, 0.0),
    Rgba::new(1.0, 1.0, 1.0, 1.0),
    Rgba::new(0.25, 0.25, 0.25, 0.5),
    Rgba::new(1.0, 0.0, 0.0, 1.0),
    Rgba::new(0.0, 1.0, 0.0, 1.0),
    Rgba::new(0.0, 0.0, 1.0, 1.0),
    Rgba::new(4.0, 4.0, 4.0, 1.0),
    Rgba::new(-0.5, 0.1, 0.1, 1.0),
    Rgba::new(0.2, 0.4, 0.6, 0.75),
];
const LAYER: Rgba = Rgba::new(0.6, 0.3, 0.9, 0.9);

fn gpu() -> Option<Arc<GpuContext>> {
    match GpuContext::shared(&GpuRequest::default()) {
        Ok(ctx) => Some(ctx),
        Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_some() => {
            panic!("MM_REQUIRE_GPU が有るのに GPU を作れない: {e}")
        }
        Err(e) => {
            eprintln!("GPU が無いのでスキップする: {e}");
            None
        }
    }
}

fn matte(kind: MatteKind, inverted: bool) -> TrackMatte {
    TrackMatte {
        layer: Id::new("m"),
        kind,
        inverted,
    }
}

/// シェーダーと同じ式の CPU の被覆率。
fn cpu_coverage(m: &TrackMatte, px: Rgba) -> f32 {
    let c = match m.kind {
        MatteKind::Alpha => px.a,
        MatteKind::Luma => LUMA[0] * px.r + LUMA[1] * px.g + LUMA[2] * px.b,
    }
    .clamp(0.0, 1.0);
    if m.inverted {
        1.0 - c
    } else {
        c
    }
}

fn texture_id(frame: &FrameRef) -> u64 {
    match frame {
        FrameRef::Gpu(t) => t.id,
        other => panic!("GPU のフレームのはずが {other:?}"),
    }
}

/// 1 行に並べたマットの画素を GPU のパスに通して、出力の画素を返す。
fn run_pass(ctx: &Arc<GpuContext>, m: &TrackMatte) -> Vec<Rgba> {
    let size = Size::new(MATTES.len() as u32, 1);
    let mut gpu = WgpuGpu::new(ctx.clone());
    let layer = gpu
        .upload(&RgbaImage::filled(size, LAYER))
        .expect("上げられる");
    let mut matte_image = RgbaImage::transparent(size);
    for (x, px) in MATTES.iter().enumerate() {
        matte_image.set_pixel(x as u32, 0, *px);
    }
    let matte_frame = gpu.upload(&matte_image).expect("上げられる");
    let out = gpu.create(size).expect("作れる");
    let pass = MattePass::new(&ctx.device);
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let texture = |id| gpu.texture(id).expect("ある");
    pass.encode(
        &ctx.device,
        &mut encoder,
        (
            texture(texture_id(&layer)),
            texture(texture_id(&matte_frame)),
            texture(out),
        ),
        flags(m),
        None,
    );
    ctx.queue.submit([encoder.finish()]);
    let image = gpu
        .download(&FrameRef::Gpu(mm_node_api::GpuTexture { id: out, size }))
        .expect("読める");
    (0..size.width).map(|x| image.pixel(x, 0)).collect()
}

#[test]
fn マットの_4_つの形は_gpu_と_cpu_で同じ被覆率になる() {
    let Some(ctx) = gpu() else { return };
    for kind in [MatteKind::Alpha, MatteKind::Luma] {
        for inverted in [false, true] {
            let m = matte(kind, inverted);
            for (px, got) in MATTES.iter().zip(run_pass(&ctx, &m)) {
                let c = cpu_coverage(&m, *px);
                let want = Rgba::new(LAYER.r * c, LAYER.g * c, LAYER.b * c, LAYER.a * c);
                assert!(
                    got.max_diff(want) <= TOLERANCE,
                    "{kind:?} 反転 {inverted} のマット {px:?}: {want:?} のはずが {got:?}"
                );
            }
        }
    }
}

#[test]
fn 輝度は線形の_rec709_の係数で_アルファを掛けた値() {
    let luma = matte(MatteKind::Luma, false);
    assert!((cpu_coverage(&luma, MATTES[3]) - 0.2126).abs() < 1e-6);
    assert!((cpu_coverage(&luma, MATTES[2]) - 0.25).abs() < 1e-6);
    assert_eq!(
        cpu_coverage(&luma, MATTES[6]),
        1.0,
        "HDR のハイライトは全部見せる"
    );
    assert_eq!(cpu_coverage(&luma, MATTES[0]), 0.0, "透明は隠す");
    let inverted = matte(MatteKind::Luma, true);
    assert_eq!(
        cpu_coverage(&inverted, MATTES[0]),
        1.0,
        "反転の透明は見せる"
    );
}

#[test]
fn マットに使われるレイヤーを集める() {
    use mm_doc::{Composition, Layer, Node};
    use mm_types::{Rational, RationalTime};
    let l = |id: &str| {
        Layer::new(
            id,
            Node::new("solid", 1),
            RationalTime::ZERO,
            RationalTime::ZERO,
            RationalTime::seconds(1),
        )
    };
    let mut target = l("t");
    target.track_matte = Some(matte(MatteKind::Alpha, false));
    let comp = Composition::new(
        Id::new("c"),
        16,
        16,
        Rational::integer(30),
        RationalTime::seconds(1),
    )
    .with_layers(vec![l("m"), target, l("other")]);
    let ids = matte_layers(&comp);
    assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec!["m"]);
}
