//! ボタンのアイコン。Windows の記号のフォント（Segoe Fluent Icons、無ければ Segoe MDL2 Assets）の字は文字の仕組みで描き
//! （[`super::toolbar_labels`]）、フォントに無い形（ツールの矢印、トリム、刃、磁石）は矩形と `QuadShape` の組み合わせで
//! アイコンの欄の中心 `(cx, cy)` に描く。三角とひし形は切ると形が変わるので、切らずに描く（欄に収まる大きさだけを使う）。

use crate::geom::{Color, Quad, QuadShape, Rect};

/// 線の太さ。
const LINE: f32 = 2.0;

/// ボタンのアイコン。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    /// 記号のフォントの 1 字。
    Glyph(char),
    Selection,
    Trim,
    Blade,
    Snapping,
}

/// フォントでなく矩形で描くアイコンの矩形の並び。字のアイコンは空。
pub(crate) fn icon_quads(icon: Icon, (cx, cy): (f32, f32), color: Color) -> Vec<Quad> {
    let mut out = Vec::new();
    let mut rects: Vec<Rect> = Vec::new();
    let mut fill = |r: Rect| rects.push(r);
    match icon {
        Icon::Glyph(_) => {}
        Icon::Selection => {
            // 矢印のカーソル。三角の下に短い軸。
            out.push(Quad::shaped(
                (cx - 1.0, cy - 2.0),
                (9.0, 12.0),
                QuadShape::TriUp,
                color,
            ));
            fill(Rect::new(cx - 2.0, cy + 3.0, LINE, 5.0));
        }
        Icon::Trim => {
            // 二つの括弧 [ ]（端を詰める）。
            fill(Rect::new(cx - 6.0, cy - 6.0, LINE, 12.0));
            fill(Rect::new(cx - 6.0, cy - 6.0, 5.0, LINE));
            fill(Rect::new(cx - 6.0, cy + 4.0, 5.0, LINE));
            fill(Rect::new(cx + 4.0, cy - 6.0, LINE, 12.0));
            fill(Rect::new(cx + 1.0, cy - 6.0, 5.0, LINE));
            fill(Rect::new(cx + 1.0, cy + 4.0, 5.0, LINE));
        }
        Icon::Blade => {
            // 刃のひし形。
            out.push(Quad::diamond((cx, cy), (10.0, 14.0), color));
        }
        Icon::Snapping => {
            // 馬蹄形の磁石（両脚と底）。
            fill(Rect::new(cx - 6.0, cy - 7.0, 4.0, 11.0));
            fill(Rect::new(cx + 2.0, cy - 7.0, 4.0, 11.0));
            fill(Rect::new(cx - 6.0, cy + 2.0, 12.0, 5.0));
        }
    }
    out.extend(rects.into_iter().map(|r| Quad::new(r, color)));
    out
}
