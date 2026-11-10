//! 再生の操作のバーの描画。矩形（背景、ジョグバー、ボタンの背景と枠とアイコン）と文字（タイムコードとボタンの名前）。
//! 座標はサーフェスの論理ピクセル。

use std::sync::Arc;

use super::{
    cells, icons, transport_rect, ButtonCell, Cells, Icon, TransportController, TransportPart,
    TransportState, Viewer, ViewerState,
};
use crate::geom::{Color, Quad, Rect};
use crate::header::timecode;
use crate::palette::{text_on, Palette};
use crate::screen::SurfaceLayout;
use crate::text::{place, place_timecode, Align, Label, TextMeasure, TextStyle, UiFont};

/// バーの上端の線の濃さ。
const EDGE_ALPHA: f32 = 0.5;
/// ジョグバーの溝の高さ。
const TRACK_H: f32 = 4.0;
/// ジョグバーの溝の濃さと、再生位置までの塗りの濃さ。
const TRACK_ALPHA: f32 = 0.35;
const FILL_ALPHA: f32 = 0.7;
/// Source のジョグバーの、イン点とアウト点の間の濃さ。
const RANGE_ALPHA: f32 = 0.5;
/// ジョグバーの再生位置の線の幅。
const PLAYHEAD_W: f32 = 2.0;
/// ボタンの背景の濃さ。ポインターが乗ったとき、押している間。
const HOVER_ALPHA: f32 = 0.12;
const PRESSED_ALPHA: f32 = 0.25;
/// 押せないボタンのアイコンと名前と枠の濃さ。
const DISABLED_ALPHA: f32 = 0.3;
/// 名前のボタンの枠の太さ。
const BORDER: f32 = 1.0;
/// 押している間、アイコンと名前を右下へずらす量（押し込んだ見た目）。
const PRESS_SHIFT: f32 = 1.0;
/// 未設定のタイムコード（イン点やアウト点が無いときに出す）。
const NO_TIMECODE: &str = "--:--:--:--";

/// 出している 2 つの再生の操作のバーの矩形（描く順）。
fn bars<'a>(
    layout: &SurfaceLayout,
    state: &'a TransportState,
) -> impl Iterator<Item = (Viewer, Rect, &'a ViewerState)> {
    let layout = *layout;
    [Viewer::Program, Viewer::Source]
        .into_iter()
        .filter_map(move |v| Some((v, transport_rect(&layout, v)?, state.of(v)?)))
}

/// ボタンの見た目を決める状態。
#[derive(Debug, Clone, Copy)]
struct ButtonLook {
    enabled: bool,
    hovered: bool,
    pressed: bool,
    /// 入（再生中の Play Forward）。
    lit: bool,
}

impl ButtonLook {
    fn of(cell: &ButtonCell, viewer: Viewer, s: &ViewerState, ctl: &TransportController) -> Self {
        let id = cell.spec.command;
        let enabled = s.enabled(id);
        Self {
            enabled,
            hovered: enabled && ctl.hover() == Some((viewer, TransportPart::Button(id))),
            pressed: enabled && ctl.pressed() == Some((viewer, id)),
            lit: enabled && cell.spec.icon == Icon::PlayForward && s.playing,
        }
    }

    /// 背景。入は Mute と Solo のボタンと同じく塗る。
    fn back(self, p: &Palette) -> Option<Color> {
        if self.pressed {
            Some(p.selection.with_alpha(PRESSED_ALPHA))
        } else if self.lit {
            Some(p.accent)
        } else if self.hovered {
            Some(p.selection.with_alpha(HOVER_ALPHA))
        } else {
            None
        }
    }

    /// アイコンと名前の色。
    fn ink(self, p: &Palette) -> Color {
        if !self.enabled {
            p.label.with_alpha(DISABLED_ALPHA)
        } else if self.pressed || (self.hovered && !self.lit) {
            p.selection
        } else if self.lit {
            text_on(p.accent)
        } else {
            p.label
        }
    }

    /// アイコンと名前の矩形。押している間は右下へずらす。
    fn place(self, r: Rect) -> Rect {
        let d = if self.pressed { PRESS_SHIFT } else { 0.0 };
        r.translated(d, d)
    }
}

/// 文字の下に描く矩形（バーの背景、ジョグバー、ボタンの背景と枠とアイコン）。
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
        for cell in &c.buttons {
            button(
                &mut out,
                cell,
                ButtonLook::of(cell, viewer, s, controller),
                palette,
            );
        }
    }
    out
}

