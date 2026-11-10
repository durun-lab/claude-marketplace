//! レイヤーの見出し。行ごとに、左から種類の色の帯、種類の略号、名前、有効、ロック、Mute、Solo のボタン、
//! キーフレームの開閉（▸）を並べる。
//! 名前はレイヤーの ID（レイヤーに名前の欄は無く、素材の名前は帯に出す）。Shy のフラグを立てたレイヤーは名前を accent の色で
//! 描く。見出しの列の上の角には再生位置のタイムコード（[`crate::timeline_timecode`] が置く）と、Shy の行の数を出す。
//! 座標は Timeline の矩形の左上を (0, 0) とする。
//!
//! 見出しの部品の矩形は [`cells`] 1 か所で決め、描く関数（`paint/header.rs`）と文字（[`labels`]）が同じ値を見る。
//! ボタンを押すと操作になり（レイヤーは選ばない）、ボタンの外を押すとレイヤーを選ぶ。

use std::sync::Arc;

use crate::geom::{Color, Rect};
use crate::keyframe_rows;
use crate::paint::PaintState;
use crate::palette::{text_on, Palette};
use crate::text::{place, Align, Label, TextMeasure, TextStyle, UiFont};
use crate::timeline::{HeaderButton, Hit, Item, ItemKind, TimelineModel};
use crate::timeline_layout::Viewport;

/// 種類の色の帯の幅。
const KIND_STRIP_W: f32 = 4.0;
/// 種類の略号を置く幅（種類の色の帯の右から）。
const ABBR_W: f32 = 30.0;
/// 有効、ロック、Mute、Solo のボタンの 1 つの幅。
pub(crate) const BUTTON_W: f32 = 18.0;
/// 右端のキーフレームの開閉（▸）の幅。キーフレームの無いレイヤーも空けて、ボタンの列を行ごとにずらさない。
const DISCLOSURE_W: f32 = 12.0;
/// 見出しの右端の余白。
const RIGHT_PAD: f32 = 4.0;
/// 文字と矩形の端の余白。
const TEXT_PAD: f32 = 3.0;
/// 見出しの列の上の角の、タイムコードの左の余白。
pub(crate) const CORNER_PAD: f32 = 8.0;
/// 無効なレイヤーの文字の不透明度。帯と同じく薄くする。
const DISABLED_ALPHA: f32 = 0.6;
/// 切のボタンのアイコンと字の不透明度。入のボタンだけが目立つようにする。
const OFF_ALPHA: f32 = 0.45;
/// フレームを切り捨てる前に足す量。フレームの境目に置いた再生位置が、浮動小数の誤差で前のフレームにならないようにする。
const FRAME_EPS: f64 = 1e-6;

/// 見出し 1 行の部品の矩形。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cells {
    pub kind_strip: Rect,
    pub abbr: Rect,
    pub name: Rect,
    pub enabled: Rect,
    pub lock: Rect,
    /// 音を持てるレイヤーだけ。
    pub mute: Option<Rect>,
    pub solo: Option<Rect>,
    /// キーフレームの開閉（▸）。▸ を描くのはキーフレームのあるレイヤーだけ。
    pub disclosure: Rect,
}

/// 行 `i` の見出しの矩形。縦は行と同じ、横は見出しの列の幅。
pub(crate) fn row_rect(vp: &Viewport, i: usize) -> Rect {
    let row = vp.row(i);
    Rect::new(vp.layout.headers.x, row.y, vp.layout.headers.w, row.h)
}

/// 見出しの行 `row` の部品。行の上端の 1 px は行の境界の線に残す。
/// ボタンはキーフレームの開閉の左に寄せて、左から有効、ロック、Mute、Solo の順に並べる。
/// 音を持てないレイヤーは Mute と Solo の分を名前に譲る。名前の長さでボタンは省かず、入らない名前は切り詰める
/// （Mute と Solo はメニューにもキーにも無く、省くと Timeline から押せなくなる）。
pub(crate) fn cells(row: Rect, item: &Item) -> Cells {
    let (y, h) = (row.y + 1.0, (row.h - 1.0).max(0.0));
    let right = row.right() - RIGHT_PAD - DISCLOSURE_W;
    let button = |slot: f32| Rect::new(right - BUTTON_W * slot, y, BUTTON_W, h);
    let audio = item.audio.is_some();
    let enabled = button(if audio { 4.0 } else { 2.0 });
    let name_x = row.x + KIND_STRIP_W + ABBR_W;
    Cells {
        kind_strip: Rect::new(row.x, y, KIND_STRIP_W, h),
        abbr: Rect::new(row.x + KIND_STRIP_W, y, ABBR_W, h),
        name: Rect::new(name_x, y, (enabled.x - name_x).max(0.0), h),
        enabled,
        lock: button(if audio { 3.0 } else { 1.0 }),
        mute: audio.then(|| button(2.0)),
        solo: audio.then(|| button(1.0)),
        disclosure: Rect::new(right, y, DISCLOSURE_W, h),
    }
}

