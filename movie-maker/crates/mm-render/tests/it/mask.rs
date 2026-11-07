//! 切り抜きとマスクの被覆を、画素のアルファで確かめる（期待画像は増やさない）。GPU が無い環境ではスキップする。

use crate::common;

use common::{comp, gpu, layer, project, render_at, renderer, seconds, solid};
use mm_doc::{Layer, Mask, MaskMode, MaskRect, Param};
use mm_render::WgpuRenderer;
use mm_types::{ParamValue, Rgba, RgbaImage, Size};
use std::sync::Arc;

const SIZE: Size = Size::new(64, 36);
const TOLERANCE: f32 = 3e-3;
const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);
/// 矩形の縁から十分離れた、左半分と右半分の画素。
const LEFT: (u32, u32) = (10, 18);
const RIGHT: (u32, u32) = (54, 18);
const TOP_RIGHT: (u32, u32) = (54, 4);
const CENTER: (u32, u32) = (32, 18);

fn num(v: f64) -> Param {
    Param::constant(ParamValue::Number(v))
}

fn rect_mask(id: &str, pos: [f64; 2], size: [f64; 2], mode: MaskMode, inverted: bool) -> Mask {
    let mut m = Mask::rect(id, MaskRect::new(pos, size, 0.0));
    m.mode = mode;
    m.inverted = inverted;
    m
}

/// 描いた画像のアルファを画素で引く。
struct Alpha(Arc<RgbaImage>);

impl Alpha {
    fn at(&self, (x, y): (u32, u32)) -> f32 {
        self.0.pixel(x, y).a
    }
}

/// 赤い 1 枚のレイヤーを描いて、アルファとレンダラーのエラーを返す。GPU が無ければ `None`。
fn render_alpha(edit: impl FnOnce(&mut Layer)) -> Option<(Alpha, Vec<String>)> {
    let ctx = gpu()?;
    let mut l = layer("l", solid(RED), 2);
    edit(&mut l);
    let p = project(vec![comp("main", SIZE, 2, vec![l])], vec![]);
    let mut r: WgpuRenderer = renderer(&ctx);
    let img = render_at(&mut r, &p, "main", seconds(0), SIZE);
    Some((Alpha(img), r.take_errors()))
}

fn assert_alpha(got: f32, want: f32, what: &str) {
    assert!(
        (got - want).abs() <= TOLERANCE,
        "{what}: {want} のはずが {got}"
    );
}

#[test]
fn 大きさ_0_の矩形のマスクはぼかしても空() {
    let Some((alpha, _)) = render_alpha(|l| {
        let mut m = rect_mask("m", [32.0, 18.0], [0.0, 0.0], MaskMode::Add, false);
        m.feather = num(20.0);
        l.masks.push(m);
    }) else {
        return;
    };
    assert_alpha(alpha.at(CENTER), 0.0, "中心");
    assert_alpha(alpha.at(LEFT), 0.0, "左");
}

#[test]
fn 大きさ_0_の矩形の反転は全部を通す() {
    let Some((alpha, _)) = render_alpha(|l| {
        l.masks.push(rect_mask(
            "m",
            [32.0, 18.0],
            [0.0, 0.0],
            MaskMode::Subtract,
            true,
        ));
    }) else {
        return;
    };
    // 全部から始めて、空の矩形の反転（全部）を引くと何も残らない。
    assert_alpha(alpha.at(CENTER), 0.0, "中心");
}

#[test]
fn 引くマスクは重なった所を抜く() {
    let Some((alpha, errors)) = render_alpha(|l| {
        l.masks.push(rect_mask(
            "a",
            [0.0, 0.0],
            [64.0, 36.0],
            MaskMode::Add,
            false,
        ));
        l.masks.push(rect_mask(
            "b",
            [0.0, 0.0],
            [32.0, 36.0],
            MaskMode::Subtract,
            false,
        ));
    }) else {
        return;
    };
    assert_alpha(alpha.at(LEFT), 0.0, "引いた左");
    assert_alpha(alpha.at(RIGHT), 1.0, "残った右");
    assert!(errors.is_empty());
}

#[test]
fn 重なりのマスクは両方に入る所だけ残す() {
    let Some((alpha, _)) = render_alpha(|l| {
        l.masks.push(rect_mask(
            "a",
            [32.0, 0.0],
            [32.0, 36.0],
            MaskMode::Add,
            false,
        ));
        l.masks.push(rect_mask(
            "b",
            [0.0, 0.0],
            [64.0, 18.0],
            MaskMode::Intersect,
            false,
        ));
    }) else {
        return;
    };
    assert_alpha(alpha.at(TOP_RIGHT), 1.0, "右上");
    assert_alpha(alpha.at(RIGHT), 0.0, "右下は 2 つ目の外");
    assert_alpha(alpha.at(LEFT), 0.0, "左は 1 つ目の外");
}

#[test]
fn 反転は矩形の外を通す() {
    let Some((alpha, _)) = render_alpha(|l| {
        l.masks.push(rect_mask(
            "a",
            [0.0, 0.0],
            [32.0, 36.0],
            MaskMode::Add,
            true,
        ));
    }) else {
        return;
    };
    assert_alpha(alpha.at(LEFT), 0.0, "矩形の中");
    assert_alpha(alpha.at(RIGHT), 1.0, "矩形の外");
}

#[test]
fn 複数枚の足すマスクは和になる() {
    let Some((alpha, _)) = render_alpha(|l| {
        l.masks.push(rect_mask(
            "a",
            [0.0, 0.0],
            [20.0, 36.0],
            MaskMode::Add,
            false,
        ));
        l.masks.push(rect_mask(
            "b",
            [44.0, 0.0],
            [20.0, 36.0],
            MaskMode::Add,
            false,
        ));
    }) else {
        return;
    };
    assert_alpha(alpha.at(LEFT), 1.0, "左");
    assert_alpha(alpha.at(RIGHT), 1.0, "右");
    assert_alpha(alpha.at(CENTER), 0.0, "間");
}

#[test]
fn 描けないマスクが先頭でも足すなら空から始めエラーを残す() {
    let Some((alpha, errors)) = render_alpha(|l| {
        let mut path = rect_mask("p", [0.0, 0.0], [1.0, 1.0], MaskMode::Add, false);
        path.rect = None;
        l.masks.push(path);
        l.masks.push(rect_mask(
            "s",
            [0.0, 0.0],
            [32.0, 36.0],
            MaskMode::Subtract,
            false,
        ));
    }) else {
        return;
    };
    assert_alpha(alpha.at(RIGHT), 0.0, "空から引いても空");
    assert!(
        errors.iter().any(|e| e.contains("mask p")),
        "頂点の並びのマスクが残る: {errors:?}"
    );
}

#[test]
fn 上限を超えたマスクはエラーに残す() {
    let Some((_, errors)) = render_alpha(|l| {
        for i in 0..5 {
            l.masks.push(rect_mask(
                &format!("m{i}"),
                [0.0, 0.0],
                [64.0, 36.0],
                MaskMode::Add,
                false,
            ));
        }
    }) else {
        return;
    };
    assert!(
        errors.iter().any(|e| e.contains("mask m4")),
        "5 枚目が残る: {errors:?}"
    );
}

#[test]
fn 左右の切り抜きが幅を少し超えても何も残らない() {
    let Some((alpha, _)) = render_alpha(|l| {
        l.crop.left = Some(num(32.5));
        l.crop.right = Some(num(32.0));
    }) else {
        return;
    };
    assert_alpha(alpha.at(CENTER), 0.0, "交差した中央");
    assert_alpha(alpha.at((31, 18)), 0.0, "中央の左");
}