/// ジョグバー。Program は再生位置までを塗り、Source はイン点とアウト点の間を塗る。再生位置に線を引く。
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
            p.accent.with_alpha(alpha),
        ));
    }
    let line = Rect::new(
        x - PLAYHEAD_W / 2.0,
        track.y + 1.0,
        PLAYHEAD_W,
        track.h - 2.0,
    );
    out.push(Quad::new(line, p.playhead));
}

/// ボタン 1 つ。名前のボタンは枠を描き、名前は文字で描く（[`transport_labels`]）。アイコンのボタンはアイコンを描く。
fn button(out: &mut Vec<Quad>, cell: &ButtonCell, look: ButtonLook, p: &Palette) {
    let r = cell.rect;
    if let Some(back) = look.back(p) {
        out.push(Quad::new(r, back));
    }
    if cell.named {
        let color = if look.enabled {
            p.tick
        } else {
            p.tick.with_alpha(DISABLED_ALPHA)
        };
        frame(out, r, color);
        return;
    }
    let at = look.place(r);
    let center = (at.x + at.w / 2.0, at.y + at.h / 2.0);
    out.extend(icons::quads(cell.spec.icon, center, look.ink(p)));
}

/// 矩形の内側の 1 論理ピクセルの枠。
fn frame(out: &mut Vec<Quad>, r: Rect, color: Color) {
    for edge in [
        Rect::new(r.x, r.y, r.w, BORDER),
        Rect::new(r.x, r.bottom() - BORDER, r.w, BORDER),
        Rect::new(r.x, r.y, BORDER, r.h),
        Rect::new(r.right() - BORDER, r.y, BORDER, r.h),
    ] {
        out.push(Quad::new(edge, color));
    }
}

/// バーの文字。タイムコードの行の右に Program は全体の長さ、Source はイン点とアウト点。名前のボタンの名前。
/// 左の再生位置は再生中に毎フレーム変わるので、ここでは置かない（[`transport_timecodes`]）。
pub fn transport_labels(
    layout: &SurfaceLayout,
    state: &TransportState,
    controller: &TransportController,
    palette: &Palette,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    let mut out = Vec::new();
    let mut put = |text: &str, area: Rect, align: Align, style: TextStyle| {
        if let Some(mut label) = place(&Arc::from(text), area, 0.0, align, style, measure) {
            label.clip = area;
            out.push(label);
        }
    };
    let style = |size, color| TextStyle { size, color };
    for (viewer, bar, s) in bars(layout, state) {
        let c = cells(bar, viewer);
        let (range, size) = match viewer {
            Viewer::Program => (timecode(s.duration, s.fps), font.base),
            Viewer::Source => (in_out_text(s), font.small),
        };
        put(&range, c.range, Align::Right, style(size, palette.label));
        for cell in c.buttons.iter().filter(|b| b.named) {
            let look = ButtonLook::of(cell, viewer, s, controller);
            let name = cell.spec.name.unwrap_or_default();
            put(
                name,
                look.place(cell.rect),
                Align::Center,
                style(font.base, look.ink(palette)),
            );
        }
    }
    out
}

/// バーのタイムコードの行の左の、再生位置のタイムコード。再生中は毎フレーム変わるので [`transport_labels`] と分けて
/// 返し、描く所がほかの文字と頂点を分ける（`mm-ui-wgpu` の `TextLayer`）。
pub fn transport_timecodes(
    layout: &SurfaceLayout,
    state: &TransportState,
    palette: &Palette,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    let style = TextStyle {
        size: font.base,
        color: palette.selection,
    };
    bars(layout, state)
        .filter_map(|(viewer, bar, s)| {
            let area = cells(bar, viewer).position;
            let text = Arc::from(timecode(s.t, s.fps));
            let mut label = place_timecode(&text, area, 0.0, Align::Left, style, measure)?;
            label.clip = area;
            Some(label)
        })
        .collect()
}

/// Source のタイムコードの行の右の文字（`In 00:00:01:00  Out --:--:--:--`）。
fn in_out_text(s: &ViewerState) -> String {
    let time = |at: Option<f64>| at.map_or_else(|| NO_TIMECODE.to_owned(), |t| timecode(t, s.fps));
    format!("In {}  Out {}", time(s.mark_in), time(s.mark_out))
}
