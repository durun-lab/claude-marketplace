//! 実際の CEF で描くテスト。`CEF_PATH`（配布物）と `MM_CEF_HOST`（`--features cef` で作った `mm-host-cef`）が
//! 無ければスキップする（`MM_REQUIRE_CEF` があれば失敗にする）。`MM_CEF_OUT` があればそこへ PNG を書く。
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mm_gpu::{GpuContext, GpuRequest, WgpuGpu};
use mm_host_cef::CefLauncher;
use mm_node_api::{FrameCx, GpuHelper, NoInputs, NodeKind, Quality};
use mm_nodes_cef::{WebCefKind, DATA_PARAM, HTML_PARAM, STATIC_PARAM};
use mm_testkit::fixtures::sample_services;
use mm_testkit::node::Harness;
use mm_types::{ParamValue, Params, RationalTime, RgbaImage, Size};

const PAGE: &str = include_str!("pages/probe.html");
const FULL_HD: Size = Size::new(1920, 1080);
/// f16 への丸めと、GPU と CPU の符号化値の復号の差を見込んだ許容。
const TOLERANCE: f32 = 4e-3;

pub(super) fn host() -> Option<PathBuf> {
    let found = std::env::var_os("CEF_PATH").and(std::env::var_os("MM_CEF_HOST"));
    if found.is_none() {
        assert!(
            std::env::var_os("MM_REQUIRE_CEF").is_none(),
            "MM_REQUIRE_CEF があるのに CEF_PATH か MM_CEF_HOST が無い"
        );
        eprintln!("CEF_PATH か MM_CEF_HOST が無いので CEF のテストをスキップする");
    }
    found.map(PathBuf::from)
}

pub(super) fn kind(exe: &Path) -> WebCefKind {
    WebCefKind::with_host(Arc::new(CefLauncher), exe.to_path_buf())
}

pub(super) fn gpu() -> WgpuGpu {
    let ctx = GpuContext::shared(&GpuRequest {
        backends: wgpu::Backends::DX12,
        ..GpuRequest::default()
    })
    .expect("D3D12 の GPU");
    WgpuGpu::new(ctx)
}

fn params(data: serde_json::Value, is_static: bool) -> Params {
    let mut p = Params::new();
    p.set(HTML_PARAM, ParamValue::Text(PAGE.into()))
        .set(DATA_PARAM, ParamValue::Json(data))
        .set(STATIC_PARAM, ParamValue::Bool(is_static));
    p
}

/// GPU の経路で 1 フレーム描いて CPU にリードバックする。
pub(super) fn render_gpu(
    kind: &WebCefKind,
    gpu: &mut WgpuGpu,
    p: &Params,
    t: RationalTime,
    size: Size,
) -> RgbaImage {
    let services = sample_services();
    let mut inputs = NoInputs;
    let mut cx = FrameCx {
        time: t,
        params: p,
        size,
        region: size.to_rect(),
        scale: 1.0,
        quality: Quality::Full,
        inputs: &mut inputs,
        footage: None,
        source_time: t,
        gpu,
        services: &services,
    };
    let frame = kind.render(&mut cx).expect("描ける");
    let image = gpu.download(&frame).expect("リードバックできる");
    gpu.release(&frame);
    gpu.after_submit();
    Arc::unwrap_or_clone(image)
}

/// 透過が見えるよう、灰色の市松の上に重ねて PNG に書く。
fn save(name: &str, image: &RgbaImage) {
    const CELL: u32 = 32;
    let Some(dir) = std::env::var_os("MM_CEF_OUT") else {
        return;
    };
    let path = PathBuf::from(dir).join(format!("{name}.png"));
    let mut board = RgbaImage::transparent(image.size());
    for y in 0..image.height() {
        for x in 0..image.width() {
            let v = if (x / CELL + y / CELL).is_multiple_of(2) {
                0.2
            } else {
                0.4
            };
            board.set_pixel(x, y, mm_types::Rgba::new(v, v, v, 1.0));
        }
    }
    board.blit_over(image, 0, 0, 1.0);
    let rgb = mm_testkit::golden::to_srgb8(&board);
    let file = std::fs::File::create(&path).expect("PNG を作れる");
    let mut enc = png::Encoder::new(file, image.width(), image.height());
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&rgb))
        .expect("PNG を書ける");
    eprintln!("{}", path.display());
}

#[test]
fn 透過のページを共有テクスチャで受けて合成できる形にする() {
    let Some(exe) = host() else { return };
    let mut gpu = gpu();
    let kind = kind(&exe);
    let p = params(serde_json::json!({"label": "web.cef"}), false);
    let image = render_gpu(&kind, &mut gpu, &p, RationalTime::seconds(1), FULL_HD);
    save("web-cef-probe", &image);
    assert_eq!(image.pixel(1900, 1060).a, 0.0, "背景は透明");
    let half = image.pixel(150, 150);
    assert!(
        (half.a - 0.5).abs() < 0.01,
        "半透明の白は alpha 0.5: {half:?}"
    );
    assert!(
        (half.r - half.a).abs() < 0.01,
        "白の premultiplied は rgb = alpha: {half:?}"
    );
    let bar = image.pixel(100 + 200, 600);
    assert!(
        bar.a > 0.99 && bar.b > 0.5,
        "t = 1 で棒は x = 300 にある: {bar:?}"
    );
}

