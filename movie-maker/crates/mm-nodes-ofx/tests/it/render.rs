//! レンダラー（`mm-render` の wgpu）の効果として描く試験。プレビューと書き出しで同じ画素になること、キーフレームの値が
//! フレームごとに届くこと、プラグインのクラッシュがそのレイヤーだけの誤りで済むこと、1080p の 1 フレームの時間。
//! GPU が無ければスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use std::sync::Arc;
use std::time::Instant;

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

use crate::support::{host, kind, params, GAIN, GAIN_ID};

const SIZE: Size = Size::new(32, 18);
const FPS: Rational = Rational::integer(30);
/// f16 の丸めを見込んだ許容。
const TOLERANCE: f32 = 3e-3;
const GREY: Rgba = Rgba::new(0.5, 0.5, 0.5, 1.0);
const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);
/// 1080p の時間を測るフレームの数（最初の 1 回は起動と読み込みを含むので数えない）。
const TIMED_FRAMES: usize = 3;

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

fn project(layers: Vec<Layer>) -> Project {
    let comp = Composition::new(
        Id::new("main"),
        SIZE.width,
        SIZE.height,
        FPS,
        RationalTime::seconds(2),
    )
    .with_layers(layers);
    let mut p = Project::new();
    p.compositions = vec![comp];
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
