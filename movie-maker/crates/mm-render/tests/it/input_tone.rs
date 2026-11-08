//! タイムラインの作業の明るさが SDR（既定）のとき、HDR の素材のレイヤーだけが合成の前に SDR のトーンマップを通り、
//! SDR の白（文字や画像の白、線形 1.0）は書き出しで 235 のまま残ること。素材は単色を返すモックのデコーダーで、
//! 色はハイライト（線形 2.0、406 cd/m²）。G はフレームの番号（0）が入るので 0。
//! GPU が無い環境ではスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use std::sync::Arc;

use crate::common;
use crate::tone_map::{
    expected_yuv, export_nv12, pq_asset, white_and_highlight_luma, CODE_TOLERANCE, SIZE,
};

use common::{comp, gpu, layer, project, render_at, services, shader, solid};
use mm_doc::{BlendMode, Node, Project, WorkingRange, NESTED_KIND, NESTED_PARAM};
use mm_render::{GpuContext, WgpuRenderer};
use mm_testkit::audio::SampleMixerFactory;
use mm_testkit::decode::{SampleDecoderFactory, SampleMedia};
use mm_types::tonemap::{bt2390_to_sdr, hdr_source_peak};
use mm_types::{ColorSpace, ParamValue, Rational, RationalTime, Rgba};

/// 素材の線形の値（R と B）。
const HIGHLIGHT: f32 = 2.0;
/// f16 の中間とトーンマップの GPU と CPU の差の許容。
const TOLERANCE: f32 = 2e-3;

/// 左半分に SDR の白、右半分は透明を描く。文字や画像の白の代わり。
const WHITE_LEFT: &str = "fn mm_main(uv: vec2<f32>) -> vec4<f32> {
    if (uv.x < 0.5) { return vec4<f32>(1.0, 1.0, 1.0, 1.0); }
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
";

/// ハイライトの単色を返すモックのデコーダーのレンダラー。
fn highlight_renderer(ctx: &Arc<GpuContext>) -> WgpuRenderer {
    let media = SampleMedia {
        duration: RationalTime::seconds(1),
        fps: Rational::integer(30),
        size: SIZE,
        base_color: Rgba::new(HIGHLIGHT, 0.0, HIGHLIGHT, 1.0),
        with_audio: false,
    };
    WgpuRenderer::new(
        ctx.clone(),
        Arc::new(SampleDecoderFactory { media }),
        Arc::new(SampleMixerFactory),
        services(),
    )
}

fn footage() -> Node {
    Node::new("footage", 1).with_param("asset", ParamValue::AssetRef(pq_asset().id))
}

/// 下に素材、上に左半分の白を重ねたコンポジション `main`。`nested` なら素材を入れ子のコンポジションに入れる。
fn scene(space: ColorSpace, range: WorkingRange, nested: bool) -> Project {
    let clip = if nested {
        Node::new(NESTED_KIND, 1).with_param(NESTED_PARAM, ParamValue::Text("inner".into()))
    } else {
        footage()
    };
    let main = comp(
        "main",
        SIZE,
        1,
        vec![
            layer("clip", clip, 1),
            layer("white", shader(WHITE_LEFT), 1),
        ],
    );
    let inner = comp("inner", SIZE, 1, vec![layer("inner-clip", footage(), 1)]);
    let mut p = project(vec![main, inner], vec![]);
    let mut asset = pq_asset();
    asset.color_space = space;
    p.assets.push(asset);
    p.color_config.working_range = range;
    p
}

/// 右半分（素材だけの所）の中央の R。
fn highlight_at(r: &mut WgpuRenderer, p: &Project) -> f32 {
    let image = render_at(r, p, "main", RationalTime::ZERO, SIZE);
    image.pixel(SIZE.width * 3 / 4, SIZE.height / 2).r
}

#[test]
fn 作業の明るさが_sdr_なら_sdr_の白は_235_のままで_hdr_の素材のハイライトは白の下に残る() {
    let Some(ctx) = gpu() else { return };
    let mut r = highlight_renderer(&ctx);
    let p = scene(ColorSpace::bt2100_pq(), WorkingRange::Sdr, false);
    let peak = hdr_source_peak([&ColorSpace::bt2100_pq()]);
    let want = bt2390_to_sdr([HIGHLIGHT, 0.0, HIGHLIGHT], peak);
    let image = render_at(&mut r, &p, "main", RationalTime::ZERO, SIZE);
    let white = image.pixel(SIZE.width / 4, SIZE.height / 2);
    assert_eq!(
        [white.r, white.g, white.b],
        [1.0; 3],
        "SDR の白は 1.0 のまま"
    );
    let lit = image.pixel(SIZE.width * 3 / 4, SIZE.height / 2);
    assert!(
        (lit.r - want[0]).abs() < TOLERANCE && lit.r < 1.0 && (lit.b - want[2]).abs() < TOLERANCE,
        "素材は入力の側で白の下に収まる: {lit:?} 期待 {want:?}"
    );

    let exported = export_nv12(&mut r, &p);
    let (white_y, highlight_y) = white_and_highlight_luma(&exported);
    assert_eq!(white_y, 235, "SDR の白は 8 bit の tv レンジで 235");
    let want_y = expected_yuv(want, 1.0)[0];
    assert!(
        (highlight_y - want_y).abs() <= CODE_TOLERANCE && highlight_y < 235,
        "ハイライトは白の下に別の明るさで残る: {highlight_y} 期待 {want_y}"
    );
}

