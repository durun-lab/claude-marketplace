//! キーフレームの行。見えている行の見出しの背景（レイヤーの種類の色の帯を続ける）、行の背景（レイヤーの範囲を明るく）、
//! キーフレームのひし形。ひし形は行の領域に収まるものだけを切らずに描き、収まらないものは縦線にして切る（帯のマーカーと同じ）。
//! ひし形は横に見えているものだけを描き、帯のマーカーをまとめる幅より近いものは 1 つだけ描く（毎フレームにキーフレームを
//! 持つ行を縮めて見ても、描く数が行の幅で頭打ちになる）。選んだものとポインターが乗ったものは、まとめずに上に描く。
//! 選んだキーフレームは accent の輪で囲み、ドラッグ中のキーフレームは動かした先に描く。レイヤーを動かすドラッグ
//! （`shiftLayer`）の間は、帯と帯のマーカーと同じく、行の範囲とひし形を動かした後の時刻で描く。行の名前の文字は
//! `keyframe_rows.rs`。

use super::{bar_color, stripe, trimmed, Canvas, Depth, PaintInput};
use crate::geom::{Quad, Rect};
use crate::gesture::EditKind;
use crate::keyframe_rows::{
    header_rect, keyframe_center, keyframes_within, merge_column, row_rect, square, KeyframeIndex,
    KeyframeRow, ROW_DIAMOND_PX,
};
use crate::palette;
use crate::timeline::{Hit, Item};

/// ひし形の中の一辺。周りに縁を残し、行の色にかかわらず形が見えるようにする。
const INNER_PX: f32 = ROW_DIAMOND_PX - 4.0;
/// 選んだひし形を囲む輪の一辺。
const RING_PX: f32 = ROW_DIAMOND_PX + 4.0;
/// 行の領域のうち、レイヤーの範囲に重ねる明るさ。
const SPAN_ALPHA: f32 = 0.05;
/// 選んだ行の見出しに重ねる明るさ（レイヤーの見出しで選んだ行と同じ）。
const SELECTED_ALPHA: f32 = 0.14;
/// ポインターが乗った行の見出しに重ねる明るさ（レイヤーの見出しと同じ）。
const HOVER_ALPHA: f32 = 0.06;
/// 収まらないひし形の代わりの縦線の幅。
const LINE_W: f32 = 2.0;
/// 種類の色の帯の幅（レイヤーの見出しと同じ）。
const KIND_STRIP_W: f32 = 4.0;

/// ひし形の見た目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    Normal,
    Hovered,
    Selected,
}

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let area = s.vp.layout.row_area;
    for i in s.vp.visible_rows() {
        let Some(item) = s.model.items.get(i) else {
            break;
        };
        let open = s.vp.rows.keyframe_rows(i);
        for (k, row) in item.keyframe_rows.iter().enumerate().take(open) {
            let r = row_rect(s.vp, i, k);
            if r.bottom() <= area.y || r.y >= area.bottom() {
                continue;
            }
            backgrounds(s, c, (i, item), k);
            diamonds(s, c, (i, k), row);
        }
    }
}

/// 行 `k` の見出しと行の領域の背景。選んだ行とポインターが乗った行の見出しは明るくする。
fn backgrounds(s: &PaintInput, c: &mut Canvas, (i, item): (usize, &Item), k: usize) {
    let p = &s.state.palette;
    let (headers, area) = (s.vp.layout.headers, s.vp.layout.row_area);
    let head = back(header_rect(s.vp, i, k));
    let row = back(row_rect(s.vp, i, k));
    c.push_in(Depth::Back, head, stripe(p, i), headers);
    c.push_in(Depth::Back, row, stripe(p, i), area);
    let strip = Rect::new(head.x, head.y, KIND_STRIP_W, head.h);
    c.push_in(Depth::Bar, strip, bar_color(item), headers);
    let (start, end) = trimmed(item, i, s.state.preview);
    let (x0, x1) = (s.vp.x_of(start), s.vp.x_of(end));
    let span = Rect::new(x0, row.y, x1 - x0, row.h);
    c.push_in(Depth::Back, span, p.selection.with_alpha(SPAN_ALPHA), area);
    let this = |item: usize, row: usize| (item, row) == (i, k);
    let picked = s
        .state
        .keyframe_rows
        .selected
        .is_some_and(|pk| this(pk.item, pk.row));
    let hovered = match s.state.hover {
        Hit::KeyframeRowHeader { item, row } | Hit::KeyframeRow { item, row } => this(item, row),
        Hit::RowKeyframe(at) => this(at.item, at.row),
        _ => false,
    };
    for (on, alpha) in [(picked, SELECTED_ALPHA), (hovered, HOVER_ALPHA)] {
        if on {
            c.push_in(Depth::Back, head, p.selection.with_alpha(alpha), headers);
        }
    }
}

