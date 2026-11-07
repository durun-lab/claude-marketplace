//! レンダラー（`mm-render` の wgpu）の効果として描く試験。プレビューと書き出しで同じ画素になること、キーフレームの値が
//! フレームごとに届くこと、プラグインのクラッシュがそのレイヤーだけの誤りで済むこと、1080p の 1 フレームの時間。
//! GPU が無ければスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use mm_host_ofx::TEST_PLUGINS;

use mm_doc::{Composition, Effect, Interpolation, Keyframe, Layer, Node, Param, Project};
use mm_node_api::{NodeKind, Quality};
use mm_nodes_basic::all_kinds;
use mm_nodes_ofx::{KIND_ID, KIND_VERSION};
use mm_render::{GpuContext, GpuRequest, WgpuRenderer};
use mm_render_api::{RangeRequest, RenderRequest, Renderer};
use mm_testkit::audio::SampleMixerFactory;
use mm_testkit::decode::SampleDecoderFactory;
use mm_testkit::fixtures::sample_services;
use mm_testkit::node::{ConstInputs, Harness, MapRegistry};
use mm_types::{Id, ParamValue, Rational, RationalTime, Rgba, RgbaImage, Size, TimeRange};

use crate::support::{host, kind, kind_in, params, GAIN, GAIN_ID};

const SIZE: Size = Size::new(32, 18);
const FPS: Rational = Rational::integer(30);
/// f16 の丸めを見込んだ許容。
const TOLERANCE: f32 = 3e-3;
const GREY: Rgba = Rgba::new(0.5, 0.5, 0.5, 1.0);
const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);
/// 1080p の時間を測るフレームの数（最初の 1 回は起動と読み込みを含むので数えない）。
const TIMED_FRAMES: usize = 10;

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

fn renderer(ctx: &Arc<GpuContext>) -> WgpuRenderer {
    WgpuRenderer::new(
        ctx.clone(),
        Arc::new(SampleDecoderFactory::default()),
        Arc::new(SampleMixerFactory),
        sample_services(),
    )
}

fn registry(ofx: Arc<dyn NodeKind>) -> MapRegistry {
    let mut r = MapRegistry::default();
    for k in all_kinds() {
        r.insert(k);
    }
    r.insert(ofx);
    r
}

/// 単色のレイヤーに `ofx` の効果を 1 つ掛ける。
fn layer(id: &str, color: Rgba, effect: Option<Node>) -> Layer {
    let solid = Node::new("solid", 1).with_param("color", ParamValue::Color(color));
    let mut l = Layer::new(
        id,
        solid,
        RationalTime::ZERO,
        RationalTime::ZERO,
        RationalTime::seconds(2),
    );
    if let Some(node) = effect {
        l.effects.push(Effect {
            id: Id::new(format!("{id}-fx")),
            node,
            enabled: true,
        });
    }
    l
}

fn gain_node(values: &[(&str, ParamValue)]) -> Node {
    let mut node = Node::new(KIND_ID, KIND_VERSION);
    for (name, value) in params(GAIN, GAIN_ID, values).0 {
        node = node.with_param(name, value);
    }
    node
}

fn comp(id: &str, layers: Vec<Layer>) -> Composition {
    Composition::new(
        Id::new(id),
        SIZE.width,
        SIZE.height,
        FPS,
        RationalTime::seconds(2),
    )
    .with_layers(layers)
}

fn project(layers: Vec<Layer>) -> Project {
    let mut p = Project::new();
    p.compositions = vec![comp("main", layers)];
    p
}

fn preview(r: &mut WgpuRenderer, p: &Project, nodes: &MapRegistry, t: RationalTime) -> RgbaImage {
    let id = Id::new("main");
    let frame = r
        .render_frame(&RenderRequest {
            project: p,
            composition: &id,
            time: t,
            size: SIZE,
            quality: Quality::Preview,
            nodes,
        })
        .expect("描ける");
    (*r.gpu().download(&frame).expect("読める")).clone()
}

fn export(r: &mut WgpuRenderer, p: &Project, nodes: &MapRegistry, frame: i64) -> RgbaImage {
    let id = Id::new("main");
    let start = RationalTime::from_frames(frame, FPS).unwrap();
    let range = TimeRange::new(start, RationalTime::from_frames(frame + 1, FPS).unwrap()).unwrap();
    let mut out = None;
    r.render_range(
        &RangeRequest {
            project: p,
            composition: &id,
            range,
            fps: FPS,
            size: SIZE,
            quality: Quality::Full,
            nodes,
        },
        &mut |_, f, gpu| {
            out = Some((*gpu.download(&f)?).clone());
            Ok(())
        },
    )
    .expect("書き出せる");
    out.expect("1 フレーム")
}

fn assert_near(got: Rgba, want: Rgba) {
    let d = [
        got.r - want.r,
        got.g - want.g,
        got.b - want.b,
        got.a - want.a,
    ];
    assert!(
        d.iter().all(|v| v.abs() <= TOLERANCE),
        "{got:?} vs {want:?}"
    );
}

