//! ステータスバーの矩形。ツールバーと同じパネルの色で、上端に目盛りの線の色の境界の線を引く（文字は [`crate::status_labels`]）。

use super::{Canvas, Depth, PaintInput};
use crate::geom::Rect;

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let bar = s.vp.layout.status;
    if bar.h <= 0.0 || bar.w <= 0.0 {
        return;
    }
    let p = &s.state.palette;
    c.push(Depth::Surround, bar, p.panel);
    c.push(Depth::Surround, Rect::new(bar.x, bar.y, bar.w, 1.0), p.tick);
}
