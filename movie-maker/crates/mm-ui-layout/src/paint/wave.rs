//! 帯の波形。音のあるレイヤーとカットの帯に、下端（Medium）か下半分（Large）の領域へ、縦の矩形の並びで描く。
//! 矩形は倍率と見えている範囲が変わったときだけ作り直し、そうでなければ前の矩形を使い回す（`Waves::quads`）。

use super::{bar_color, rows::trimmed, Canvas, Depth, PaintInput};
use crate::geom::{Color, Quad, Rect};
use crate::palette;
use crate::timeline::{cut_span, Item};
use crate::wave::{amplitudes, region, Grid, Peaks, WaveKey, WaveSource, Waves};

/// 列の幅（論理ピクセル）。1 px の細い矩形を数千並べず、見た目の粗さと矩形の数の釣り合いを取る。
const COLUMN_PX: f32 = 2.0;
/// 波形の不透明度。帯の色の上に薄く重ね、下端のマーカーと名前を邪魔しない。
const WAVE_ALPHA: f32 = 0.6;
/// 無効なレイヤーの波形に掛ける不透明度。
const DISABLED_SCALE: f32 = 0.4;

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let Some(waves) = s.waves else {
        return;
    };
    let key = WaveKey {
        time: s.vp.time,
        scroll_y: s.vp.scroll_y,
        area: s.vp.layout.row_area,
        preview: s.state.preview,
    };
    for quad in waves.quads(key, || build(s, waves)).iter() {
        c.push_quad(Depth::Wave, *quad);
    }
}

/// 見えている行の波形の矩形。ピークがまだ無い素材は描かない。
fn build(s: &PaintInput, waves: &Waves) -> Vec<Quad> {
    let mut out = Vec::new();
    for i in s.vp.visible_rows() {
        let Some(item) = s.model.items.get(i) else {
            break;
        };
        let (start, end) = trimmed(item, i, s.state.preview);
        let bar = s.vp.bar(i, start, end);
        let Some(area) = region(s.vp.rows.height(i), bar) else {
            continue;
        };
        let mut draw = |src: &WaveSource, area: Rect, color: Color| {
            if let Some(peaks) = waves.get(&src.content_hash) {
                columns(s, &mut out, (peaks, src), area, color);
            }
        };
        if item.cuts.is_empty() {
            if let Some(src) = &item.wave {
                draw(src, area, wave_color(item, bar_color(item)));
            }
            continue;
        }
        let color = wave_color(item, palette::item_color(item.kind));
        let left = s.vp.layout.row_area.x;
        for cut in item.cuts.iter().filter(|c| !c.gap) {
            let Some(src) = &cut.wave else { continue };
            // カットはレイヤーの帯の内側だけが映るので、波形もその中だけ。
            let (cx, cw) = cut_span(&s.vp.time, cut);
            let x = (left + cx).max(area.x);
            let w = (left + cx + cw).min(area.right()) - x;
            if w > 0.0 {
                draw(src, Rect::new(x, area.y, w, area.h), color);
            }
        }
    }
    out
}

/// 領域 `area` の行の領域に見える横の範囲へ、`src` の波形を描く矩形を足す。同じ高さの列は 1 つの矩形にまとめる。
fn columns(
    s: &PaintInput,
    out: &mut Vec<Quad>,
    (peaks, src): (&Peaks, &WaveSource),
    area: Rect,
    color: Color,
) {
    let clip = s.vp.layout.row_area;
    let (x0, x1) = (area.x.max(clip.x), area.right().min(clip.right()));
    let (y0, y1) = (area.y.max(clip.y), area.bottom().min(clip.bottom()));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let grid = Grid::covering(&s.vp.time, clip.x, (x0, x1), COLUMN_PX);
    let heights: Vec<f32> = amplitudes(peaks, src, &grid)
        .iter()
        .map(|a| column_height(*a, area.h))
        .collect();
    let center = area.y + area.h / 2.0;
    let mut i = 0;
    while i < heights.len() {
        let h = heights[i];
        let run = heights[i..].iter().take_while(|v| **v == h).count();
        // 列の境は時刻に固定してあり帯の端とは揃わないので、端の列は帯の中だけを描く。
        let (left, right) = (grid.x(i).max(x0), grid.x(i + run).min(x1));
        let (top, bottom) = ((center - h / 2.0).max(y0), (center + h / 2.0).min(y1));
        if h > 0.0 && right > left && bottom > top {
            out.push(Quad::new(
                Rect::new(left, top, right - left, bottom - top),
                color,
            ));
        }
        i += run;
    }
}

/// 振幅 `amp`（0..=1）の列の高さ。論理ピクセルに丸め、音のある所は最低 1 px（無音は 0）。
fn column_height(amp: f32, full: f32) -> f32 {
    let h = (amp * full).round();
    if amp > 0.0 {
        h.max(1.0)
    } else {
        h
    }
}

/// 波形の色。帯の色 `base` の上で読める明るさ（名前の字と同じ選び方）を薄く重ねる。無効なレイヤーはさらに薄くする。
fn wave_color(item: &Item, base: Color) -> Color {
    let alpha = if item.enabled {
        WAVE_ALPHA
    } else {
        WAVE_ALPHA * DISABLED_SCALE
    };
    palette::text_on(base).with_alpha(alpha)
}

#[cfg(test)]
mod tests;
