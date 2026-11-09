//! レイヤーの見出し。行ごとに、左から種類の色の帯、種類の略号、名前、有効、ロック、Mute、Solo を並べる。
//! 名前はレイヤーの ID（レイヤーに名前の欄は無い）。見出しの列の上の角には再生位置の時刻を出す。座標は Timeline の矩形の左上を (0, 0) とする。
//!
//! 部品の矩形は [`cells`] 1 か所で決め、描く関数（`paint/header.rs`）、文字（[`labels`]）、
//! ボタンの当たり判定が同じ値を見る。この PR は状態の表示だけで、押して切り替えるのは見出しのボタンの PR。

use std::sync::Arc;

use crate::geom::{Color, Rect};
use crate::paint::PaintState;
use crate::palette::{Palette, BAR_TEXT_DARK};
use crate::text::{place, Align, Label, TextMeasure, TextStyle, UiFont};
use crate::timeline::{Item, ItemKind, TimelineModel};
use crate::timeline_layout::Viewport;

/// 種類の色の帯の幅。
pub(crate) const STRIPE_W: f32 = 4.0;
/// 種類の略号を置く幅（帯の右から）。
const ABBR_W: f32 = 30.0;
/// 有効、ロック、Mute、Solo の 1 つの幅。
pub(crate) const BUTTON_W: f32 = 18.0;
/// 見出しの右端の余白。
const RIGHT_PAD: f32 = 4.0;
/// 文字と矩形の端の余白。
const TEXT_PAD: f32 = 3.0;
/// 角の時刻の左の余白。
const CORNER_PAD: f32 = 8.0;
/// 無効なレイヤーの文字の不透明度。帯と同じく薄くする。
const DISABLED_ALPHA: f32 = 0.6;

/// 見出し 1 行の部品の矩形。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cells {
    pub stripe: Rect,
    pub abbr: Rect,
    pub name: Rect,
    pub enabled: Rect,
    pub lock: Rect,
    /// 音のあるレイヤーだけ。
    pub mute: Option<Rect>,
    pub solo: Option<Rect>,
}

/// 行 `i` の見出しの矩形。縦は行と同じ、横は見出しの列の幅。
pub(crate) fn row_rect(vp: &Viewport, i: usize) -> Rect {
    let row = vp.row(i);
    Rect::new(vp.layout.headers.x, row.y, vp.layout.headers.w, row.h)
}

/// 見出しの行 `row` の部品。行の上端の 1 px は行どうしの境の線に残す。
/// ボタンは右から、有効、ロック、Mute、Solo の順に並べ、音の無いレイヤーは Mute と Solo の分を名前に譲る。
pub(crate) fn cells(row: Rect, item: &Item) -> Cells {
    let (y, h) = (row.y + 1.0, (row.h - 1.0).max(0.0));
    let button = |slot: f32| Rect::new(row.right() - RIGHT_PAD - BUTTON_W * slot, y, BUTTON_W, h);
    let audio = item.audio.is_some();
    let enabled = button(if audio { 4.0 } else { 2.0 });
    let name_x = row.x + STRIPE_W + ABBR_W;
    Cells {
        stripe: Rect::new(row.x, y, STRIPE_W, h),
        abbr: Rect::new(row.x + STRIPE_W, y, ABBR_W, h),
        name: Rect::new(name_x, y, (enabled.x - name_x).max(0.0), h),
        enabled,
        lock: button(if audio { 3.0 } else { 1.0 }),
        mute: audio.then(|| button(2.0)),
        solo: audio.then(|| button(1.0)),
    }
}

/// 種類の略号。`Other` は無し。
pub(crate) fn abbreviation(kind: ItemKind) -> Option<&'static str> {
    match kind {
        ItemKind::Footage => Some("V"),
        ItemKind::Image => Some("I"),
        ItemKind::Text => Some("T"),
        ItemKind::Solid => Some("S"),
        ItemKind::Composition => Some("C"),
        ItemKind::Sequence => Some("Seq"),
        ItemKind::Other => None,
    }
}

/// 時刻 `t`（秒）の `HH:MM:SS:FF`（フレームの数え方はフレームレート `fps`）。
pub(crate) fn timecode(t: f64, fps: f64) -> String {
    let rate = fps.round().max(1.0) as u64;
    let frames = (t.max(0.0) * fps.max(1.0)).round() as u64;
    let (secs, ff) = (frames / rate, frames % rate);
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        ff
    )
}

/// `cell` のうち `area`（見出しの列）に見えている所。見えなければ `None`。
pub(crate) fn visible(cell: Rect, area: Rect) -> Option<Rect> {
    let (x0, y0) = (cell.x.max(area.x), cell.y.max(area.y));
    let (x1, y1) = (
        cell.right().min(area.right()),
        cell.bottom().min(area.bottom()),
    );
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// 見出しの文字。見えている行の略号、名前、Mute と Solo の文字と、角の時刻。
pub(crate) fn labels(
    model: &TimelineModel,
    vp: &Viewport,
    state: &PaintState,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    let mut out = Vec::new();
    let area = vp.layout.headers;
    let p = &state.palette;
    let mut put = |text: &Arc<str>, cell: Rect, align, style| {
        let Some(clip) = visible(cell, area) else {
            return;
        };
        if let Some(mut label) = place(text, cell, TEXT_PAD, align, style, measure) {
            label.clip = clip;
            out.push(label);
        }
    };
    for i in vp.visible_rows() {
        let Some(item) = model.items.get(i) else {
            break;
        };
        let c = cells(row_rect(vp, i), item);
        let fade = |color: Color| {
            if item.enabled {
                color
            } else {
                color.with_alpha(DISABLED_ALPHA)
            }
        };
        let name_color = if state.selected == Some(i) {
            p.selection
        } else {
            p.label
        };
        let small = |color| TextStyle {
            size: font.small,
            color,
        };
        if let Some(abbr) = abbreviation(item.kind) {
            put(&abbr.into(), c.abbr, Align::Center, small(fade(p.label)));
        }
        let base = TextStyle {
            size: font.base,
            color: fade(name_color),
        };
        put(&item.id, c.name, Align::Left, base);
        for (cell, letter, on) in [
            (c.mute, "M", item.audio.is_some_and(|a| a.mute)),
            (c.solo, "S", item.audio.is_some_and(|a| a.solo)),
        ] {
            if let Some(cell) = cell {
                put(&letter.into(), cell, Align::Center, small(chip_text(p, on)));
            }
        }
    }
    let time: Arc<str> = timecode(state.playhead, model.fps).into();
    let corner = vp.layout.corner;
    let style = TextStyle {
        size: font.base,
        color: p.selection,
    };
    if let Some(mut label) = place(&time, corner, CORNER_PAD, Align::Left, style, measure) {
        label.clip = corner;
        out.push(label);
    }
    out
}

/// Mute と Solo の字の色。押されている（点いている）ときは、点いた色の上で読める字にする。
pub(crate) fn chip_text(p: &Palette, on: bool) -> Color {
    if on {
        BAR_TEXT_DARK
    } else {
        p.label
    }
}

#[cfg(test)]
mod tests;
