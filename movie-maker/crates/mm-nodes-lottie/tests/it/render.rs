//! レンダラーと組み立てを通す試験: 操作（MCP と同じ JSON）でプロジェクトの素材とレイヤーを足し、wgpu で描く。
//! GPU が無い環境ではスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use std::path::Path;
use std::sync::Arc;

use mm_doc::{Asset, Composition, Layer, Node, Project};
use mm_node_api::{NodeRegistry, Quality};
use mm_nodes_lottie::{LottieKind, ANIMATION_PARAM, KIND_ID};
use mm_ops::{Entry, Op, OpContext};
use mm_render::{GpuContext, GpuRequest, WgpuRenderer};
use mm_render_api::{RenderRequest, Renderer};
use mm_runtime::{assemble, Backend, RuntimeConfig};
use mm_testkit::audio::SampleMixerFactory;
use mm_testkit::decode::SampleDecoderFactory;
use mm_testkit::fixtures::sample_services;
use mm_testkit::node::MapRegistry;
use mm_types::{
    AssetId, ColorSpace, Id, MmError, ParamValue, Rational, RationalTime, Rgba, RgbaImage, Size,
};
use serde_json::json;

use crate::fixture;

const SIZE: Size = Size::new(64, 64);
const ASSET: &str = "anim";
/// 背景の単色（線形の青）。
const BACKGROUND: Rgba = Rgba::new(0.0, 0.0, 1.0, 1.0);
/// t = 0 の赤い箱の中心（アニメーションの (20, 30) を 64/100 倍した位置）。
const BOX_PIXEL: (u32, u32) = (13, 19);

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

fn asset(path: &Path) -> Asset {
    Asset {
        id: Id::new(ASSET),
        path: path.to_string_lossy().into_owned(),
        // フレームのキャッシュのキーに入るので、フィクスチャごとに変える。
        content_hash: if path.ends_with("broken.json") {
            "b0ce0000".into()
        } else {
            "10771e00".into()
        },
        color_space: ColorSpace::working(),
        duration: RationalTime::ZERO,
        fps: None,
        timebase_offset: RationalTime::ZERO,
    }
}

fn layer(id: &str, node: Node) -> Layer {
    Layer::new(
        id,
        node,
        RationalTime::ZERO,
        RationalTime::ZERO,
        RationalTime::seconds(2),
    )
}

fn lottie_node() -> Node {
    Node::new(KIND_ID, 1).with_param(ANIMATION_PARAM, ParamValue::AssetRef(AssetId::new(ASSET)))
}

/// 背景の単色だけのコンポジションに、操作（MCP のツールと同じ JSON）で素材と `lottie` のレイヤーを足す。
fn project_via_ops(path: &Path) -> Project {
    let mut p = Project::new();
    let solid = Node::new("solid", 1).with_param("color", ParamValue::Color(BACKGROUND));
    p.compositions = vec![Composition::new(
        Id::new("main"),
        SIZE.width,
        SIZE.height,
        Rational::integer(30),
        RationalTime::seconds(2),
    )
    .with_layers(vec![layer("bg", solid)])];
    let ops = [
        json!({"op": "addAsset", "asset": asset(path)}),
        json!({"op": "addLayer", "composition": "main", "layer": layer("anim", lottie_node())}),
    ];
    let ctx = OpContext::new(Entry::Mcp);
    for value in ops {
        let op: Op = serde_json::from_value(value).expect("操作の JSON");
        assert!(Op::NAMES.contains(&op.name()), "MCP のツールになる操作");
        op.apply(&mut p, &ctx).expect("操作を当てられる");
    }
    p
}

fn registry() -> MapRegistry {
    let mut r = MapRegistry::default();
    for kind in mm_nodes_basic::all_kinds() {
        r.insert(kind);
    }
    r.insert(Arc::new(LottieKind::default()));
    r
}

fn render(r: &mut WgpuRenderer, p: &Project) -> Arc<RgbaImage> {
    let nodes = registry();
    let frame = r
        .render_frame(&RenderRequest {
            project: p,
            composition: &Id::new("main"),
            time: RationalTime::ZERO,
            size: SIZE,
            quality: Quality::Full,
            nodes: &nodes,
        })
        .expect("描ける");
    r.gpu().download(&frame).expect("リードバックできる")
}

fn renderer(ctx: &Arc<GpuContext>) -> WgpuRenderer {
    WgpuRenderer::new(
        ctx.clone(),
        Arc::new(SampleDecoderFactory::default()),
        Arc::new(SampleMixerFactory),
        sample_services(),
    )
}

fn is_background(c: Rgba) -> bool {
    c.r < 0.01 && c.g < 0.01 && c.b > 0.99
}

#[test]
fn 素材の_lottie_をレイヤーとして描く() {
    let Some(ctx) = gpu() else { return };
    let mut r = renderer(&ctx);
    let img = render(&mut r, &project_via_ops(&fixture("shapes.json")));
    assert_eq!(r.take_errors(), Vec::<String>::new());
    let (x, y) = BOX_PIXEL;
    let c = img.pixel(x, y);
    assert!(c.r > 0.5 && c.b < 0.1, "赤い箱が描かれる: {c:?}");
    assert!(is_background(img.pixel(63, 0)), "アニメーションの外は背景");
}

#[test]
fn 壊れた_json_はそのレイヤーだけ誤りで_ほかは描く() {
    let Some(ctx) = gpu() else { return };
    let mut r = renderer(&ctx);
    let img = render(&mut r, &project_via_ops(&fixture("broken.json")));
    let errors = r.take_errors();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("anim"),
        "誤りはレイヤー anim: {errors:?}"
    );
    let (x, y) = BOX_PIXEL;
    assert!(is_background(img.pixel(x, y)), "背景のレイヤーは描かれる");
}

#[test]
fn 組み立てたエンジンの種類の一覧に入り_描ける() {
    let config = RuntimeConfig {
        render: Backend::Wgpu,
        ..RuntimeConfig::sample()
    };
    let mut engine = match assemble(&config) {
        Ok(e) => e,
        Err(MmError::Unsupported(e)) if std::env::var_os("MM_REQUIRE_GPU").is_none() => {
            eprintln!("GPU が無いのでスキップする: {e}");
            return;
        }
        Err(e) => panic!("組めない: {e}"),
    };
    assert!(engine.registry().kinds().contains(&KIND_ID));
    let p = project_via_ops(&fixture("shapes.json"));
    let img = engine
        .render_frame(
            &p,
            &Id::new("main"),
            RationalTime::ZERO,
            SIZE,
            Quality::Full,
        )
        .expect("描ける");
    let (x, y) = BOX_PIXEL;
    assert!(img.pixel(x, y).r > 0.5, "赤い箱が描かれる");
}
