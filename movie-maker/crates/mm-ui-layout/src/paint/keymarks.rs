//! 帯のマーカー。行の領域に収まるひし形は切らずに描き、収まらないものと Small の行は縦線にする。

use super::{Canvas, Depth, PaintInput};
use crate::geom::{Color, Quad, Rect};
use crate::gesture::EditKind;
use crate::keymarks::{is_line, line_rect, mark_rect, marks, BarMark, MARK_PX};
use crate::palette;
use crate::timeline::Hit;

/// ひし形の中の一辺。周りに縁を残し、帯の色にかかわらず形が見えるようにする。
const INNER_PX: f32 = MARK_PX - 3.0;

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let clip = s.vp.layout.row_area;
    for i in s.vp.visible_rows() {
        let Some(item) = s.model.items.get(i).filter(|it| it.has_keyframes()) else {
            continue;
        };
        // ドラッグ中の `shiftLayer` はキーフレームも帯と一緒に動くので、描く位置も同じ量ずらす。
        let dx = match s.state.preview {
            Some((EditKind::ShiftLayer { item }, by)) if item == i => {
                (by * s.vp.time.px_per_sec) as f32
            }
            _ => 0.0,
        };
        for mark in marks(s.vp, item) {
            let mark = BarMark {
                x: mark.x + dx,
                ..mark
            };
            let rect = mark_rect(s.vp, i, mark.x);
            let (rim, fill) = colors(s, i, mark);
            if is_line(s.vp, i, rect) {
                let line = line_rect(rect);
                c.push_in(Depth::BarMark, grow(line, 1.0), rim, clip);
                c.push_in(Depth::BarMark, line, fill, clip);
            } else {
                let center = (mark.x, rect.y + MARK_PX / 2.0);
                let outer = Quad::diamond(center, (MARK_PX, MARK_PX), rim);
                let inner = Quad::diamond(center, (INNER_PX, INNER_PX), fill);
                c.push_quad(Depth::BarMark, outer);
                c.push_quad(Depth::BarMark, inner);
            }
        }
    }
}

/// 行 `i` のマーカー `mark` の縁と中の色。ホバーしたものは、中を Inspector のキーフレームの欄で選んだひし形と
/// 同じ accent にし、縁を明るくする（暗い縁のままだと、accent に近い青の帯の上で中が帯に紛れる）。
fn colors(s: &PaintInput, i: usize, mark: BarMark) -> (Color, Color) {
    let p = &s.state.palette;
    let hovered = Hit::BarMark {
        item: i,
        keyframe: mark.keyframe,
    };
    if s.state.hover == hovered {
        (p.selection, p.accent)
    } else {
        (palette::CUT_SEAM, p.selection)
    }
}

fn grow(r: Rect, by: f32) -> Rect {
    Rect::new(r.x - by, r.y - by, r.w + 2.0 * by, r.h + 2.0 * by)
}
