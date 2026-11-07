//! 試験の共通の道具。GPU と DXC の用意、フィクスチャの読み込み、1 フレームの描画。

use std::path::PathBuf;
use std::sync::Arc;

use mm_gpu::{GpuContext, GpuRequest, WgpuGpu};
use mm_node_api::{FrameCx, GpuHelper, NodeKind, Quality};
use mm_nodes_hlsl::Dxc;
use mm_testkit::fixtures::sample_services;
use mm_testkit::node::ConstInputs;
use mm_types::{MmError, ParamValue, Params, RationalTime, Rgba, RgbaImage, Size};

/// 設定すると、DXC が無い環境でもスキップせず失敗にする。
const REQUIRE_DXC_ENV: &str = "MM_REQUIRE_DXC";

pub fn gpu() -> Option<Arc<GpuContext>> {
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

pub fn dxc() -> Option<Dxc> {
    let found = Dxc::discover();
    if found.is_none() {
        assert!(
            std::env::var_os(REQUIRE_DXC_ENV).is_none(),
            "{REQUIRE_DXC_ENV} が有るのに SPIR-V を出せる DXC が無い"
        );
        eprintln!("SPIR-V を出せる DXC が無いのでスキップする");
    }
    found
}

/// GPU と DXC の両方。どちらかが無ければ `None`。
pub fn gpu_and_dxc() -> Option<(Arc<GpuContext>, Dxc)> {
    Some((gpu()?, dxc()?))
}

/// `tests/it/fixtures/` のファイルのパス。
pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/it/fixtures")
        .join(name)
}

/// `tests/it/fixtures/` のファイルの中身。
pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name)).expect("フィクスチャを読める")
}

/// 色と透明が場所で変わる入力。`flip` で向きを変え、2 つ目の入力にする。
pub fn gradient(size: Size, flip: bool) -> Arc<RgbaImage> {
    let mut img = RgbaImage::transparent(size);
    for y in 0..size.height {
        for x in 0..size.width {
            let u = x as f32 / (size.width - 1) as f32;
            let v = y as f32 / (size.height - 1) as f32;
            let (u, v) = if flip { (v, 1.0 - u) } else { (u, v) };
            let a = if y < 3 { 0.0 } else { 0.4 + 0.6 * u };
            img.set_pixel(x, y, Rgba::from_straight(u * 1.5, v, 1.0 - u, a));
        }
    }
    Arc::new(img)
}

/// 既定値に `set` を上書きしたパラメータ。
pub fn params_of(kind: &dyn NodeKind, set: &[(&str, ParamValue)]) -> Params {
    let mut params = kind.params().defaults();
    for (name, value) in set {
        params.set(*name, value.clone());
    }
    params
}

/// 1 フレームを描く。
pub fn render_with(
    kind: &dyn NodeKind,
    params: &Params,
    helper: &mut dyn GpuHelper,
    inputs: Vec<Arc<RgbaImage>>,
    (size, time): (Size, RationalTime),
) -> Result<mm_node_api::FrameRef, MmError> {
    let services = sample_services();
    let mut inputs = ConstInputs(inputs);
    let mut cx = FrameCx {
        time,
        params,
        size,
        region: size.to_rect(),
        scale: 1.0,
        quality: Quality::Full,
        inputs: &mut inputs,
        footage: None,
        source_time: RationalTime::ZERO,
        gpu: helper,
        services: &services,
    };
    kind.render(&mut cx)
}

/// GPU で描いてリードバックする。
pub fn render_image(
    ctx: &Arc<GpuContext>,
    kind: &dyn NodeKind,
    params: &Params,
    inputs: Vec<Arc<RgbaImage>>,
    at: (Size, RationalTime),
) -> Arc<RgbaImage> {
    let mut wgpu = WgpuGpu::new(ctx.clone());
    let frame = render_with(kind, params, &mut wgpu, inputs, at)
        .unwrap_or_else(|e| panic!("{} で描ける: {e}", kind.id()));
    wgpu.download(&frame).expect("リードバックできる")
}
