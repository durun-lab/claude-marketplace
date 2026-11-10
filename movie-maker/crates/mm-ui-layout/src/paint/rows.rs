//! 行の領域。見えている行の縞、帯、カット、つまみ、選んだ外枠と、コンポジションの終わりより右の暗がり。
//! 行の領域の外（目盛りの帯の下に隠れた所）にはみ出す分は切って描く。

use super::{Canvas, Depth, PaintInput};
use crate::geom::{Color, Rect};
use crate::gesture::{EditKind, Side};
use crate::palette::{self, Palette};
use crate::timeline::{bar_rect, cut_span, Hit, Item, Part, HANDLE_PX};

/// 空の sequence の帯の塗りに掛ける不透明度。再生位置の線の隣でも帯と分かるよう、カットを並べた帯の薄い塗り（0.25）より
/// 濃くする。無効なレイヤーは帯の色の不透明度に掛けて、ほかの帯と同じく薄くする。
pub(crate) const EMPTY_FILL_ALPHA: f32 = 0.6;
/// 空の sequence の帯の枠を、帯の色から明るくする量。
pub(crate) const EMPTY_EDGE_LIGHTEN: f32 = 0.6;
/// 無効なレイヤーの帯の不透明度。
const DISABLED_BAR_ALPHA: f32 = 0.35;

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let clip = s.vp.layout.row_area;
    for i in s.vp.visible_rows() {
        let Some(item) = s.model.items.get(i) else {
            break;
        };
        let row = s.vp.row(i);
        // 行の上端の 1 px は背景のまま残し、行どうしの境の線にする。
        let band = Rect::new(row.x, row.y + 1.0, row.w, row.h - 1.0);
        c.push_in(Depth::Back, band, stripe(&s.state.palette, i), clip);
        let (start, end) = trimmed(item, i, s.state.preview);
        let bar = bar_rect(s.vp, i, item, (start, end));
        if item.cuts.is_empty() {
            layer_bar(s, c, (i, item), bar);
        } else {
            c.push_in(Depth::Bar, bar, bar_color(item).with_alpha(0.25), clip);
            cuts(s, c, (i, item), bar);
        }
        if s.state.selected == Some(i) {
            outline(c, bar, s.state.palette.selection, clip);
        }
    }
    outside(s, c);
}

/// カットを持たないレイヤーの帯とつまみ。空の sequence の帯は中身が無いので、薄い塗りに明るい枠を引く。当たり判定は
/// ほかの帯と同じくレイヤーの帯とつまみを返すので、ポインターが乗ったときの明るさとつまみも同じく描く。
fn layer_bar(s: &PaintInput, c: &mut Canvas, (index, item): (usize, &Item), bar: Rect) {
    let hovered = matches!(s.state.hover, Hit::Layer { index: i, .. } if i == index);
    let color = bar_color(item);
    let color = if hovered {
        palette::lighten(color, 0.18)
    } else {
        color
    };
    let clip = s.vp.layout.row_area;
    if item.is_empty_sequence() {
        let fill = color.with_alpha(color.0[3] * EMPTY_FILL_ALPHA);
        c.push_in(Depth::Bar, bar, fill, clip);
        outline(c, bar, palette::lighten(color, EMPTY_EDGE_LIGHTEN), clip);
    } else {
        c.push_in(Depth::Bar, bar, color, clip);
    }
    handles(s, c, bar, |hit| match hit {
        Hit::Layer { index: i, part } if i == index => Some(part),
        _ => None,
    });
}

/// 行 `index` の背景の色（縞）。
pub(crate) fn stripe(p: &Palette, index: usize) -> Color {
    if index.is_multiple_of(2) {
        p.row_even
    } else {
        p.row_odd
    }
}

/// レイヤーの帯の色。無効なレイヤーは行の背景に薄く重ねる。
pub(crate) fn bar_color(item: &Item) -> Color {
    let color = palette::item_color(item.kind);
    if item.enabled {
        color
    } else {
        color.with_alpha(DISABLED_BAR_ALPHA)
    }
}

