//! 目盛りの帯。帯の色と、主な目盛りと細かい目盛りの線。時刻の文字は `labels.rs`。

use super::{Canvas, Layer, Scene};
use crate::geom::Rect;
use crate::ruler::ticks;

/// 主な目盛りと細かい目盛りの線の長さ。
const MAJOR_TICK: f32 = 9.0;
const MINOR_TICK: f32 = 4.0;

pub(super) fn paint(s: &Scene, c: &mut Canvas) {
    let r = s.vp.layout.ruler;
    let p = &s.state.palette;
    c.push(Layer::Chrome, r, p.ruler);
    for t in ticks(&s.vp.time, r.w) {
        let len = if t.major { MAJOR_TICK } else { MINOR_TICK };
        let tick = Rect::new(r.x + t.x, r.bottom() - len, 1.0, len);
        c.push_in(Layer::Chrome, tick, p.tick, r);
    }
}
