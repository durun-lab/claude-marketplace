//! トラックマット（`Layer.track_matte`）を、単色のレイヤーの画素で確かめる（期待画像は増やさない）。
//! GPU が無い環境ではスキップする。

use crate::common;

use common::{comp, gpu, layer, project, render_at, renderer, seconds, solid};
use mm_doc::{BlendMode, Layer, Mask, MaskRect, MatteKind, Param, Project, TrackMatte};
use mm_render::WgpuRenderer;
use mm_types::{Id, ParamValue, Rgba, RgbaImage, Size};
use std::sync::Arc;

const SIZE: Size = Size::new(64, 36);
/// 中間の画像は半精度の浮動小数なので、0.5 前後で 1e-3 程度の丸めが出る。
const TOLERANCE: f32 = 3e-3;
const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);
const BLUE: Rgba = Rgba::new(0.0, 0.0, 1.0, 1.0);
const WHITE: Rgba = Rgba::new(1.0, 1.0, 1.0, 1.0);
/// 左半分と右半分の、縁から離れた画素。
const LEFT: (u32, u32) = (10, 18);
const RIGHT: (u32, u32) = (54, 18);

fn matte_of(kind: MatteKind, inverted: bool) -> TrackMatte {
    TrackMatte {
        layer: Id::new("matte"),
        kind,
        inverted,
    }
}

/// 左半分だけを覆うマットのレイヤー（白の単色を矩形のマスクで切る）。
fn left_half(color: Rgba) -> Layer {
    let mut l = layer("matte", solid(color), 2);
    l.masks.push(Mask::rect(
        "half",
        MaskRect::new([0.0, 0.0], [32.0, 36.0], 0.0),
    ));
    l
}

/// 赤の切り抜くレイヤーと、その上に置いたマットのレイヤー。
fn scene(matte: Layer, track: TrackMatte) -> Vec<Layer> {
    let mut target = layer("target", solid(RED), 2);
    target.track_matte = Some(track);
    vec![target, matte]
}

fn render(layers: Vec<Layer>) -> Option<(Arc<RgbaImage>, Vec<String>)> {
    let ctx = gpu()?;
    let p = project(vec![comp("main", SIZE, 2, layers)], vec![]);
    let mut r: WgpuRenderer = renderer(&ctx);
    let img = render_at(&mut r, &p, "main", seconds(0), SIZE);
    Some((img, r.take_errors()))
}

fn assert_px(img: &RgbaImage, (x, y): (u32, u32), want: Rgba, what: &str) {
    let got = img.pixel(x, y);
    assert!(
        got.max_diff(want) <= TOLERANCE,
        "{what}: {want:?} のはずが {got:?}"
    );
}

fn scaled(c: Rgba, k: f32) -> Rgba {
    Rgba::new(c.r * k, c.g * k, c.b * k, c.a * k)
}