/// 帯の始まりと終わり。ドラッグ中の `trimLayer` と `shiftLayer` は動かした後の位置にする。
/// 確定は離したときの 1 回の操作に任せ、ここでは描く位置だけを変える。
pub(crate) fn trimmed(item: &Item, index: usize, preview: Option<(EditKind, f64)>) -> (f64, f64) {
    match preview {
        Some((EditKind::TrimLayer { item: i, side }, by)) if i == index => match side {
            Side::In => (item.start + by, item.end),
            Side::Out => (item.start, item.end + by),
        },
        Some((EditKind::ShiftLayer { item: i }, by)) if i == index => {
            (item.start + by, item.end + by)
        }
        _ => (item.start, item.end),
    }
}

/// シーケンスのカットを並べて描く。隙間は枠だけの空き、選んだカットは外枠、ホバーしたカットの端はつまみを出す。
/// カットはレイヤーの帯 `layer` の内側だけを描く（帯の外はレイヤーのイン点とアウト点で切られて映らない）。
fn cuts(s: &PaintInput, c: &mut Canvas, (index, item): (usize, &Item), layer: Rect) {
    let p = &s.state.palette;
    let clip = s.vp.layout.row_area;
    let base = palette::item_color(item.kind);
    for (k, cut) in item.cuts.iter().enumerate() {
        let (cx, cw) = cut_span(&s.vp.time, cut);
        let cx = cx + clip.x;
        let x = cx.max(layer.x);
        let w = (cx + cw).min(layer.right()) - x;
        if w <= 0.0 || x >= clip.right() || x + w <= clip.x {
            continue;
        }
        let r = Rect::new(x, layer.y, w, layer.h);
        let hovered =
            matches!(s.state.hover, Hit::Cut { item: i, cut, .. } if i == index && cut == k);
        if cut.gap {
            c.push_in(Depth::Bar, r, palette::GAP.with_alpha(0.5), clip);
            outline(c, r, p.selection, clip);
        } else {
            let color = if hovered {
                palette::lighten(base, 0.18)
            } else {
                base
            };
            c.push_in(Depth::Bar, r, color, clip);
            // 隣のカットとの編集点が見えるように、左端に暗い線を引く。
            let seam = Rect::new(x, layer.y, 1.0, layer.h);
            c.push_in(Depth::Bar, seam, palette::CUT_SEAM, clip);
        }
        if hovered {
            handles(s, c, r, |hit| match hit {
                Hit::Cut { item: i, cut, part } if i == index && cut == k => Some(part),
                _ => None,
            });
        }
        if s.state.selected_cut == Some((index, k)) {
            outline(c, r, p.selection, clip);
        }
    }
}

/// 帯 `bar` の左右のつまみ。`lit` はポインターが乗っているつまみを返す。
fn handles(s: &PaintInput, c: &mut Canvas, bar: Rect, lit: impl Fn(Hit) -> Option<Part>) {
    let hw = HANDLE_PX.min(bar.w / 3.0);
    let on = lit(s.state.hover);
    let selection = s.state.palette.selection;
    for (part, x) in [(Part::InHandle, bar.x), (Part::OutHandle, bar.right() - hw)] {
        let alpha = if on == Some(part) { 0.85 } else { 0.14 };
        let r = Rect::new(x, bar.y, hw, bar.h);
        c.push_in(
            Depth::OnBar,
            r,
            selection.with_alpha(alpha),
            s.vp.layout.row_area,
        );
    }
}

fn outline(c: &mut Canvas, r: Rect, color: Color, clip: Rect) {
    for edge in [
        Rect::new(r.x, r.y, r.w, 1.0),
        Rect::new(r.x, r.bottom() - 1.0, r.w, 1.0),
        Rect::new(r.x, r.y, 1.0, r.h),
        Rect::new(r.right() - 1.0, r.y, 1.0, r.h),
    ] {
        c.push_in(Depth::OnBar, edge, color, clip);
    }
}

/// コンポジションの終わりより右を、行のある高さだけ暗くする。
fn outside(s: &PaintInput, c: &mut Canvas) {
    let area = s.vp.layout.row_area;
    let end = s.vp.x_of(s.model.duration).max(area.x);
    let content = s.vp.rows.content_height() - s.vp.scroll_y;
    let shade = Rect::new(end, area.y, area.right() - end, content.min(area.h));
    c.push_in(Depth::Shade, shade, s.state.palette.outside, area);
}
