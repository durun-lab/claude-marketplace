//! Blade のときの、割る時刻の縦線。ポインターの下のカットの帯の高さに、カーソルの形（Z5）が無くても割る位置が分かる線を引く
//! （Final Cut Pro の Blade のスキマーと同じ考え）。色は FCP のスキマーと同じく、再生位置の線と見分けられる色
//! （テーマの `text`）にし、再生位置に吸い付いたときだけスナップのガイド線の色にする。

use super::{Canvas, Depth, PaintInput};
use crate::geom::Rect;

/// 線の幅。
const LINE_W: f32 = 2.0;
/// 帯の上下に出す長さ。帯の外枠より少し長くし、帯の色に紛れさせない。
const OVERSHOOT: f32 = 3.0;

/// 吸い付いた線は再生位置の線と同じ x に立つので、再生位置の線より上（`Depth::Pointer`）に描く。
pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let Some(line) = s.blade_line else {
        return;
    };
    if !s.vp.visible_rows().contains(&line.item) {
        return;
    }
    let Some(it) = s.model.items.get(line.item) else {
        return;
    };
    let bar = s.vp.bar(line.item, it.start, it.end);
    let x = s.vp.x_of(line.time) - LINE_W / 2.0;
    let rect = Rect::new(x, bar.y - OVERSHOOT, LINE_W, bar.h + 2.0 * OVERSHOOT);
    let palette = &s.state.palette;
    let color = if line.snapped {
        palette.guide
    } else {
        palette.selection
    };
    c.push_in(Depth::Pointer, rect, color, s.vp.layout.row_area);
}
