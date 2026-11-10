//! レイヤーの見出しの列。行の背景、種類の色の帯、有効とロックのボタンのアイコン、Mute と Solo のボタンの枠と背景、
//! キーフレームの開閉の ▸ と、見出しの列の上の角の背景。文字（略号、名前、M、S、タイムコード）は `header.rs`。矩形は見出しの列の中だけに描く。

use super::{Canvas, Depth, PaintInput};
use crate::geom::{Color, Quad, QuadShape, Rect};
use crate::header::{button_center, cells, mark_color, row_rect, BUTTON_W};
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
/// キーフレームの開閉の ▸（右向きの三角形）の外接する矩形。
const DISCLOSURE: (f32, f32) = (6.0, 8.0);
/// アイコンと枠の線の太さ。
const LINE: f32 = 1.0;
/// ポインターが乗ったボタンの背景に重ねる色の濃さ。
const BUTTON_HOVER_ALPHA: f32 = 0.22;
/// 無効なレイヤーの行の斜線の、点の大きさと線の間隔（横）と濃さ。
const HATCH_DOT: f32 = 3.0;
const HATCH_PERIOD: f32 = 10.0;
const HATCH_ALPHA: f32 = 0.2;
/// 見出しのドラッグで入る位置の線の太さ。
const DROP_LINE: f32 = 2.0;

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
                matches!(
                    s.state.hover,
                    Hit::Header { index }
                        | Hit::HeaderButton { index, .. }
                        | Hit::Disclosure { index } if index == i
                ),
                HOVER_ALPHA,
            ),
        ] {
            if on {
                c.push_in(Depth::Back, back, p.selection.with_alpha(alpha), area);
            }
        }
        if !item.enabled {
            hatch(c, back, p.label.with_alpha(HATCH_ALPHA), area);
        }
        hovered_button(s, c, (i, item), row);
        buttons(c, (row, s.vp.rows.keyframe_rows(i) > 0), item, p, area);
    }
    drop_line(s, c);
    let edge = Rect::new(area.right() - LINE, area.y, LINE, area.h);
    c.push_in(Depth::Surround, edge, p.tick.with_alpha(EDGE_ALPHA), area);
}

/// 無効なレイヤーの行の斜線（右上がり）。小さな正方形を斜めに並べ、行の矩形と見出しの列の両方に収める。
fn hatch(c: &mut Canvas, row: Rect, color: Color, area: Rect) {
    let Some(clip) = crate::header::inside(row, area) else {
        return;
    };
    let steps = (row.h / HATCH_DOT).ceil() as usize;
    let mut x = row.x - row.h;
    while x < row.right() {
        for k in 0..steps {
            let k = k as f32;
            let dot = Rect::new(
                x + k * HATCH_DOT,
                row.bottom() - (k + 1.0) * HATCH_DOT,
                HATCH_DOT,
                HATCH_DOT,
            );
            c.push_in(Depth::OnBar, dot, color, clip);
        }
        x += HATCH_PERIOD;
    }
}

/// ポインターが乗っているボタンとキーフレームの開閉の背景を明るくする（押せることが分かるように）。
fn hovered_button(s: &PaintInput, c: &mut Canvas, (i, item): (usize, &Item), row: Rect) {
    let (cx, w) = match s.state.hover {
        Hit::HeaderButton { index, button } if index == i => {
            let Some((cx, _)) = button_center(s.vp, item, i, button) else {
                return;
            };
            (cx, BUTTON_W)
        }
        Hit::Disclosure { index } if index == i => {
            let d = cells(row, item).disclosure;
            (d.x + d.w / 2.0, d.w)
        }
        _ => return,
    };
    let r = Rect::new(cx - w / 2.0, row.y + LINE, w, row.h - LINE);
    let color = s.state.palette.selection.with_alpha(BUTTON_HOVER_ALPHA);
    c.push_in(Depth::Back, r, color, s.vp.layout.headers);
}

/// 見出しのドラッグで入る位置の線。入る位置の行の境界に、見出しの列と行の領域を横に貫いて引く。
fn drop_line(s: &PaintInput, c: &mut Canvas) {
    let Some(boundary) = s.state.header_drop else {
        return;
    };
    let Some(last) = s.model.items.len().checked_sub(1) else {
        return;
    };
    // 最後の行の下は、開いたキーフレームの行の下。
    let y = if boundary <= last {
        row_rect(s.vp, boundary).y
    } else {
        s.vp.layout.row_area.y + s.vp.rows.below(last) - s.vp.scroll_y
    };
    let l = s.vp.layout;
    for area in [l.headers, l.row_area] {
        let line = Rect::new(area.x, y - DROP_LINE / 2.0, area.w, DROP_LINE);
        c.push_in(Depth::Surround, line, s.state.palette.selection, area);
    }
}

/// 見出しの列の上の角の背景。目盛りの帯と同じ色。
fn corner(s: &PaintInput, c: &mut Canvas) {
    let r = s.vp.layout.corner;
    c.push(Depth::Surround, r, s.state.palette.ruler);
}

/// 1 行の種類の色の帯と、有効、ロック、Mute、Solo のボタンと、キーフレームの開閉（`open` は下に開いているか）。
fn buttons(c: &mut Canvas, (row, open): (Rect, bool), item: &Item, p: &Palette, area: Rect) {
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
    if item.has_keyframes() {
        disclosure(c, (centered(cells.disclosure, DISCLOSURE), open), p, area);
    }
}

/// キーフレームの開閉。閉じていれば ▸、開いていれば ▾。三角形は切ると形が変わるので、見出しの列に全部収まる行だけに描く。
fn disclosure(c: &mut Canvas, (r, open): (Rect, bool), p: &Palette, area: Rect) {
    let horizontal = r.x >= area.x && r.right() <= area.right();
    if horizontal && r.y >= area.y && r.bottom() <= area.bottom() {
        let center = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let (shape, size) = if open {
            (QuadShape::TriDown, (r.h, r.w))
        } else {
            (QuadShape::TriRight, (r.w, r.h))
        };
        let tri = Quad::shaped(center, size, shape, mark_color(p, true));
        c.push_quad(Depth::OnBar, tri);
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