#[test]
fn プレビューと書き出しで同じ画素になり_キーフレームの値が届く() {
    let (Some(ctx), Some(exe)) = (gpu(), host()) else {
        return;
    };
    let mut fx = gain_node(&[]);
    fx.params.insert(
        "gain".into(),
        Param {
            value: ParamValue::Number(1.0),
            keyframes: vec![
                Keyframe::new(
                    RationalTime::ZERO,
                    ParamValue::Number(0.0),
                    Interpolation::Linear,
                ),
                Keyframe::new(
                    RationalTime::seconds(2),
                    ParamValue::Number(2.0),
                    Interpolation::Linear,
                ),
            ],
        },
    );
    let p = project(vec![layer("a", GREY, Some(fx))]);
    let nodes = registry(Arc::new(kind(&exe)));
    // フレーム 15（0.5 秒）は gain 0.5、フレーム 45（1.5 秒）は gain 1.5。
    for (frame, gain) in [(15, 0.5_f32), (45, 1.5)] {
        let t = RationalTime::from_frames(frame, FPS).unwrap();
        let shown = preview(&mut renderer(&ctx), &p, &nodes, t);
        let written = export(&mut renderer(&ctx), &p, &nodes, frame);
        assert_eq!(shown, written, "フレーム {frame}");
        let v = 0.5 * gain;
        assert_near(shown.pixel(3, 3), Rgba::new(v, v, v, 1.0));
    }
}

#[test]
fn プラグインのクラッシュはそのレイヤーだけの誤りになる() {
    let (Some(ctx), Some(exe)) = (gpu(), host()) else {
        return;
    };
    let nodes = registry(Arc::new(kind(&exe)));
    let mut r = renderer(&ctx);
    let crashing = gain_node(&[("crash", ParamValue::Bool(true))]);
    let p = project(vec![
        layer("a", RED, None),
        layer("b", GREY, Some(crashing)),
    ]);
    let img = preview(&mut r, &p, &nodes, RationalTime::ZERO);
    assert_near(img.pixel(3, 3), RED);
    assert_eq!(r.take_errors().len(), 1, "クラッシュしたレイヤーだけ誤り");

    let healthy = gain_node(&[("gain", ParamValue::Number(0.5))]);
    let p = project(vec![layer("b", GREY, Some(healthy))]);
    let img = preview(&mut r, &p, &nodes, RationalTime::ZERO);
    assert_near(img.pixel(3, 3), Rgba::new(0.25, 0.25, 0.25, 1.0));
    assert!(r.take_errors().is_empty(), "ホストを起こし直して描ける");
}

/// 試験の Gain のバンドルを `root` の下に写す。
fn copy_gain(root: &Path) -> PathBuf {
    let arch = Path::new("Contents").join(mm_host_ofx::scan::arch_dir());
    let dst = root.join(GAIN).join(&arch);
    std::fs::create_dir_all(&dst).unwrap();
    let binary = dst.join("MmTestGain.ofx");
    std::fs::copy(
        Path::new(TEST_PLUGINS)
            .join(GAIN)
            .join(&arch)
            .join("MmTestGain.ofx"),
        &binary,
    )
    .unwrap();
    binary
}

/// 共有ライブラリを gain が 2 倍の版に差し替える。読み込んだままのホストがいても、名前を変えれば置ける。
fn swap_to_v2(binary: &Path) {
    let arch = Path::new("Contents").join(mm_host_ofx::scan::arch_dir());
    let v2 = Path::new(TEST_PLUGINS)
        .join("MmTestGainV2.ofx.bundle")
        .join(arch)
        .join("MmTestGainV2.ofx");
    std::fs::rename(binary, binary.with_extension("old")).unwrap();
    std::fs::copy(v2, binary).unwrap();
    // 写しても更新時刻が元のままのことがあるので、差し替えた時刻にする。
    let file = std::fs::File::options().write(true).open(binary).unwrap();
    file.set_modified(std::time::SystemTime::now()).unwrap();
}

#[test]
fn プラグインを差し替えたらキャッシュの前の画素を返さない() {
    let (Some(ctx), Some(exe)) = (gpu(), host()) else {
        return;
    };
    let root = std::env::temp_dir().join(format!("mm-nodes-ofx-swap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let binary = copy_gain(&root);
    let nodes = registry(Arc::new(kind_in(&exe, root.clone())));
    // 入れ子の画像はフレームのキャッシュに入るので、中の効果の差し替えがキーに出ないと前の画素を返す。
    let inner = comp("inner", vec![layer("a", GREY, Some(gain_node(&[])))]);
    let nested =
        Node::new("composition", 1).with_param("composition", ParamValue::Text("inner".into()));
    let mut outer = layer("outer", GREY, None);
    outer.source = nested;
    let mut p = project(vec![outer]);
    p.compositions.push(inner);
    let mut r = renderer(&ctx);
    let before = preview(&mut r, &p, &nodes, RationalTime::ZERO);
    assert_near(before.pixel(3, 3), GREY);
    swap_to_v2(&binary);
    let after = preview(&mut r, &p, &nodes, RationalTime::ZERO);
    assert_near(after.pixel(3, 3), Rgba::new(1.0, 1.0, 1.0, 1.0));
    assert!(r.take_errors().is_empty());
    drop((r, nodes));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn フル_hd_の_1_フレームの時間を測る() {
    let Some(exe) = host() else { return };
    let size = Size::new(1920, 1080);
    let k = kind(&exe);
    let p = params(GAIN, GAIN_ID, &[("gain", ParamValue::Number(0.5))]);
    let mut inputs = ConstInputs(vec![Arc::new(RgbaImage::filled(size, GREY))]);
    let mut h = Harness::new(None);
    let mut times = Vec::new();
    for i in 0..=TIMED_FRAMES {
        let start = Instant::now();
        let t = RationalTime::from_frames(i as i64, FPS).unwrap();
        h.render(&k, &p, t, size, 1.0, &mut inputs).expect("描ける");
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let first = times.remove(0);
    times.sort_by(f64::total_cmp);
    let p50 = times[times.len() / 2];
    eprintln!("ofx 1920x1080: 最初 {first:.1} ms（起動と読み込みを含む）、以後の p50 {p50:.1} ms");
}
