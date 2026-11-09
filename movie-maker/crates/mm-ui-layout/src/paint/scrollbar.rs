//! 縦のスクロールバー。溝は目盛りの帯の色、スクロールボックスは目盛りの時刻の色（溝とのコントラストの比が WCAG 2 の部品の
//! 下限の 3 以上）。ポインターが乗っているか掴んでいる間は、スクロールボックスを選んだ外枠の色にする。

use super::{Canvas, Depth, PaintInput};
use crate::geom::Rect;
use crate::timeline::Hit;

/// スクロールボックスと溝の縁の間隔。
const THUMB_INSET: f32 = 2.0;

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let l = s.vp.layout;
    let track = l.scrollbar;
    if track.w <= 0.0 {
        return;
    }
    let p = &s.state.palette;
    // スクロールバーの上の角も目盛りの帯の色で埋め、帯が右端まで続いて見えるようにする。
    let corner = Rect::new(track.x, l.ruler.y, track.w, l.ruler.h);
    c.push(Depth::Surround, corner, p.ruler);
    c.push(Depth::Surround, track, p.ruler);
    // テーマでは溝と奇数の行の縞が同じ色になるので、左端に目盛りの線の色の境を引く。
    let edge = Rect::new(track.x, track.y, 1.0, track.h);
    c.push(Depth::Surround, edge, p.tick);
    let lit = matches!(s.state.hover, Hit::Scrollbar { thumb: true });
    let color = if lit { p.selection } else { p.label };
    c.push(Depth::Surround, s.vp.thumb().inset(THUMB_INSET), color);
}
