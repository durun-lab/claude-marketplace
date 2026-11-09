//! ボタンのアイコン。三角と四角（`QuadShape`）と細い矩形の組み合わせで、中心 `(cx, cy)` に描く。
//! 三角は切ると形が変わるので、切らずに描く（ボタンの矩形に収まる大きさだけを使う）。

use super::Icon;
use crate::geom::{Color, Quad, QuadShape, Rect};

/// 縦線と括弧の線の太さ。
const LINE: f32 = 2.0;
/// アイコンの高さ。
const H: f32 = 12.0;
/// 括弧の横線の長さ。
const ARM: f32 = 5.0;

/// アイコン `icon` の矩形の並び。
pub(super) fn quads(icon: Icon, (cx, cy): (f32, f32), color: Color) -> Vec<Quad> {
    let mut out = Vec::new();
    let mut tri = |x: f32, w: f32, shape: QuadShape| {
        out.push(Quad::shaped((x, cy), (w, H), shape, color));
    };
    let mut lines: Vec<Rect> = Vec::new();
    match icon {
        Icon::GoStart => {
            lines.push(vline(cx - 7.0, cy, H));
            tri(cx - 3.0, 6.0, QuadShape::TriLeft);
            tri(cx + 3.0, 6.0, QuadShape::TriLeft);
        }
        Icon::StepBack => {
            tri(cx - 2.0, 8.0, QuadShape::TriLeft);
            lines.push(vline(cx + 5.0, cy, H));
        }
        Icon::PlayReverse => tri(cx, 10.0, QuadShape::TriLeft),
        Icon::PlayForward => tri(cx, 10.0, QuadShape::TriRight),
        Icon::StepForward => {
            lines.push(vline(cx - 5.0, cy, H));
            tri(cx + 2.0, 8.0, QuadShape::TriRight);
        }
        Icon::GoEnd => {
            tri(cx - 3.0, 6.0, QuadShape::TriRight);
            tri(cx + 3.0, 6.0, QuadShape::TriRight);
            lines.push(vline(cx + 7.0, cy, H));
        }
        Icon::Stop => lines.push(Rect::new(cx - 5.0, cy - 5.0, 10.0, 10.0)),
        Icon::MarkIn => bracket(&mut lines, cx - 3.0, cy, true),
        Icon::MarkOut => bracket(&mut lines, cx + 3.0, cy, false),
        Icon::ClearInOut => {
            bracket(&mut lines, cx - 5.0, cy, true);
            bracket(&mut lines, cx + 5.0, cy, false);
        }
        Icon::Insert => {
            lines.push(vline(cx - 7.0, cy, 10.0));
            lines.push(vline(cx + 6.0, cy, 10.0));
            lines.push(Rect::new(cx - 4.0, cy + 3.0, 8.0, LINE));
            arrow_down(&mut lines, &mut out, (cx, cy - 1.0), color);
        }
        Icon::Overwrite => {
            lines.push(Rect::new(cx - 7.0, cy + 3.0, 14.0, LINE + 1.0));
            arrow_down(&mut lines, &mut out, (cx, cy - 1.0), color);
        }
    }
    out.extend(lines.into_iter().map(|r| Quad::new(r, color)));
    out
}

/// 中心 `x` の縦線。
fn vline(x: f32, cy: f32, h: f32) -> Rect {
    Rect::new(x - LINE / 2.0, cy - h / 2.0, LINE, h)
}

/// 括弧。`open_right` なら `[`（横線が右）、そうでなければ `]`（横線が左）。`x` は縦線の中心。
fn bracket(lines: &mut Vec<Rect>, x: f32, cy: f32, open_right: bool) {
    lines.push(vline(x, cy, H));
    let arm_x = if open_right {
        x - LINE / 2.0
    } else {
        x + LINE / 2.0 - ARM
    };
    lines.push(Rect::new(arm_x, cy - H / 2.0, ARM, LINE));
    lines.push(Rect::new(arm_x, cy + H / 2.0 - LINE, ARM, LINE));
}

/// 下向きの矢印（縦線と三角）。
fn arrow_down(lines: &mut Vec<Rect>, tris: &mut Vec<Quad>, (cx, cy): (f32, f32), color: Color) {
    lines.push(Rect::new(cx - 1.0, cy - 6.0, LINE, 5.0));
    tris.push(Quad::shaped(
        (cx, cy + 1.0),
        (8.0, 6.0),
        QuadShape::TriDown,
        color,
    ));
}