/// 行 `index` の見出しの点 (x, y)（Timeline の座標）に当たるもの。ボタンの上ならそのボタン、キーフレームのある
/// レイヤーの ▸ の上ならキーフレームの開閉、下に開いたキーフレームの行ならその見出し、ほかは見出し。
pub(crate) fn hit(vp: &Viewport, (index, item): (usize, &Item), (x, y): (f32, f32)) -> Hit {
    if let Some(row) = keyframe_rows::row_at(vp, index, y) {
        return Hit::KeyframeRowHeader { item: index, row };
    }
    let c = cells(row_rect(vp, index), item);
    if item.has_keyframes() && c.disclosure.contains(x, y) {
        return Hit::Disclosure { index };
    }
    let button = [
        (Some(c.enabled), HeaderButton::Enabled),
        (Some(c.lock), HeaderButton::Lock),
        (c.mute, HeaderButton::Mute),
        (c.solo, HeaderButton::Solo),
    ]
    .into_iter()
    .find_map(|(cell, button)| cell.filter(|r| r.contains(x, y)).map(|_| button));
    button.map_or(Hit::Header { index }, |button| Hit::HeaderButton {
        index,
        button,
    })
}

/// 行 `index` のボタン `button` の中心（Timeline の座標）。音を持てないレイヤーの Mute と Solo は `None`。
/// 自動試験がクリックの位置を求めるのに使う。
pub fn button_center(
    vp: &Viewport,
    item: &Item,
    index: usize,
    button: HeaderButton,
) -> Option<(f32, f32)> {
    let c = cells(row_rect(vp, index), item);
    let cell = match button {
        HeaderButton::Enabled => Some(c.enabled),
        HeaderButton::Lock => Some(c.lock),
        HeaderButton::Mute => c.mute,
        HeaderButton::Solo => c.solo,
    }?;
    Some((cell.x + cell.w / 2.0, cell.y + cell.h / 2.0))
}

/// 行 `index` の見出しのキーフレームの開閉（▸）の中心（Timeline の座標）。自動試験がクリックの位置を求めるのに使う。
pub fn disclosure_center(vp: &Viewport, item: &Item, index: usize) -> (f32, f32) {
    let cell = cells(row_rect(vp, index), item).disclosure;
    (cell.x + cell.w / 2.0, cell.y + cell.h / 2.0)
}

/// 行 `index` の見出しの、ボタンの左の名前の中央（Timeline の座標）。見出しをつかむ位置。
pub fn name_center(vp: &Viewport, item: &Item, index: usize) -> (f32, f32) {
    let name = cells(row_rect(vp, index), item).name;
    (name.x + name.w / 2.0, name.y + name.h / 2.0)
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

/// 時刻 `t`（秒）のタイムコード `HH:MM:SS:FF`。フレームは `t` を含むフレーム（WebView の Frame の数と同じく切り捨て）で、
/// 1 秒のフレームの数はフレームレート `fps` を丸めた数（ノンドロップ）。
pub(crate) fn timecode(t: f64, fps: f64) -> String {
    let fps = fps.max(1.0);
    let rate = fps.round() as u64;
    let frames = (t.max(0.0) * fps + FRAME_EPS).floor() as u64;
    let (secs, ff) = (frames / rate, frames % rate);
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        ff
    )
}

/// ボタンのアイコンの色。入なら明るく、切なら薄く。
pub(crate) fn mark_color(p: &Palette, on: bool) -> Color {
    if on {
        p.label
    } else {
        p.label.with_alpha(OFF_ALPHA)
    }
}

/// Mute と Solo のボタンの字の色。入なら背景の色 `lit` の上で読める字、切ならアイコンと同じ薄い字。
fn audio_button_text(p: &Palette, lit: Option<Color>) -> Color {
    lit.map_or_else(|| mark_color(p, false), text_on)
}

