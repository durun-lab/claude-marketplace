//! 縦のスクロールバー。溝とつまみ。ポインターが乗っているか掴んでいる間は、つまみを明るくする。

use super::{Canvas, Layer, Scene};
use crate::geom::Rect;
use crate::timeline::Hit;

/// つまみと溝の縁の間隔。
const THUMB_INSET: f32 = 2.0;

pub(super) fn paint(s: &Scene, c: &mut Canvas) {
    let l = s.vp.layout;
    let track = l.scrollbar;
    if track.w <= 0.0 {
        return;
    }
    let p = &s.state.palette;
    // スクロールバーの上の角も目盛りの帯の色で埋め、帯が右端まで続いて見えるようにする。
    let corner = Rect::new(track.x, l.ruler.y, track.w, l.ruler.h);
    c.push(Layer::Chrome, corner, p.ruler);
    c.push(Layer::Chrome, track, p.ruler);
    let lit = matches!(s.state.hover, Hit::Scrollbar { thumb: true });
    let color = if lit { p.label } else { p.tick };
    c.push(Layer::Chrome, s.vp.thumb().inset(THUMB_INSET), color);
}