#[test]
fn 同じ入力なら同じ画素で_gpu_と_cpu_の経路も合う() {
    let Some(exe) = host() else { return };
    let size = Size::new(640, 360);
    let p = params(serde_json::json!({"label": "same"}), false);
    let t = RationalTime::new(3, 2).unwrap();
    let mut gpu = gpu();
    let a = render_gpu(&kind(&exe), &mut gpu, &p, t, size);
    let b = render_gpu(&kind(&exe), &mut gpu, &p, t, size);
    assert_eq!(a, b, "別のホストで描き直しても同じ画素");
    let cpu_kind = kind(&exe);
    let p = p.with_defaults(&cpu_kind.params());
    let (c, _) = Harness::new(None)
        .render(&cpu_kind, &p, t, size, 1.0, &mut NoInputs)
        .expect("CPU の経路で描ける");
    let diff = a.max_diff(&c).expect("同じ大きさ");
    assert!(diff < TOLERANCE, "GPU と CPU の経路の差 {diff}");
}

/// キャッシュから追い出すのに使う大きさ。キャッシュの上限（384 MiB）を数枚で超える。
const EVICTOR: Size = Size::new(3840, 2160);

#[test]
fn 同じホストで同じ時刻を描き直しても同じ画素() {
    let Some(exe) = host() else { return };
    let size = Size::new(640, 360);
    let p = params(serde_json::json!({"label": "again"}), false);
    let t = RationalTime::new(3, 2).unwrap();
    let kind = kind(&exe);
    let mut gpu = gpu();
    let first = render_gpu(&kind, &mut gpu, &p, t, size);
    for n in 10..18 {
        render_gpu(&kind, &mut gpu, &p, RationalTime::seconds(n), EVICTOR);
    }
    let drawn = kind.stats().drawn;
    let second = render_gpu(&kind, &mut gpu, &p, t, size);
    assert_eq!(
        kind.stats().drawn,
        drawn + 1,
        "キャッシュから出ていて描き直す"
    );
    assert_eq!(first, second, "同じホストで時刻を往復しても同じ画素");
}

#[test]
fn デバイスを作り直しても新しいデバイスのホストで描く() {
    let Some(exe) = host() else { return };
    let size = Size::new(320, 180);
    let p = params(serde_json::json!({"label": "recreate"}), false);
    let t = RationalTime::seconds(1);
    let kind = kind(&exe);
    let mut before = gpu();
    let a = render_gpu(&kind, &mut before, &p, t, size);
    let fresh = before.context().recreate().expect("作り直せる");
    let mut after = WgpuGpu::new(fresh);
    let b = render_gpu(&kind, &mut after, &p, t, size);
    assert_eq!(a, b, "作り直したデバイスでも同じ画素");
    assert_eq!(
        kind.stats().drawn,
        2,
        "キャッシュは古いデバイスのテクスチャを使わない"
    );
}

/// `data.i` 番目の位置に白い矩形を描くページ。`variant` を変えると別のページ（別のホストの割り振り）になる。
/// 矩形を動かすのは BeginFrame の後の requestAnimationFrame で、描画に `slow_ms` かかる
/// （多数のレイヤーの合成や動画のデコードと GPU を取り合って、1 フレームが遅くなった場合）。
fn mark_page(variant: usize, slow_ms: u32) -> String {
    format!(
        r#"<!doctype html><html><head><!-- {variant} --><style>
html, body {{ margin: 0; background: transparent; }}
#m {{ position: absolute; top: 4px; width: 12px; height: 12px; background: white; }}
</style></head><body><div id="m"></div><script>
window.mmSetTime = (t, d) => {{ requestAnimationFrame(() => {{
  const end = performance.now() + {slow_ms}; while (performance.now() < end) {{}}
  document.getElementById('m').style.left = (d.i * 20 + 4) + 'px'; }}); }};
</script></body></html>"#
    )
}

#[test]
fn 同じフレームに多数のレイヤーが並んでも全部を描く() {
    const LAYERS: usize = 14;
    /// ホストの上限（3）に収まる数のページを交互に使い、ページを読み直さずに描く経路を通す。
    const PAGES: usize = 2;
    const SLOW_MS: u32 = 80;
    let Some(exe) = host() else { return };
    let size = Size::new(LAYERS as u32 * 20, 20);
    let mut gpu = gpu();
    let kind = kind(&exe);
    let t = RationalTime::seconds(1);
    for i in 0..LAYERS {
        let mut p = params(serde_json::json!({ "i": i }), false);
        p.set(HTML_PARAM, ParamValue::Text(mark_page(i % PAGES, SLOW_MS)));
        let image = render_gpu(&kind, &mut gpu, &p, t, size);
        let marks: Vec<usize> = (0..LAYERS)
            .filter(|n| image.pixel(*n as u32 * 20 + 10, 10).a > 0.99)
            .collect();
        assert_eq!(
            marks,
            vec![i],
            "{i} 枚目のレイヤーは自分の位置にだけ矩形を描く"
        );
    }
    assert_eq!(kind.stats().drawn, LAYERS as u64);
}