/// `cell` のうち `area`（見出しの列）に入る所。入らなければ `None`。
pub(crate) fn inside(cell: Rect, area: Rect) -> Option<Rect> {
    let (x0, y0) = (cell.x.max(area.x), cell.y.max(area.y));
    let (x1, y1) = (
        cell.right().min(area.right()),
        cell.bottom().min(area.bottom()),
    );
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// 見出しの文字。見えている行の略号、名前、Mute と Solo の字と、見出しの列の上の角の Shy の行の数。
/// 角の再生位置のタイムコードは毎フレーム変わるので、ここでは置かない（[`crate::timeline_timecode`]）。
pub(crate) fn labels(
    model: &TimelineModel,
    vp: &Viewport,
    state: &PaintState,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    let mut placer = Placer {
        area: vp.layout.headers,
        measure,
        font,
        palette: &state.palette,
        out: Vec::new(),
    };
    for i in vp.visible_rows() {
        let Some(item) = model.items.get(i) else {
            break;
        };
        placer.row(
            item,
            cells(row_rect(vp, i), item),
            state.selected == Some(i),
        );
    }
    let corner = vp.layout.corner;
    let mut out = placer.out;
    if let Some((text, color)) = shy_count(model, &state.palette) {
        let style = TextStyle {
            size: font.small,
            color,
        };
        out.extend(corner_label(&text, corner, style, placer.measure));
    }
    out
}

/// 見出しの列の上の角の右に出す Shy の行の数。隠していれば隠した数、隠していなければフラグを立てた行の数を、フラグを立てた
/// 名前と同じ accent の色で出す（名前の色が Shy のフラグを示すと読めるように）。どちらも無ければ `None`。
fn shy_count(model: &TimelineModel, p: &Palette) -> Option<(Arc<str>, Color)> {
    if model.hidden > 0 {
        return Some((format!("{} hidden", model.hidden).into(), p.label));
    }
    let marked = model.items.iter().filter(|i| i.shy).count();
    (marked > 0).then(|| (format!("{marked} shy").into(), p.accent))
}

/// 見出しの列の上の角の右に寄せた文字。
fn corner_label(
    text: &Arc<str>,
    corner: Rect,
    style: TextStyle,
    measure: &mut dyn TextMeasure,
) -> Option<Label> {
    let mut label = place(text, corner, CORNER_PAD, Align::Right, style, measure)?;
    label.clip = corner;
    Some(label)
}

/// 見出しの文字を置いていく途中の状態。
struct Placer<'a> {
    area: Rect,
    measure: &'a mut dyn TextMeasure,
    font: &'a UiFont,
    palette: &'a Palette,
    out: Vec<Label>,
}

impl Placer<'_> {
    /// 行 1 本の略号、名前、Mute と Solo の字。無効なレイヤーの略号と名前は薄くする。
    /// Shy のフラグを立てた名前は、選んだ行でも accent の色にして、フラグを立てた結果をすぐ見せる。
    fn row(&mut self, item: &Item, c: Cells, selected: bool) {
        let p = self.palette;
        let fade = |color: Color| {
            if item.enabled {
                color
            } else {
                color.with_alpha(DISABLED_ALPHA)
            }
        };
        if let Some(abbr) = abbreviation(item.kind) {
            self.put(
                &abbr.into(),
                c.abbr,
                Align::Center,
                self.small(fade(p.label)),
            );
        }
        let name = if item.shy {
            p.accent
        } else if selected {
            p.selection
        } else {
            p.label
        };
        let base = TextStyle {
            size: self.font.base,
            color: fade(name),
        };
        self.put(&item.id, c.name, Align::Left, base);
        let audio = item.audio.unwrap_or_default();
        for (cell, letter, lit) in [
            (c.mute, "M", audio.mute.then_some(p.mute)),
            (c.solo, "S", audio.solo.then_some(p.solo)),
        ] {
            if let Some(cell) = cell {
                let style = self.small(audio_button_text(p, lit));
                self.put(&letter.into(), cell, Align::Center, style);
            }
        }
    }

    fn small(&self, color: Color) -> TextStyle {
        TextStyle {
            size: self.font.small,
            color,
        }
    }

    /// `cell` に `text` を置き、見出しの列に入る所で切る。
    fn put(&mut self, text: &Arc<str>, cell: Rect, align: Align, style: TextStyle) {
        let Some(clip) = inside(cell, self.area) else {
            return;
        };
        if let Some(mut label) = place(text, cell, TEXT_PAD, align, style, self.measure) {
            label.clip = clip;
            self.out.push(label);
        }
    }
}

#[cfg(test)]
mod name_tests;
#[cfg(test)]
mod tests;
