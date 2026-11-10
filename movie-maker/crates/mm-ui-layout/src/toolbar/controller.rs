//! ツールバーのポインター操作。ボタンは押して、同じボタンの上で離したときに実行する（再生の操作のバーと同じく、
//! 押したままボタンの外へずらして離せば取りやめられる）。スライダーは押したときとドラッグ中に倍率を返す。

use super::{cells, part_in, zoom_px, ToolbarState};
use crate::controller::PointerEvent;
use crate::geom::Rect;

/// ツールバーの押せる部品。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarPart {
    /// コマンドの表の ID を持つボタン。
    Button(&'static str),
    /// ズームのスライダー。
    Slider,
}

/// ツールバーの操作の結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolbarAction {
    /// ボタンのコマンドを実行する（コマンドの表の ID）。
    Command(&'static str),
    /// スライダーが指す、秒あたりの論理ピクセル。ドラッグ中は動くたびに返るので、受ける側が描画の 1 周に 1 回だけ使う。
    Zoom(f64),
}

/// ツールバーのポインター操作の状態。
#[derive(Debug, Default)]
pub struct ToolbarController {
    /// 押したまま離していないボタン。
    pressed: Option<&'static str>,
    /// スライダーをドラッグしている。
    dragging: bool,
    /// ポインターの下の部品。離したときの位置にも使う（離したことの入力は位置を持たない）。
    hover: Option<ToolbarPart>,
}

impl ToolbarController {
    /// ポインターが乗っている部品。
    pub fn hover(&self) -> Option<ToolbarPart> {
        self.hover
    }

    /// スライダーをドラッグしている。
    pub(crate) fn dragging(&self) -> bool {
        self.dragging
    }

    /// 押したまま、ポインターがその上にあるボタン（押した見た目にし、離せば実行する）。
    pub(crate) fn pressed(&self) -> Option<&'static str> {
        self.pressed
            .filter(|id| self.hover == Some(ToolbarPart::Button(id)))
    }

    /// ツールバーの矩形 `bar` と同じ座標の入力を受ける。
    pub fn handle(
        &mut self,
        ev: PointerEvent,
        bar: Rect,
        state: &ToolbarState,
    ) -> Vec<ToolbarAction> {
        match ev {
            PointerEvent::Down { x, y, .. } => {
                self.hover = part_in(bar, x, y);
                self.down(x, bar, state)
            }
            PointerEvent::Move { x, y } => {
                self.hover = part_in(bar, x, y);
                self.drag_to(x, bar, state).into_iter().collect()
            }
            PointerEvent::Up => {
                self.dragging = false;
                self.release(state).into_iter().collect()
            }
            PointerEvent::Cancel => {
                self.dragging = false;
                self.pressed = None;
                Vec::new()
            }
            PointerEvent::Leave => {
                self.hover = None;
                Vec::new()
            }
            // ホイール、キーとメニューのコマンド、スナップのキーは Timeline だけに効く。
            PointerEvent::Wheel { .. } | PointerEvent::Command(_) | PointerEvent::SnapKeyUp => {
                Vec::new()
            }
        }
    }

    fn down(&mut self, x: f32, bar: Rect, state: &ToolbarState) -> Vec<ToolbarAction> {
        match self.hover {
            Some(ToolbarPart::Slider) => {
                self.dragging = true;
                self.drag_to(x, bar, state).into_iter().collect()
            }
            Some(ToolbarPart::Button(id)) if enabled(state, id) => {
                self.pressed = Some(id);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// 離したとき、押したボタンの上にいて、まだ押せればそのコマンドを返す。
    fn release(&mut self, state: &ToolbarState) -> Option<ToolbarAction> {
        let armed = self.pressed();
        self.pressed = None;
        let id = armed?;
        enabled(state, id).then_some(ToolbarAction::Command(id))
    }

    /// ドラッグ中のスライダーの `x` が指す倍率。
    fn drag_to(&self, x: f32, bar: Rect, state: &ToolbarState) -> Option<ToolbarAction> {
        if !self.dragging {
            return None;
        }
        let slider = cells(bar).slider?;
        Some(ToolbarAction::Zoom(zoom_px(
            slider.fraction_at(x),
            state.floor,
        )))
    }
}

fn enabled(state: &ToolbarState, id: &str) -> bool {
    !state.disabled.contains(&id)
}
