//! 再生の操作のバーの描画。矩形（背景、ジョグバー、ボタンの絵柄）と文字（時刻）。座標はサーフェスの論理ピクセル。

use std::sync::Arc;

use super::{
    bar_of, cells, icons, Cells, Target, TransportController, TransportState, Viewer, ViewerState,
};
use crate::geom::{Color, Quad, Rect};
use crate::header::timecode;
use crate::palette::Palette;
use crate::screen::SurfaceLayout;
use crate::text::{place, Align, Label, TextMeasure, TextStyle, UiFont};

/// バーの上端の線の濃さ。
const EDGE_ALPHA: f32 = 0.5;
/// ジョグバーの線の高さ。
const TRACK_H: f32 = 4.0;
/// ジョグバーの線の濃さと、進んだ分の濃さ。
const TRACK_ALPHA: f32 = 0.35;
const FILL_ALPHA: f32 = 0.7;
/// Source のジョグバーの、イン点とアウト点の間の濃さ。
const RANGE_ALPHA: f32 = 0.5;
/// 再生位置のつまみの幅。
const THUMB_W: f32 = 2.0;
/// ポインターが乗ったボタンの下地の濃さ。
const HOVER_ALPHA: f32 = 0.12;
/// 押せないボタンの絵柄の濃さ。
const DISABLED_ALPHA: f32 = 0.3;
/// 時刻の欄の文字と矩形の端の余白。
const TEXT_PAD: f32 = super::SIDE_PAD;
/// 時刻が定まらないときの表示（イン点とアウト点が無い）。
const NO_TIME: &str = "--:--:--:--";

/// 表示する 2 つのバーの矩形（描く順）。
fn bars<'a>(
    layout: &SurfaceLayout,
    state: &'a TransportState,
) -> impl Iterator<Item = (Viewer, Rect, &'a ViewerState)> {
    let layout = *layout;
    [Viewer::Program, Viewer::Source]
        .into_iter()
        .filter_map(move |v| Some((v, bar_of(&layout, v)?, state.of(v)?)))
}

/// 文字の下に描く矩形（バーの背景、ジョグバー、ボタンの下地と絵柄）。
pub fn transport_quads(
    layout: &SurfaceLayout,
    state: &TransportState,
    controller: &TransportController,
    palette: &Palette,
) -> Vec<Quad> {
    let mut out = Vec::new();
    for (viewer, bar, s) in bars(layout, state) {
        let c = cells(bar, viewer);
        out.push(Quad::new(bar, palette.panel));
        let edge = Rect::new(bar.x, bar.y, bar.w, 1.0);
        out.push(Quad::new(edge, palette.tick.with_alpha(EDGE_ALPHA)));
        jog(&mut out, &c, s, palette);
        let hover = controller.hover().filter(|(v, _)| *v == viewer);
        buttons(&mut out, &c, s, hover.map(|(_, t)| t), palette);
    }
    out
}

/// ジョグバー。Program は再生位置までを塗り、Source はイン点とアウト点の間を塗る。つまみは再生位置。
fn jog(out: &mut Vec<Quad>, c: &Cells, s: &ViewerState, p: &Palette) {
    let track = c.track();
    let y = track.y + (track.h - TRACK_H) / 2.0;
    out.push(Quad::new(
        Rect::new(track.x, y, track.w, TRACK_H),
        p.tick.with_alpha(TRACK_ALPHA),
    ));
    if s.duration <= 0.0 {
        return;
    }
    let x_of = |t: f64| track.x + track.w * (t / s.duration).clamp(0.0, 1.0) as f32;
    let x = x_of(s.t);
    let (from, to, alpha) = if s.mark_in.is_some() || s.mark_out.is_some() {
        let (i, o) = (s.mark_in.unwrap_or(0.0), s.mark_out.unwrap_or(s.duration));
        (x_of(i), x_of(o), RANGE_ALPHA)
    } else {
        (track.x, x, FILL_ALPHA)
    };
    if to > from {
        out.push(Quad::new(
            Rect::new(from, y, to - from, TRACK_H),
            p.solo.with_alpha(alpha),
        ));
    }
    let thumb = Rect::new(x - THUMB_W / 2.0, track.y + 1.0, THUMB_W, track.h - 2.0);
    out.push(Quad::new(thumb, p.playhead));
}

fn buttons(out: &mut Vec<Quad>, c: &Cells, s: &ViewerState, hover: Option<Target>, p: &Palette) {
    for slot in &c.buttons {
        let enabled = !s.disabled.contains(&slot.spec.command);
        let hovered = enabled && hover == Some(Target::Button(slot.spec.command));
        if hovered {
            out.push(Quad::new(slot.rect, p.selection.with_alpha(HOVER_ALPHA)));
        }
        let color = icon_color(p, enabled, hovered);
        let center = (
            slot.rect.x + slot.rect.w / 2.0,
            slot.rect.y + slot.rect.h / 2.0,
        );
        out.extend(icons::quads(slot.spec.icon, s.playing, center, color));
    }
}

/// 絵柄の色。ポインターが乗れば明るく、押せなければ薄く。
fn icon_color(p: &Palette, enabled: bool, hovered: bool) -> Color {
    match (enabled, hovered) {
        (false, _) => p.label.with_alpha(DISABLED_ALPHA),
        (true, true) => p.selection,
        (true, false) => p.label,
    }
}

/// バーの文字。左に再生位置、右に Program は全体の長さ、Source はイン点とアウト点。
pub fn transport_labels(
    layout: &SurfaceLayout,
    state: &TransportState,
    palette: &Palette,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    let mut out = Vec::new();
    let base = |color| TextStyle {
        size: font.base,
        color,
    };
    let small = |color| TextStyle {
        size: font.small,
        color,
    };
    for (viewer, bar, s) in bars(layout, state) {
        let c = cells(bar, viewer);
        let now: Arc<str> = timecode(s.t, s.fps).into();
        let mut put = |text: &Arc<str>, cell: Rect, align: Align, style: TextStyle| {
            if let Some(mut label) = place(text, cell, TEXT_PAD, align, style, measure) {
                label.clip = cell;
                out.push(label);
            }
        };
        put(&now, c.time, Align::Left, base(palette.selection));
        match viewer {
            Viewer::Program => {
                let total: Arc<str> = timecode(s.duration, s.fps).into();
                put(&total, c.side, Align::Right, base(palette.label));
            }
            Viewer::Source => {
                let (top, bottom) = halves(c.side);
                for (name, at, cell) in [("In", s.mark_in, top), ("Out", s.mark_out, bottom)] {
                    let time = at.map_or_else(|| NO_TIME.to_owned(), |t| timecode(t, s.fps));
                    let text: Arc<str> = format!("{name} {time}").into();
                    put(&text, cell, Align::Right, small(palette.label));
                }
            }
        }
    }
    out
}

/// 矩形を上下の半分に割る。
fn halves(r: Rect) -> (Rect, Rect) {
    let h = r.h / 2.0;
    (Rect::new(r.x, r.y, r.w, h), Rect::new(r.x, r.y + h, r.w, h))
}
