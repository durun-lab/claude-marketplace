//! ボタンの絵柄。三角と四角（`QuadShape`）と細い矩形の組み合わせで、中心 `(cx, cy)` に描く。
//! 三角は切ると形が変わるので、クリップはしない（ボタンの矩形に収まる大きさだけを使う）。

use super::Icon;
use crate::geom::{Color, Quad, QuadShape, Rect};

/// 棒と括弧の線の太さ。
const LINE: f32 = 2.0;
/// 絵柄の高さ。
const H: f32 = 12.0;

/// 絵柄 `icon` の矩形の並び。`playing` は再生中の [`Icon::PlayStop`] を停止の四角にする。
pub(super) fn quads(icon: Icon, playing: bool, (cx, cy): (f32, f32), color: Color) -> Vec<Quad> {
    let mut out = Vec::new();
    let mut tri = |x: f32, w: f32, shape: QuadShape| {
        out.push(Quad::shaped((x, cy), (w, H), shape, color));
    };
    let mut bars: Vec<Rect> = Vec::new();
    match icon {
        Icon::GoStart => {
            bars.push(vbar(cx - 7.0, cy, H));
            tri(cx - 3.0, 6.0, QuadShape::TriLeft);
            tri(cx + 3.0, 6.0, QuadShape::TriLeft);
        }
        Icon::StepBack => {
            tri(cx - 2.0, 8.0, QuadShape::TriLeft);
            bars.push(vbar(cx + 5.0, cy, H));
        }
        Icon::PlayReverse => tri(cx, 10.0, QuadShape::TriLeft),
        Icon::PlayStop if playing => bars.push(Rect::new(cx - 5.0, cy - 5.0, 10.0, 10.0)),
        Icon::PlayStop | Icon::PlayForward => tri(cx, 10.0, QuadShape::TriRight),
        Icon::StepForward => {
            bars.push(vbar(cx - 5.0, cy, H));
            tri(cx + 2.0, 8.0, QuadShape::TriRight);
        }
        Icon::GoEnd => {
            tri(cx - 3.0, 6.0, QuadShape::TriRight);
            tri(cx + 3.0, 6.0, QuadShape::TriRight);
            bars.push(vbar(cx + 7.0, cy, H));
        }
        Icon::Stop => bars.push(Rect::new(cx - 5.0, cy - 5.0, 10.0, 10.0)),
        Icon::MarkIn => bracket(&mut bars, cx - 3.0, cy, true),
        Icon::MarkOut => bracket(&mut bars, cx + 3.0, cy, false),
        Icon::ClearMarks => {
            bracket(&mut bars, cx - 5.0, cy, true);
            bracket(&mut bars, cx + 5.0, cy, false);
        }
        Icon::Insert => {
            bars.push(vbar(cx - 7.0, cy, 10.0));
            bars.push(vbar(cx + 6.0, cy, 10.0));
            bars.push(Rect::new(cx - 4.0, cy + 3.0, 8.0, LINE));
            arrow_down(&mut bars, &mut out, (cx, cy - 1.0), color);
        }
        Icon::Overwrite => {
            bars.push(Rect::new(cx - 7.0, cy + 3.0, 14.0, LINE + 1.0));
            arrow_down(&mut bars, &mut out, (cx, cy - 1.0), color);
        }
    }
    out.extend(bars.into_iter().map(|r| Quad::new(r, color)));
    out
}

/// 中心 `x` の縦の棒。
fn vbar(x: f32, cy: f32, h: f32) -> Rect {
    Rect::new(x - LINE / 2.0, cy - h / 2.0, LINE, h)
}

/// 括弧。`open_right` なら `[`（腕が右）、そうでなければ `]`（腕が左）。`x` は縦の棒の中心。
fn bracket(bars: &mut Vec<Rect>, x: f32, cy: f32, open_right: bool) {
    bars.push(vbar(x, cy, H));
    let arm_x = if open_right {
        x - LINE / 2.0
    } else {
        x + LINE / 2.0 - ARM
    };
    bars.push(Rect::new(arm_x, cy - H / 2.0, ARM, LINE));
    bars.push(Rect::new(arm_x, cy + H / 2.0 - LINE, ARM, LINE));
}

/// 括弧の腕の長さ。
const ARM: f32 = 5.0;

/// 下向きの矢印（軸と三角）。
fn arrow_down(bars: &mut Vec<Rect>, tris: &mut Vec<Quad>, (cx, cy): (f32, f32), color: Color) {
    bars.push(Rect::new(cx - 1.0, cy - 6.0, LINE, 5.0));
    tris.push(Quad::shaped(
        (cx, cy + 1.0),
        (8.0, 6.0),
        QuadShape::TriDown,
        color,
    ));
}
