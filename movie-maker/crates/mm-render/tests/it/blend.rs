//! 合成モード（`Layer.blend`）を、単色を 2 枚重ねた画素で確かめる（期待画像は増やさない）。GPU が無い環境ではスキップする。

use crate::common;

use common::{comp, gpu, layer, project, render_at, renderer, seconds, solid};
use mm_doc::{BlendMode, Param};
use mm_render::WgpuRenderer;
use mm_types::{ParamValue, Rgba, Size};

const SIZE: Size = Size::new(16, 16);
/// 中間の画像は半精度の浮動小数なので、0.5 前後で 1e-3 程度の丸めが出る。
const TOLERANCE: f32 = 3e-3;
const CENTER: (u32, u32) = (8, 8);
/// 下の色（不透明）。
const BASE: Rgba = Rgba::new(0.5, 0.25, 1.0, 1.0);
/// 上の色（不透明）。
const TOP: Rgba = Rgba::new(0.5, 0.5, 0.2, 1.0);

/// `base` の上に `top` を `mode` と不透明度 `opacity` で重ねた中心の画素と、レンダラーの誤り。GPU が無ければ `None`。
fn blended(base: Rgba, top: Rgba, mode: BlendMode, opacity: f64) -> Option<(Rgba, Vec<String>)> {
    let ctx = gpu()?;
    let mut over = layer("top", solid(top), 2);
    over.blend = mode;
    over.opacity = Param::constant(ParamValue::Number(opacity));
    let under = layer("base", solid(base), 2);
    let p = project(vec![comp("main", SIZE, 2, vec![under, over])], vec![]);
    let mut r: WgpuRenderer = renderer(&ctx);
    let img = render_at(&mut r, &p, "main", seconds(0), SIZE);
    Some((img.pixel(CENTER.0, CENTER.1), r.take_errors()))
}

fn assert_rgba(got: Rgba, want: Rgba, what: &str) {
    assert!(
        got.max_diff(want) <= TOLERANCE,
        "{what}: {want:?} のはずが {got:?}"
    );
}

#[test]
fn 乗算は下の色に掛ける() {
    let Some((px, errors)) = blended(BASE, TOP, BlendMode::Multiply, 1.0) else {
        return;
    };
    assert!(errors.is_empty(), "{errors:?}");
    assert_rgba(px, Rgba::new(0.25, 0.125, 0.2, 1.0), "multiply");
}

#[test]
fn 半透明の乗算は不透明度の分だけ下の色に寄る() {
    let Some((px, _)) = blended(BASE, TOP, BlendMode::Multiply, 0.5) else {
        return;
    };
    // co = cs·cb + cb·(1 − αs)（下地は不透明）。cs は不透明度を掛けた premultiplied。
    let want = |cs: f32, cb: f32| cs * 0.5 * cb + cb * 0.5;
    assert_rgba(
        px,
        Rgba::new(want(0.5, 0.5), want(0.5, 0.25), want(0.2, 1.0), 1.0),
        "multiply 0.5",
    );
}

#[test]
fn 乗算は透明な下地では上の色をそのまま出す() {
    let Some((px, _)) = blended(Rgba::TRANSPARENT, TOP, BlendMode::Multiply, 1.0) else {
        return;
    };
    assert_rgba(px, TOP, "透明の上の multiply");
}

#[test]
fn 加算とスクリーンは明るくする() {
    let Some((add, _)) = blended(BASE, TOP, BlendMode::Add, 1.0) else {
        return;
    };
    assert_rgba(add, Rgba::new(1.0, 0.75, 1.2, 1.0), "add");
    let Some((screen, _)) = blended(BASE, TOP, BlendMode::Screen, 1.0) else {
        return;
    };
    let s = |a: f32, b: f32| a + b - a * b;
    assert_rgba(
        screen,
        Rgba::new(s(0.5, 0.5), s(0.25, 0.5), s(1.0, 0.2), 1.0),
        "screen",
    );
}

#[test]
fn 描けない合成モードは_normal_で重ねて誤りに残す() {
    let Some((px, errors)) = blended(BASE, TOP, BlendMode::Overlay, 1.0) else {
        return;
    };
    assert_rgba(px, TOP, "overlay は normal");
    assert_eq!(errors.len(), 1, "{errors:?}");
}
