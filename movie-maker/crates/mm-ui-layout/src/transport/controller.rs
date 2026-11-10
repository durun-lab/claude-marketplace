//! 再生の操作のバーのポインター操作。ボタンは押して、同じボタンの上で離したときに実行する（Windows のボタンと同じく、
//! 押したままボタンの外へずらして離せば取りやめられる）。ジョグバーは押したときとドラッグ中にシークする。

use super::{
    cells, jog_time, part_at, transport_rect, TransportAction, TransportPart, TransportState,
    Viewer,
};
use crate::controller::PointerEvent;
use crate::screen::SurfaceLayout;

/// バーのポインター操作の状態。ジョグバーのドラッグと押したボタンは、押したバーに結び付く。
#[derive(Debug, Default)]
pub struct TransportController {
    /// ジョグバーをドラッグしているバー。
    drag: Option<Viewer>,
    /// 押したまま離していないボタン。
    pressed: Option<(Viewer, &'static str)>,
    /// ポインターの下のバーの部品。離したときの位置にも使う（離したことの入力は位置を持たない）。
    hover: Option<(Viewer, TransportPart)>,
}

impl TransportController {
    /// ポインターが乗っているバーの部品。
    pub fn hover(&self) -> Option<(Viewer, TransportPart)> {
        self.hover
    }

    /// 押したまま、ポインターがその上にあるボタン（押した見た目にし、離せば実行する）。
    pub(crate) fn pressed(&self) -> Option<(Viewer, &'static str)> {
        self.pressed
            .filter(|(v, id)| self.hover == Some((*v, TransportPart::Button(id))))
    }

    /// 論理ピクセルのポインターの入力を受ける。
    pub fn handle(
        &mut self,
        ev: PointerEvent,
        layout: &SurfaceLayout,
        state: &TransportState,
    ) -> Vec<TransportAction> {
        match ev {
            PointerEvent::Down { x, y, .. } => {
                self.hover = part_at(layout, x, y);
                self.down(x, layout, state)
            }
            PointerEvent::Move { x, y } => {
                self.hover = part_at(layout, x, y);
                self.seek(x, layout, state).into_iter().collect()
            }
            PointerEvent::Up => {
                self.drag = None;
                self.release(state).into_iter().collect()
            }
            PointerEvent::Cancel => {
                self.drag = None;
                self.pressed = None;
                Vec::new()
            }
            PointerEvent::Leave => {
                self.hover = None;
                Vec::new()
            }
            // Timeline のコマンド、スナップのキー、右クリックのメニューは Timeline だけに効く。
            PointerEvent::Wheel { .. }
            | PointerEvent::Command(_)
            | PointerEvent::SnapKeyUp
            | PointerEvent::ContextMenu { .. }
            | PointerEvent::ContextMenuKey => Vec::new(),
        }
    }

    fn down(
        &mut self,
        x: f32,
        layout: &SurfaceLayout,
        state: &TransportState,
    ) -> Vec<TransportAction> {
        match self.hover {
            Some((viewer, TransportPart::Jog)) => {
                self.drag = Some(viewer);
                self.seek(x, layout, state).into_iter().collect()
            }
            Some((viewer, TransportPart::Button(id))) if enabled(state, viewer, id) => {
                self.pressed = Some((viewer, id));
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// 離したとき、押したボタンの上にいて、まだ押せればそのコマンドを返す。
    fn release(&mut self, state: &TransportState) -> Option<TransportAction> {
        let armed = self.pressed();
        self.pressed = None;
        let (viewer, id) = armed?;
        enabled(state, viewer, id).then_some(TransportAction::Command(id))
    }

    /// ドラッグ中のジョグバーの `x` へのシーク。
    fn seek(
        &self,
        x: f32,
        layout: &SurfaceLayout,
        state: &TransportState,
    ) -> Option<TransportAction> {
        let viewer = self.drag?;
        let bar = transport_rect(layout, viewer)?;
        let duration = state.of(viewer)?.duration;
        if duration <= 0.0 {
            return None;
        }
        let t = jog_time(cells(bar, viewer).track(), duration, x);
        Some(TransportAction::Seek { viewer, t })
    }
}

fn enabled(state: &TransportState, viewer: Viewer, id: &str) -> bool {
    state.of(viewer).is_some_and(|s| s.enabled(id))
}

#[cfg(test)]
mod tests;
