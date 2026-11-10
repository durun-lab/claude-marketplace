//! ツールバーの矩形。背景と下端の線、グループの間の縦線、ボタンの背景、矩形で描くアイコン、ズームのスライダー
//! （溝、下限からつまみまでの塗り、つまみ）。字のアイコンとボタンの名前は文字で描く（[`crate::toolbar_labels`]）。

use super::{Canvas, Depth, PaintInput};
use crate::geom::Rect;
use crate::palette::Palette;
use crate::toolbar::{
    cells, icon_quads, zoom_fraction, ButtonLook, Slider, ToolbarController, ToolbarPart,
    ToolbarState, THUMB_W,
};

/// 下端の線と、グループの間の縦線の濃さ。
const EDGE_ALPHA: f32 = 0.5;
const SEPARATOR_ALPHA: f32 = 0.5;
/// スライダーの溝の高さと濃さ、塗りの濃さ。
const TRACK_H: f32 = 4.0;
const TRACK_ALPHA: f32 = 0.35;
const FILL_ALPHA: f32 = 0.7;
/// つまみの高さ。
const THUMB_H: f32 = 16.0;

pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let (Some(view), bar) = (s.toolbar, s.vp.layout.toolbar) else {
        return;
    };
    if bar.h <= 0.0 || bar.w <= 0.0 {
        return;
    }
    let p = &s.state.palette;
    let depth = Depth::Surround;
    c.push(depth, bar, p.panel);
    c.push(
        depth,
        Rect::new(bar.x, bar.bottom() - 1.0, bar.w, 1.0),
        p.tick.with_alpha(EDGE_ALPHA),
    );
    let layout = cells(bar);
    for sep in &layout.separators {
        c.push(depth, *sep, p.tick.with_alpha(SEPARATOR_ALPHA));
    }
    for cell in &layout.buttons {
        let look = ButtonLook::of(cell, view.state, view.controller);
        if let Some(back) = look.back(p) {
            c.push(depth, cell.rect, back);
        }
        let at = look.place(cell.icon_box());
        let center = (at.x + at.w / 2.0, at.y + at.h / 2.0);
        for q in icon_quads(cell.spec.icon, center, look.ink(p)) {
            c.push_quad(depth, q);
        }
    }
    if let Some(slider) = layout.slider {
        paint_slider(c, slider, (view.state, view.controller), p);
    }
}

/// スライダー。溝の上に、下限からつまみまでを accent で塗り、つまみを置く（乗っているか掴んでいる間は明るく）。
fn paint_slider(
    c: &mut Canvas,
    slider: Slider,
    (state, ctl): (&ToolbarState, &ToolbarController),
    p: &Palette,
) {
    let depth = Depth::Surround;
    let x = slider.thumb_x(zoom_fraction(state.px_per_sec, state.floor));
    let y = slider.track.y + (slider.track.h - TRACK_H) / 2.0;
    let groove = Rect::new(slider.track.x, y, slider.track.w, TRACK_H);
    c.push(depth, groove, p.tick.with_alpha(TRACK_ALPHA));
    let fill = Rect::new(groove.x, y, (x - groove.x).max(0.0), TRACK_H);
    c.push(depth, fill, p.accent.with_alpha(FILL_ALPHA));
    let active = ctl.dragging() || ctl.hover() == Some(ToolbarPart::Slider);
    let thumb = Rect::new(
        x - THUMB_W / 2.0,
        slider.track.y + (slider.track.h - THUMB_H) / 2.0,
        THUMB_W,
        THUMB_H,
    );
    c.push(depth, thumb, if active { p.selection } else { p.label });
}
