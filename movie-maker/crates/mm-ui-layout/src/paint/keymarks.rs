//! キーフレームのマーカー。行の領域に収まるひし形は切らずに描き、収まらないものと Small の行は縦線にする。

use super::{Canvas, Depth, PaintInput};
use crate::geom::{Quad, Rect};
use crate::keymarks::{is_tick, mark_rect, marks, tick_rect, MARK_PX};
use crate::palette;
use crate::timeline::Hit;

/// ひし形の中の塗りの一辺。外縁は暗い色で、帯の色にかかわらず見える。
const FILL_PX: f32 = MARK_PX - 3.0;

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let clip = s.vp.layout.row_area;
    for i in s.vp.visible_rows() {
        let Some(item) = s.model.items.get(i).filter(|it| it.has_keyframes()) else {
            continue;
        };
        for mark in marks(s.vp, item) {
            let rect = mark_rect(s.vp, i, mark.x);
            let lit =
                matches!(s.state.hover, Hit::Key { item: j, key } if j == i && key == mark.key);
            let fill = if lit {
                s.state.palette.guide
            } else {
                s.state.palette.selection
            };
            if is_tick(s.vp, i, rect) {
                let tick = tick_rect(rect);
                c.push_in(Depth::Mark, grow(tick, 1.0), palette::CUT_SEAM, clip);
                c.push_in(Depth::Mark, tick, fill, clip);
            } else {
                let center = (mark.x, rect.y + MARK_PX / 2.0);
                let size = (MARK_PX, MARK_PX);
                c.push_quad(Depth::Mark, Quad::diamond(center, size, palette::CUT_SEAM));
                let inner = (FILL_PX, FILL_PX);
                c.push_quad(Depth::Mark, Quad::diamond(center, inner, fill));
            }
        }
    }
}

fn grow(r: Rect, by: f32) -> Rect {
    Rect::new(r.x - by, r.y - by, r.w + 2.0 * by, r.h + 2.0 * by)
}
