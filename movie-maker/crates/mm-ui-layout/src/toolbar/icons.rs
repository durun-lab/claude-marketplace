//! ボタンのアイコン。Windows の記号のフォント（Segoe Fluent Icons、無ければ Segoe MDL2 Assets）の字は文字の仕組みで描き
//! （[`super::toolbar_labels`]）、フォントに無い形（ツールの矢印、トリム、刃、磁石、Lift、Extract）は矩形と `QuadShape` の
//! 組み合わせでアイコンの欄の中心 `(cx, cy)` に描く。三角とひし形は切ると形が変わるので、切らずに描く（欄に収まる大きさだけを使う）。

use crate::geom::{Color, Quad, QuadShape, Rect};

/// 線の太さ。
const LINE: f32 = 2.0;
/// Lift と Extract の下に描く帯の上端の、中心からの下がりと高さ。
const TRACK_Y: f32 = 4.0;
const TRACK_H: f32 = 3.0;

/// ボタンのアイコン。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    /// 字 1 つ。`symbol` は記号のフォントの字、`plain` は記号のフォントが無いときに標準のフォントで描く同じ意味の字。
    Glyph {
        symbol: char,
        plain: char,
    },
    Selection,
    Trim,
    Blade,
    Snapping,
    Lift,
    Extract,
}

impl Icon {
    pub(crate) const fn glyph(symbol: char, plain: char) -> Self {
        Self::Glyph { symbol, plain }
    }
}

/// フォントでなく矩形で描くアイコンの矩形の並び。字のアイコンは空。
pub(crate) fn icon_quads(icon: Icon, (cx, cy): (f32, f32), color: Color) -> Vec<Quad> {
    let mut out = Vec::new();
    let mut rects: Vec<Rect> = Vec::new();
    match icon {
        Icon::Glyph { .. } => {}
        Icon::Selection => {
            // 矢印のカーソル。三角の下に短い軸。
            out.push(Quad::shaped(
                (cx - 1.0, cy - 2.0),
                (9.0, 12.0),
                QuadShape::TriUp,
                color,
            ));
            rects.push(Rect::new(cx - 2.0, cy + 3.0, LINE, 5.0));
        }
        Icon::Trim => {
            // 二つの括弧 [ ]（端を詰める）。
            rects.push(Rect::new(cx - 6.0, cy - 6.0, LINE, 12.0));
            rects.push(Rect::new(cx - 6.0, cy - 6.0, 5.0, LINE));
            rects.push(Rect::new(cx - 6.0, cy + 4.0, 5.0, LINE));
            rects.push(Rect::new(cx + 4.0, cy - 6.0, LINE, 12.0));
            rects.push(Rect::new(cx + 1.0, cy - 6.0, 5.0, LINE));
            rects.push(Rect::new(cx + 1.0, cy + 4.0, 5.0, LINE));
        }
        Icon::Blade => out.push(Quad::diamond((cx, cy), (10.0, 14.0), color)),
        Icon::Snapping => {
            // 馬蹄形の磁石（両脚と底）。
            rects.push(Rect::new(cx - 6.0, cy - 7.0, 4.0, 11.0));
            rects.push(Rect::new(cx + 2.0, cy - 7.0, 4.0, 11.0));
            rects.push(Rect::new(cx - 6.0, cy + 2.0, 12.0, 5.0));
        }
        Icon::Lift => lift(&mut rects, &mut out, (cx, cy), color),
        Icon::Extract => extract(&mut rects, &mut out, (cx, cy), color),
    }
    out.extend(rects.into_iter().map(|r| Quad::new(r, color)));
    out
}

/// 上向きの矢印（三角と軸）。軸の下端は `bottom`。
fn arrow_up(
    rects: &mut Vec<Rect>,
    tris: &mut Vec<Quad>,
    (cx, cy): (f32, f32),
    bottom: f32,
    color: Color,
) {
    tris.push(Quad::shaped(
        (cx, cy - 5.0),
        (8.0, 6.0),
        QuadShape::TriUp,
        color,
    ));
    rects.push(Rect::new(
        cx - LINE / 2.0,
        cy - 2.0,
        LINE,
        bottom - (cy - 2.0),
    ));
}

/// Lift。帯の中ほどを上へ抜き、抜いた所に隙間が残る（Premiere と Avid の Lift、再生の操作のバーの Overwrite の逆）。
fn lift(rects: &mut Vec<Rect>, tris: &mut Vec<Quad>, (cx, cy): (f32, f32), color: Color) {
    arrow_up(rects, tris, (cx, cy), cy + TRACK_Y - 1.0, color);
    rects.push(Rect::new(cx - 8.0, cy + TRACK_Y, 5.0, TRACK_H));
    rects.push(Rect::new(cx + 3.0, cy + TRACK_Y, 5.0, TRACK_H));
}

/// Extract。帯の中ほどを上へ抜き、両側が内向きに寄って隙間を詰める（Premiere と Avid の Extract、Insert の逆）。
fn extract(rects: &mut Vec<Rect>, tris: &mut Vec<Quad>, (cx, cy): (f32, f32), color: Color) {
    arrow_up(rects, tris, (cx, cy), cy + 1.0, color);
    let mid = cy + TRACK_Y + TRACK_H / 2.0;
    rects.push(Rect::new(cx - 8.0, cy + TRACK_Y, 4.0, TRACK_H));
    rects.push(Rect::new(cx + 4.0, cy + TRACK_Y, 4.0, TRACK_H));
    tris.push(Quad::shaped(
        (cx - 2.0, mid),
        (4.0, 7.0),
        QuadShape::TriRight,
        color,
    ));
    tris.push(Quad::shaped(
        (cx + 2.0, mid),
        (4.0, 7.0),
        QuadShape::TriLeft,
        color,
    ));
}