/// 作業の明るさが HDR なら素材は 1 を超える値のまま合成され、書き出しは合成の後のトーンマップで収める（今と同じ）。
#[test]
fn 作業の明るさが_hdr_なら_hdr_の素材は合成の前にトーンマップしない() {
    let Some(ctx) = gpu() else { return };
    let mut r = highlight_renderer(&ctx);
    let p = scene(ColorSpace::bt2100_pq(), WorkingRange::Hdr, false);
    let lit = highlight_at(&mut r, &p);
    assert!((lit - HIGHLIGHT).abs() < TOLERANCE, "{lit}");
    let (white_y, _) = white_and_highlight_luma(&export_nv12(&mut r, &p));
    assert!(
        (white_y - 214).abs() <= CODE_TOLERANCE,
        "白は肩の中で 214: {white_y}"
    );
}

/// HDR の素材が無いプロジェクトはトーンマップのパスを通らず、作業の明るさのどちらでも同じ画素と書き出し。
#[test]
fn hdr_の素材が無いプロジェクトは作業の明るさで変わらない() {
    let Some(ctx) = gpu() else { return };
    let mut r = highlight_renderer(&ctx);
    let sdr = scene(ColorSpace::bt709(), WorkingRange::Sdr, false);
    let hdr = scene(ColorSpace::bt709(), WorkingRange::Hdr, false);
    let a = render_at(&mut r, &sdr, "main", RationalTime::ZERO, SIZE);
    let b = render_at(&mut r, &hdr, "main", RationalTime::ZERO, SIZE);
    assert_eq!(a.max_diff(&b).expect("同じ大きさ"), 0.0);
    assert_eq!(a.pixel(SIZE.width * 3 / 4, SIZE.height / 2).r, HIGHLIGHT);
    assert_eq!(export_nv12(&mut r, &sdr), export_nv12(&mut r, &hdr));
}

/// 入れ子の画像はトーンマップの後の画素を覚えるので、作業の明るさを変えたら引き直す（キーに作業の明るさが入る）。
#[test]
fn 作業の明るさを変えると入れ子の画像を引き直す() {
    let Some(ctx) = gpu() else { return };
    let mut r = highlight_renderer(&ctx);
    let peak = hdr_source_peak([&ColorSpace::bt2100_pq()]);
    let mapped = bt2390_to_sdr([HIGHLIGHT, 0.0, HIGHLIGHT], peak)[0];
    for range in [WorkingRange::Sdr, WorkingRange::Hdr, WorkingRange::Sdr] {
        let p = scene(ColorSpace::bt2100_pq(), range, true);
        let want = match range {
            WorkingRange::Sdr => mapped,
            WorkingRange::Hdr => HIGHLIGHT,
        };
        let got = highlight_at(&mut r, &p);
        assert!(
            (got - want).abs() < TOLERANCE,
            "{range:?}: {got} 期待 {want}"
        );
    }
}

/// レイヤーの画像はトーンマップの前を覚えるので、作業の明るさを切り替えても素材の画像を作り直さない。
#[test]
fn 作業の明るさを切り替えてもレイヤーの画像は覚えたものを使う() {
    let Some(ctx) = gpu() else { return };
    let mut r = highlight_renderer(&ctx);
    highlight_at(
        &mut r,
        &scene(ColorSpace::bt2100_pq(), WorkingRange::Sdr, false),
    );
    let inserted = r.frame_cache_stats().inserted;
    for range in [WorkingRange::Hdr, WorkingRange::Sdr] {
        highlight_at(&mut r, &scene(ColorSpace::bt2100_pq(), range, false));
        assert_eq!(r.frame_cache_stats().inserted, inserted, "{range:?}");
    }
}

/// 効果と合成モードは入力の側のトーンマップの後で掛かる。加算の合成は SDR に収めた素材の値を下の色に足す。
#[test]
fn 作業の明るさが_sdr_なら加算の合成は収めた後の素材を足す() {
    let Some(ctx) = gpu() else { return };
    let mut r = highlight_renderer(&ctx);
    let base = 0.25;
    let mut clip = layer("clip", footage(), 1);
    clip.blend = BlendMode::Add;
    let under = layer("base", solid(Rgba::new(base, base, base, 1.0)), 1);
    let mut p = project(vec![comp("main", SIZE, 1, vec![under, clip])], vec![]);
    p.assets.push(pq_asset());
    let peak = hdr_source_peak([&ColorSpace::bt2100_pq()]);
    let mapped = bt2390_to_sdr([HIGHLIGHT, 0.0, HIGHLIGHT], peak)[0];
    for (range, want) in [
        (WorkingRange::Sdr, base + mapped),
        (WorkingRange::Hdr, base + HIGHLIGHT),
    ] {
        p.color_config.working_range = range;
        let got = highlight_at(&mut r, &p);
        assert!(
            (got - want).abs() < TOLERANCE,
            "{range:?}: {got} 期待 {want}"
        );
    }
}
