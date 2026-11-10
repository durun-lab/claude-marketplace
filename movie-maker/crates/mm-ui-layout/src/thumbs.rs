//! 帯の先頭のサムネイル。Medium と Large の行の帯（シーケンスはカットごと）の左に、素材の 1 コマを出す。
//! サムネイルは矩形ではなくテクスチャで描くので、矩形の並び（[`crate::timeline_quads`]）とは別の並びにし、
//! `mm-ui-wgpu` が帯と帯の中の波形の後、帯に重ねる矩形の前（[`crate::UnderText::bars`]）に描く。
//! 名前はサムネイルの右に寄せる（Premiere と同じ）。
//! 座標は Timeline の矩形の左上を (0, 0) とする。

use std::sync::Arc;

use crate::geom::Rect;
use crate::paint::{trimmed, PaintState};
use crate::rows::RowHeight;
use crate::timeline::{cut_span, CutBar, TimelineModel};
use crate::timeline_layout::Viewport;

/// サムネイルの幅と高さの比。サムネイルはこの比に切り出して持つ（`mm-ui-wgpu` のアトラスの 1 スロットの比）。
pub const THUMB_ASPECT: f32 = 16.0 / 9.0;
/// 無効なレイヤーのサムネイルの不透明度。帯の薄さに合わせる。
const DISABLED_ALPHA: f32 = 0.35;

/// 描くサムネイル 1 枚。
#[derive(Debug, Clone, PartialEq)]
pub struct ThumbQuad {
    /// サムネイルのキー（素材のコンテンツハッシュ）。
    pub key: Arc<str>,
    /// 置く矩形（行の領域で切った後）。
    pub rect: Rect,
    /// サムネイルの中の、`rect` に当たる部分（0..1。行の領域の上端や下端で切れたときだけ全体より小さい）。
    pub uv: Rect,
    pub alpha: f32,
}

/// 高さ `bar_h` の帯に置くサムネイルの幅。
fn thumb_width(bar_h: f32) -> f32 {
    (bar_h * THUMB_ASPECT).round()
}

/// 帯の矩形 `span`（Timeline の座標。帯の高さがサムネイルの高さ）に置くサムネイルの矩形。
/// 行の高さが Small、または見えている幅がサムネイルの幅に満たなければ `None`。
/// 帯の左端が画面の外に出てもサムネイルが読めるよう、名前と同じく帯の見えている所の左に寄せる。
fn thumb_box(vp: &Viewport, row: usize, span: Rect) -> Option<Rect> {
    if vp.rows.height(row) == RowHeight::Small {
        return None;
    }
    let area = vp.layout.row_area;
    let left = span.x.max(area.x);
    let w = thumb_width(span.h);
    let fits = w > 0.0 && span.right().min(area.right()) - left >= w;
    fits.then(|| Rect::new(left, span.y, w, span.h))
}

/// 名前を右へずらす幅。サムネイルが出ない帯は 0。
pub(crate) fn name_inset(vp: &Viewport, row: usize, key: Option<&Arc<str>>, span: Rect) -> f32 {
    key.and_then(|_| thumb_box(vp, row, span))
        .map_or(0.0, |b| b.w)
}

/// シーケンスのカットの、レイヤーの帯 `bar` の内側に見えている矩形（Timeline の座標）。
pub(crate) fn cut_rect(vp: &Viewport, bar: Rect, cut: &CutBar) -> Rect {
    let (cx, cw) = cut_span(&vp.time, cut);
    let edge = vp.layout.row_area.x;
    let left = (edge + cx).max(bar.x);
    let right = (edge + cx + cw).min(bar.right());
    Rect::new(left, bar.y, right - left, bar.h)
}

/// 見えている行のサムネイル。行の領域の上端と下端（目盛りの帯の下に隠れた所）で切る。
pub fn timeline_thumbs(model: &TimelineModel, vp: &Viewport, state: &PaintState) -> Vec<ThumbQuad> {
    let mut out = Vec::new();
    let area = vp.layout.row_area;
    for i in vp.visible_rows() {
        let Some(item) = model.items.get(i) else {
            break;
        };
        let (start, end) = trimmed(item, i, state.preview);
        let bar = vp.bar(i, start, end);
        let alpha = if item.enabled { 1.0 } else { DISABLED_ALPHA };
        if item.cuts.is_empty() {
            push(&mut out, vp, (i, item.thumb.as_ref()), bar, alpha);
            continue;
        }
        for cut in item.cuts.iter().filter(|c| !c.gap) {
            let r = cut_rect(vp, bar, cut);
            if r.right() >= area.x && r.x <= area.right() {
                push(&mut out, vp, (i, cut.thumb.as_ref()), r, alpha);
            }
        }
    }
    out
}

fn push(
    out: &mut Vec<ThumbQuad>,
    vp: &Viewport,
    (row, key): (usize, Option<&Arc<str>>),
    span: Rect,
    alpha: f32,
) {
    let Some((key, b)) = key.zip(thumb_box(vp, row, span)) else {
        return;
    };
    let area = vp.layout.row_area;
    let (top, bottom) = (b.y.max(area.y), b.bottom().min(area.bottom()));
    if bottom <= top {
        return;
    }
    out.push(ThumbQuad {
        key: key.clone(),
        rect: Rect::new(b.x, top, b.w, bottom - top),
        uv: Rect::new(0.0, (top - b.y) / b.h, 1.0, (bottom - top) / b.h),
        alpha,
    });
}

#[cfg(test)]
mod tests;
