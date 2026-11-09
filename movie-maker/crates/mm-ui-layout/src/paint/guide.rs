//! スナップのガイド線。ドラッグで吸い付いた時刻に細い縦の線を出す。

use super::{vline, Canvas, Scene};

/// ガイド線の太さ。
const GUIDE_W: f32 = 1.0;

pub(super) fn paint(s: &Scene, c: &mut Canvas) {
    if let Some(t) = s.state.guide {
        vline(c, s.vp, t, GUIDE_W, s.state.palette.guide);
    }
}
