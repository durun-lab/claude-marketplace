//! ボタンの見た目を決める状態と色。矩形（`paint/toolbar.rs`）と文字（[`super::toolbar_labels`]）が同じ値を見る。
//! 再生の操作のバーのボタンと同じく、ポインターが乗れば明るく、押している間は押した見た目（背景を濃くして中身を右下へずらす）、
//! 押せなければ薄く、入（Snapping が入、今のツール）は accent で塗る。

use super::{ButtonCell, ToolbarController, ToolbarPart, ToolbarState};
use crate::geom::{Color, Rect};
use crate::palette::{text_on, Palette};

/// ボタンの背景の濃さ。ポインターが乗ったとき、押している間。
const HOVER_ALPHA: f32 = 0.12;
const PRESSED_ALPHA: f32 = 0.25;
/// 押せないボタンのアイコンと名前の濃さ。
const DISABLED_ALPHA: f32 = 0.3;
/// 押している間、アイコンと名前を右下へずらす量（押し込んだ見た目）。
const PRESS_SHIFT: f32 = 1.0;

/// ボタンの見た目を決める状態。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ButtonLook {
    enabled: bool,
    hovered: bool,
    pressed: bool,
    /// 入（Snapping が入、今のツール）。
    lit: bool,
}

impl ButtonLook {
    pub(crate) fn of(cell: &ButtonCell, state: &ToolbarState, ctl: &ToolbarController) -> Self {
        let spec = cell.spec;
        let enabled = state.enabled(spec);
        let hovered = ctl.hover() == Some(ToolbarPart::Button(spec.command));
        Self {
            enabled,
            hovered: enabled && hovered,
            pressed: enabled && ctl.pressed() == Some(spec.command),
            lit: enabled && state.lit(spec),
        }
    }

    /// 背景。
    pub(crate) fn back(self, p: &Palette) -> Option<Color> {
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
    pub(crate) fn ink(self, p: &Palette) -> Color {
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
    pub(crate) fn place(self, r: Rect) -> Rect {
        let d = if self.pressed { PRESS_SHIFT } else { 0.0 };
        r.translated(d, d)
    }

    #[cfg(test)]
    pub(crate) fn is_pressed(self) -> bool {
        self.pressed
    }

    #[cfg(test)]
    pub(crate) fn is_lit(self) -> bool {
        self.lit
    }

    #[cfg(test)]
    pub(crate) fn is_enabled(self) -> bool {
        self.enabled
    }
}