/// 行の背景。上端の 1 px を行の境界の線に残した矩形（見出しの行の背景と同じ）。
fn back(r: Rect) -> Rect {
    Rect::new(r.x, r.y + 1.0, r.w, (r.h - 1.0).max(0.0))
}

/// ドラッグ中の `shiftLayer` で、レイヤー `i` のキーフレームが帯と一緒に動く量（秒）。
fn shifted_by(s: &PaintInput, i: usize) -> f64 {
    match s.state.preview {
        Some((EditKind::ShiftLayer { item }, by)) if item == i => by,
        _ => 0.0,
    }
}

/// 行 `k` の横に見えているキーフレームのひし形。近いものは列ごとに 1 つだけ描き、選んだものとポインターが乗ったものは
/// 後から上に描く。ドラッグ中のキーフレームは元の位置に描かず、動かした先に最後に描く。
fn diamonds(s: &PaintInput, c: &mut Canvas, (i, k): (usize, usize), row: &KeyframeRow) {
    let st = &s.state.keyframe_rows;
    let dragging = st.dragging.filter(|(at, _)| (at.item, at.row) == (i, k));
    let area = s.vp.layout.row_area;
    let by = shifted_by(s, i);
    // 動かした後の時刻で見える範囲を、動かす前の時刻の範囲に戻して引く。
    let dx = (by * s.vp.time.px_per_sec) as f32;
    let half = RING_PX / 2.0;
    let mut column = None;
    let mut on_top = Vec::new();
    for j in keyframes_within(s.vp, row, (area.x - half - dx, area.right() + half - dx)) {
        if dragging.is_some_and(|(d, _)| d.keyframe == j) {
            continue;
        }
        let at = KeyframeIndex {
            item: i,
            row: k,
            keyframe: j,
        };
        let time = row.keyframes[j].time;
        match look(s, at) {
            Look::Normal => {
                let col = merge_column(s.vp, time);
                if column.replace(col) != Some(col) {
                    diamond(s, c, (i, k), time + by, Look::Normal);
                }
            }
            other => on_top.push((time + by, other)),
        }
    }
    for (time, look) in on_top {
        diamond(s, c, (i, k), time, look);
    }
    if let Some((_, to)) = dragging {
        diamond(s, c, (i, k), to, Look::Selected);
    }
}

/// キーフレーム `at` の見た目。選んだものはホバーより先。
fn look(s: &PaintInput, at: KeyframeIndex) -> Look {
    if s.state
        .keyframe_rows
        .selected
        .and_then(|p| p.keyframe_index())
        == Some(at)
    {
        Look::Selected
    } else if s.state.hover == Hit::RowKeyframe(at) {
        Look::Hovered
    } else {
        Look::Normal
    }
}

/// 時刻 `t` のひし形。輪まで行の領域に収まれば切らずに描き、収まらなければ縦線にして切る。
/// 縁と中の色は帯のマーカーと同じ（ホバーと選択は中を Inspector で選んだキーフレームと同じ accent にする）。
fn diamond(s: &PaintInput, c: &mut Canvas, (i, k): (usize, usize), t: f64, look: Look) {
    let p = &s.state.palette;
    let area = s.vp.layout.row_area;
    let center = keyframe_center(s.vp, (i, k), t);
    let (rim, fill) = match look {
        Look::Normal => (palette::CUT_SEAM, p.selection),
        Look::Hovered | Look::Selected => (p.selection, p.accent),
    };
    let selected = look == Look::Selected;
    let outer = square(center, if selected { RING_PX } else { ROW_DIAMOND_PX });
    let fits = outer.x >= area.x
        && outer.y >= area.y
        && outer.right() <= area.right()
        && outer.bottom() <= area.bottom();
    if !fits {
        let (x, y) = (center.0 - LINE_W / 2.0, center.1 - ROW_DIAMOND_PX / 2.0);
        c.push_in(
            Depth::BarMark,
            Rect::new(x, y, LINE_W, ROW_DIAMOND_PX),
            fill,
            area,
        );
        return;
    }
    if selected {
        c.push_quad(
            Depth::BarMark,
            Quad::diamond(center, (RING_PX, RING_PX), p.accent),
        );
    }
    c.push_quad(
        Depth::BarMark,
        Quad::diamond(center, (ROW_DIAMOND_PX, ROW_DIAMOND_PX), rim),
    );
    c.push_quad(
        Depth::BarMark,
        Quad::diamond(center, (INNER_PX, INNER_PX), fill),
    );
}
