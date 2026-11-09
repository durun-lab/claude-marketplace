//! 再生位置の線と、その上端の四角。

use super::{vline, Canvas, Layer, Scene};
use crate::geom::Rect;

/// 再生位置の線の幅。
const PLAYHEAD_W: f32 = 2.0;
/// 再生位置の線の上端に付ける四角の一辺。
const PLAYHEAD_CAP: f32 = 10.0;

pub(super) fn paint(s: &Scene, c: &mut Canvas) {
    let color = s.state.palette.playhead;
    vline(c, s.vp, s.state.playhead, PLAYHEAD_W, color);
    let column = s.vp.layout.time_column();
    let x = s.vp.x_of(s.state.playhead);
    let cap = Rect::new(
        x - PLAYHEAD_CAP / 2.0,
        column.y,
        PLAYHEAD_CAP,
        PLAYHEAD_CAP * 0.8,
    );
    c.push_in(Layer::Over, cap, color, column);
}
