//! レイヤーの見出しの列。行の背景、種類の色の帯、有効とロックのボタンのアイコン、Mute と Solo のボタンの枠と背景と、
//! 見出しの列の上の角の背景。文字（略号、名前、M、S、タイムコード）は `header.rs`。矩形は見出しの列の中だけに描く。

use super::{Canvas, Depth, PaintInput};
use crate::geom::{Color, Rect};
use crate::header::{cells, mark_color, row_rect, BUTTON_W};
use crate::palette::Palette;
use crate::timeline::{Hit, Item};

/// 選んだレイヤーの見出しに重ねる色の濃さ。
const SELECTED_ALPHA: f32 = 0.14;
/// ポインターが乗った見出しに重ねる色の濃さ。
const HOVER_ALPHA: f32 = 0.06;
/// 見出しの列の右端の線の濃さ。
const EDGE_ALPHA: f32 = 0.5;
/// 目の形のアイコンの外接矩形と、縁の斜めの段の 1 つの幅。
const EYE_W: f32 = 12.0;
const EYE_H: f32 = 8.0;
const EYE_STEP: f32 = 2.0;
/// 目の形のアイコンの瞳。
const PUPIL: f32 = 4.0;
/// 鍵の形のアイコンの本体と、上の U 字の金具。
const LOCK_BODY: (f32, f32) = (10.0, 6.0);
const LOCK_SHACKLE: (f32, f32) = (6.0, 5.0);
/// ロックしていないときの U 字の金具の、右の柱の切れ目の長さ。
const LOCK_OPEN: f32 = 3.0;
/// Mute と Solo のボタンの枠。
const AUDIO_BUTTON: (f32, f32) = (14.0, 13.0);
/// アイコンと枠の線の太さ。
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
        let back = Rect::new(row.x, row.y + LINE, row.w, row.h - LINE);
        c.push_in(Depth::Back, back, super::stripe(p, i), area);
        for (on, alpha) in [
            (s.state.selected == Some(i), SELECTED_ALPHA),
            (
                matches!(s.state.hover, Hit::Header { index } if index == i),
                HOVER_ALPHA,
            ),
        ] {
            if on {
                c.push_in(Depth::Back, back, p.selection.with_alpha(alpha), area);
            }
        }
        buttons(c, row, item, p, area);
    }
    let edge = Rect::new(area.right() - LINE, area.y, LINE, area.h);
    c.push_in(Depth::Surround, edge, p.tick.with_alpha(EDGE_ALPHA), area);
}

/// 見出しの列の上の角の背景。目盛りの帯と同じ色。
fn corner(s: &PaintInput, c: &mut Canvas) {
    let r = s.vp.layout.corner;
    c.push(Depth::Surround, r, s.state.palette.ruler);
}

/// 1 行の種類の色の帯と、有効、ロック、Mute、Solo のボタン。
fn buttons(c: &mut Canvas, row: Rect, item: &Item, p: &Palette, area: Rect) {
    let cells = cells(row, item);
    c.push_in(Depth::Bar, cells.kind_strip, super::bar_color(item), area);
    eye(
        c,
        centered(cells.enabled, (EYE_W, EYE_H)),
        item.enabled,
        p,
        area,
    );
    lock(c, cells.lock, item.locked, p, area);
    let audio = item.audio.unwrap_or_default();
    for (cell, lit) in [
        (cells.mute, audio.mute.then_some(p.mute)),
        (cells.solo, audio.solo.then_some(p.solo)),
    ] {
        if let Some(cell) = cell {
            audio_button(c, centered(cell, AUDIO_BUTTON), lit, p, area);
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

/// 矩形 `r` の枠の 4 辺。
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

/// 有効のボタンのアイコン（目の形）。縁を上下の線、斜めの段、左右の線で描き、有効なら瞳を入れ、無効なら縁だけを薄く出す。
fn eye(c: &mut Canvas, r: Rect, enabled: bool, p: &Palette, area: Rect) {
    let color = mark_color(p, enabled);
    let (left, right) = (r.x + LINE, r.right() - LINE - EYE_STEP);
    let (upper, lower) = (r.y + LINE, r.bottom() - 2.0 * LINE);
    let inset = LINE + EYE_STEP;
    for edge in [
        Rect::new(r.x + inset, r.y, r.w - 2.0 * inset, LINE),
        Rect::new(r.x + inset, r.bottom() - LINE, r.w - 2.0 * inset, LINE),
        Rect::new(left, upper, EYE_STEP, LINE),
        Rect::new(right, upper, EYE_STEP, LINE),
        Rect::new(left, lower, EYE_STEP, LINE),
        Rect::new(right, lower, EYE_STEP, LINE),
        Rect::new(r.x, r.y + 2.0 * LINE, LINE, r.h - 4.0 * LINE),
        Rect::new(r.right() - LINE, r.y + 2.0 * LINE, LINE, r.h - 4.0 * LINE),
    ] {
        c.push_in(Depth::OnBar, edge, color, area);
    }
    if enabled {
        let pupil = centered(r, (PUPIL, PUPIL));
        c.push_in(Depth::OnBar, pupil, color, area);
    }
}

/// ロックのボタンのアイコン（鍵の形）。ロック中は U 字の金具が閉じて明るく、そうでなければ右の柱が切れて薄い。
fn lock(c: &mut Canvas, cell: Rect, locked: bool, p: &Palette, area: Rect) {
    let color = mark_color(p, locked);
    let total_h = LOCK_BODY.1 + LOCK_SHACKLE.1;
    let top = cell.y + (cell.h - total_h) / 2.0;
    let left = cell.x + (cell.w.min(BUTTON_W) - LOCK_BODY.0) / 2.0;
    let body = Rect::new(left, top + LOCK_SHACKLE.1, LOCK_BODY.0, LOCK_BODY.1);
    let sx = left + (LOCK_BODY.0 - LOCK_SHACKLE.0) / 2.0;
    let (sw, sh) = LOCK_SHACKLE;
    let right_h = if locked { sh } else { sh - LOCK_OPEN };
    for part in [
        body,
        Rect::new(sx, top, sw, LINE),
        Rect::new(sx, top, LINE, sh),
        Rect::new(sx + sw - LINE, top, LINE, right_h),
    ] {
        c.push_in(Depth::OnBar, part, color, area);
    }
}

/// Mute と Solo のボタンの枠と背景。入なら `lit` の色で塗り、切なら薄い枠だけ。
fn audio_button(c: &mut Canvas, r: Rect, lit: Option<Color>, p: &Palette, area: Rect) {
    match lit {
        Some(color) => c.push_in(Depth::OnBar, r, color, area),
        None => frame(c, r, mark_color(p, false), area),
    }
}