#[test]
fn アルファのマットはマットの形で切り抜き_マットのレイヤーは描かない() {
    let layers = scene(left_half(WHITE), matte_of(MatteKind::Alpha, false));
    let Some((img, errors)) = render(layers) else {
        return;
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert_px(
        &img,
        LEFT,
        RED,
        "マットの中は切り抜くレイヤー（上の白は描かない）",
    );
    assert_px(&img, RIGHT, Rgba::TRANSPARENT, "マットの外は透明");
}

#[test]
fn 反転のアルファのマットは外を見せる() {
    let layers = scene(left_half(WHITE), matte_of(MatteKind::Alpha, true));
    let Some((img, _)) = render(layers) else {
        return;
    };
    assert_px(&img, LEFT, Rgba::TRANSPARENT, "反転のマットの中");
    assert_px(&img, RIGHT, RED, "反転のマットの外");
}

#[test]
fn 輝度のマットは線形の輝度を被覆率にする() {
    let gray = Rgba::new(0.25, 0.25, 0.25, 1.0);
    let Some((img, _)) = render(scene(left_half(gray), matte_of(MatteKind::Luma, false))) else {
        return;
    };
    assert_px(&img, LEFT, scaled(RED, 0.25), "灰色 0.25 の輝度");
    assert_px(&img, RIGHT, Rgba::TRANSPARENT, "透明の輝度は 0");
    let Some((img, _)) = render(scene(left_half(gray), matte_of(MatteKind::Luma, true))) else {
        return;
    };
    assert_px(&img, LEFT, scaled(RED, 0.75), "反転の輝度");
    assert_px(&img, RIGHT, RED, "反転の透明は全部見せる");
}

#[test]
fn マットのレイヤーの不透明度と変形が効く() {
    let mut matte = left_half(WHITE);
    matte.opacity = Param::constant(ParamValue::Number(0.5));
    matte.transform.position = Param::constant(ParamValue::Vec2([32.0, 0.0]));
    let Some((img, _)) = render(scene(matte, matte_of(MatteKind::Alpha, false))) else {
        return;
    };
    assert_px(&img, LEFT, Rgba::TRANSPARENT, "右へ動かしたマットの外");
    assert_px(&img, RIGHT, scaled(RED, 0.5), "不透明度 0.5 のマット");
}

#[test]
fn マットのレイヤーが時刻の外なら空のマットになる() {
    let late = |l: &mut Layer| l.start = seconds(1);
    let mut matte = left_half(WHITE);
    late(&mut matte);
    let Some((img, errors)) = render(scene(matte.clone(), matte_of(MatteKind::Alpha, false)))
    else {
        return;
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert_px(&img, LEFT, Rgba::TRANSPARENT, "空のマットは全部を隠す");
    let Some((img, _)) = render(scene(matte, matte_of(MatteKind::Alpha, true))) else {
        return;
    };
    assert_px(&img, LEFT, RED, "反転の空のマットは全部を見せる");
}

#[test]
fn 切り抜いた結果を切り抜くレイヤーの合成モードで重ねる() {
    let mut layers = scene(left_half(WHITE), matte_of(MatteKind::Alpha, false));
    layers[0].blend = BlendMode::Add;
    layers.insert(0, layer("base", solid(BLUE), 2));
    let Some((img, errors)) = render(layers) else {
        return;
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert_px(
        &img,
        LEFT,
        Rgba::new(1.0, 0.0, 1.0, 1.0),
        "マットの中は加算",
    );
    assert_px(&img, RIGHT, BLUE, "マットの外は下のまま");
}

fn matted_by(mut l: Layer, matte: &str) -> Layer {
    l.track_matte = Some(TrackMatte {
        layer: Id::new(matte),
        kind: MatteKind::Alpha,
        inverted: false,
    });
    l
}

#[test]
fn 循環したマットは描かずにエラーを記録する() {
    // c ← a ← b ← a。a と b はマットなので合成から外れ、c はマットを辿りきれない。
    let layers = vec![
        matted_by(layer("c", solid(RED), 2), "a"),
        matted_by(layer("a", solid(WHITE), 2), "b"),
        matted_by(layer("b", solid(WHITE), 2), "a"),
    ];
    let Some((img, errors)) = render(layers) else {
        return;
    };
    assert_px(&img, LEFT, Rgba::TRANSPARENT, "描かない");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("layer c:"), "{errors:?}");
}

#[test]
fn マットの連鎖は段ごとに掛ける() {
    // target ← matte（左半分）← outer（上半分）。matte 自身も outer で切り抜かれる。
    let matte = matted_by(left_half(WHITE), "outer");
    let mut outer = layer("outer", solid(WHITE), 2);
    outer.masks.push(Mask::rect(
        "top",
        MaskRect::new([0.0, 0.0], [64.0, 18.0], 0.0),
    ));
    let mut layers = scene(matte, matte_of(MatteKind::Alpha, false));
    layers.push(outer);
    let Some((img, errors)) = render(layers) else {
        return;
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert_px(&img, (10, 8), RED, "左上だけ残る");
    assert_px(
        &img,
        (10, 28),
        Rgba::TRANSPARENT,
        "左下は外のマットで消える",
    );
    assert_px(&img, (54, 8), Rgba::TRANSPARENT, "右上はマットの外");
}

/// 入れ子 `inner` の中に赤を輝度のマットで切り抜いた絵を置き、親 `main` に並べたプロジェクト。
fn nested(matte_color: Rgba) -> Project {
    let mut matte = layer("matte", solid(matte_color), 2);
    matte.masks.clear();
    let inner = comp(
        "inner",
        SIZE,
        2,
        scene(matte, matte_of(MatteKind::Luma, false)),
    );
    let node = mm_doc::Node::new("composition", 1)
        .with_param("composition", ParamValue::Text("inner".into()));
    project(
        vec![inner, comp("main", SIZE, 2, vec![layer("n", node, 2)])],
        vec![],
    )
}

#[test]
fn 入れ子の中のマットの中身を変えると入れ子の画像を描き直す() {
    let Some(ctx) = gpu() else { return };
    let mut r: WgpuRenderer = renderer(&ctx);
    let dark = nested(Rgba::new(0.25, 0.25, 0.25, 1.0));
    let img = render_at(&mut r, &dark, "main", seconds(0), SIZE);
    assert_px(&img, LEFT, scaled(RED, 0.25), "暗いマット");
    let again = render_at(&mut r, &dark, "main", seconds(0), SIZE);
    assert_eq!(*img, *again, "同じ中身は同じ画像");
    let bright = nested(Rgba::new(0.75, 0.75, 0.75, 1.0));
    let img = render_at(&mut r, &bright, "main", seconds(0), SIZE);
    assert_px(&img, LEFT, scaled(RED, 0.75), "マットを明るくした後");
    assert!(r.take_errors().is_empty());
}
