//! レイヤーの見出しの列。行の背景、種類の色の帯、有効とロックの印、Mute と Solo の地と、角の背景。
//! 文字（略号、名前、M、S、時刻）は `header.rs`。矩形は見出しの列の中だけに描く。

use super::{Canvas, Depth, PaintInput};
use crate::geom::{Color, Rect};
use crate::header::{cells, row_rect, BUTTON_W};
use crate::palette::{self, Palette};
use crate::timeline::{Hit, Item};

/// 選んだレイヤーの見出しに重ねる色の濃さ。
const SELECTED_ALPHA: f32 = 0.14;
/// ポインターが乗った見出しに重ねる色の濃さ。
const HOVER_ALPHA: f32 = 0.06;
/// 切り替えが切れているときの印の不透明度。
const OFF_ALPHA: f32 = 0.45;
/// 目の形の外枠。
const EYE_W: f32 = 12.0;
const EYE_H: f32 = 8.0;
/// 目の形の瞳。
const PUPIL: f32 = 4.0;
/// 鍵の形の本体と、つる。
const LOCK_BODY: (f32, f32) = (10.0, 6.0);
const LOCK_SHACKLE: (f32, f32) = (6.0, 5.0);
/// Mute と Solo の四角。
const CHIP: (f32, f32) = (14.0, 13.0);
/// 印の線の太さ。
const LINE: f32 = 1.0;

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let p = &s.state.palette;
    let area = s.vp.layout.headers;
    corner(s, c);
    for i in s.vp.visible_rows() {
        let Some(item) = s.model.items.get(i) else {
            break;
        };
        let row = row_rect(s.vp, i);
        let band = Rect::new(row.x, row.y + 1.0, row.w, row.h - 1.0);
        c.push_in(Depth::Back, band, super::stripe(p, i), area);
        for (on, alpha) in [
            (s.state.selected == Some(i), SELECTED_ALPHA),
            (
                matches!(s.state.hover, Hit::Header { index } if index == i),
                HOVER_ALPHA,
            ),
        ] {
            if on {
                c.push_in(Depth::Back, band, p.selection.with_alpha(alpha), area);
            }
        }
        cell_marks(s, c, row, item);
    }
    let edge = Rect::new(area.right() - LINE, area.y, LINE, area.h);
    c.push_in(Depth::Surround, edge, p.tick.with_alpha(0.5), area);
}

/// 角の背景。目盛りの帯と同じ色。
fn corner(s: &PaintInput, c: &mut Canvas) {
    let r = s.vp.layout.corner;
    c.push(Depth::Surround, r, s.state.palette.ruler);
}

/// 1 行の種類の帯と、有効、ロック、Mute、Solo の印。
fn cell_marks(s: &PaintInput, c: &mut Canvas, row: Rect, item: &Item) {
    let p = &s.state.palette;
    let area = s.vp.layout.headers;
    let cells = cells(row, item);
    c.push_in(Depth::Bar, cells.stripe, super::bar_color(item), area);
    eye(
        c,
        centered(cells.enabled, (EYE_W, EYE_H)),
        item.enabled,
        p,
        area,
    );
    lock(c, cells.lock, item.locked, p, area);
    let audio = item.audio.unwrap_or_default();
    for (cell, on, color) in [
        (cells.mute, audio.mute, palette::MUTE_ON),
        (cells.solo, audio.solo, palette::SOLO_ON),
    ] {
        if let Some(cell) = cell {
            chip(c, centered(cell, CHIP), on.then_some(color), p, area);
        }
    }
}

/// `cell` の中央に置く大きさ `size` の矩形。
fn centered(cell: Rect, (w, h): (f32, f32)) -> Rect {
    Rect::new(
        cell.x + (cell.w.min(BUTTON_W) - w) / 2.0,
        cell.y + (cell.h - h) / 2.0,
        w,
        h,
    )
}

/// 印の色。点いていれば明るく、切れていれば薄く。
fn mark_color(p: &Palette, on: bool) -> Color {
    if on {
        p.label
    } else {
        p.label.with_alpha(OFF_ALPHA)
    }
}

/// 外枠 `r` の 4 辺。
fn frame(c: &mut Canvas, r: Rect, color: Color, area: Rect) {
    for edge in [
        Rect::new(r.x, r.y, r.w, LINE),
        Rect::new(r.x, r.bottom() - LINE, r.w, LINE),
        Rect::new(r.x, r.y, LINE, r.h),
        Rect::new(r.right() - LINE, r.y, LINE, r.h),
    ] {
        c.push_in(Depth::OnBar, edge, color, area);
    }
}

/// 有効の印（目の形）。有効なら瞳が入り、無効なら外枠だけを薄く出す。
fn eye(c: &mut Canvas, r: Rect, enabled: bool, p: &Palette, area: Rect) {
    frame(c, r, mark_color(p, enabled), area);
    if enabled {
        let pupil = Rect::new(
            r.x + (r.w - PUPIL) / 2.0,
            r.y + (r.h - PUPIL) / 2.0,
            PUPIL,
            PUPIL,
        );
        c.push_in(Depth::OnBar, pupil, p.label, area);
    }
}

/// ロックの印（鍵の形）。ロック中はつるが閉じて明るく、そうでなければつるの右が開いて薄い。
fn lock(c: &mut Canvas, cell: Rect, locked: bool, p: &Palette, area: Rect) {
    let color = mark_color(p, locked);
    let total_h = LOCK_BODY.1 + LOCK_SHACKLE.1;
    let top = cell.y + (cell.h - total_h) / 2.0;
    let left = cell.x + (cell.w.min(BUTTON_W) - LOCK_BODY.0) / 2.0;
    let body = Rect::new(left, top + LOCK_SHACKLE.1, LOCK_BODY.0, LOCK_BODY.1);
    c.push_in(Depth::OnBar, body, color, area);
    let sx = left + (LOCK_BODY.0 - LOCK_SHACKLE.0) / 2.0;
    let (sw, sh) = LOCK_SHACKLE;
    let mut parts = vec![Rect::new(sx, top, sw, LINE), Rect::new(sx, top, LINE, sh)];
    if locked {
        parts.push(Rect::new(sx + sw - LINE, top, LINE, sh));
    } else {
        parts.push(Rect::new(sx + sw - LINE, top, LINE, sh / 2.0 - 1.0));
    }
    for part in parts {
        c.push_in(Depth::OnBar, part, color, area);
    }
}

/// Mute と Solo の四角。点いていれば `lit` の色で塗り、切れていれば薄い枠だけ。
fn chip(c: &mut Canvas, r: Rect, lit: Option<Color>, p: &Palette, area: Rect) {
    match lit {
        Some(color) => c.push_in(Depth::OnBar, r, color, area),
        None => frame(c, r, p.label.with_alpha(OFF_ALPHA), area),
    }
}
