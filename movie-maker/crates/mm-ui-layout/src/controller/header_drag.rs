//! 見出しの縦のドラッグ（重なりの順を変える）。After Effects と同じく、ドラッグ中は入る位置に線を出し、
//! 離したときに 1 回の操作にする。Esc で取りやめる（ドラッグの取り消しと同じ経路）。
//!
//! 位置は行の境目の番号 `slot`（0 が先頭の行の上、行の数が最後の行の下）で表す。入る位置は見えている行の境目に限り、
//! 線の出ない所には入れない。見えていない行の間へは、ドラッグしたままホイールで縦にスクロールして動かす。

use super::{Action, Drag, TimelineController};

/// 押した所からこれだけ縦に動くまでは、ただのクリックとして選ぶだけにする（論理ピクセル）。
const DRAG_START_PX: f32 = 4.0;

/// 見出しの縦のドラッグ。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct HeaderDrag {
    /// つかんだ行。
    index: usize,
    /// 押した縦の位置（Timeline の座標）。
    grab_y: f32,
    /// 入る行の境目。動き始めてから。
    slot: Option<usize>,
}

impl HeaderDrag {
    /// 離した。別の位置に入るなら `MoveLayer`。自分の上か下の境目なら並びが変わらないので何もしない。
    pub(super) fn release(self) -> Vec<Action> {
        match self.slot {
            Some(slot) if slot != self.index && slot != self.index + 1 => {
                vec![Action::MoveLayer {
                    from: self.index,
                    slot,
                }]
            }
            _ => Vec::new(),
        }
    }
}

impl TimelineController {
    /// 見出しを押した。レイヤーを選び、縦のドラッグの準備をする。
    pub(super) fn press_header(&mut self, index: usize, y: f32) {
        self.selected = Some(index);
        let drag = HeaderDrag {
            index,
            grab_y: y,
            slot: None,
        };
        self.drag = Drag::Header(drag);
    }

    /// 見出しのドラッグ中にポインターが縦の位置 `y` へ動いた。動き始めたら入る位置を置く。
    pub(super) fn drag_header(&mut self, mut drag: HeaderDrag, y: f32, len: usize) -> Vec<Action> {
        if drag.slot.is_some() || (y - drag.grab_y).abs() >= DRAG_START_PX {
            drag.slot = Some(self.slot_at(y, len));
            self.drag = Drag::Header(drag);
        }
        Vec::new()
    }

    /// ホイールで表示が動いた。見出しのドラッグが動き始めていれば、同じポインターの位置で入る位置を置き直す。
    pub(super) fn follow_scroll(&mut self, len: usize) {
        if let Drag::Header(drag) = self.drag {
            if drag.slot.is_some() {
                self.drag_header(drag, self.pointer.1, len);
            }
        }
    }

    /// 入る位置の線を出す行の境目。見出しのドラッグが動き始めてから離すまで。
    pub(super) fn header_drop(&self) -> Option<usize> {
        match self.drag {
            Drag::Header(drag) => drag.slot,
            _ => None,
        }
    }

    /// 縦の位置 `y`（Timeline の座標）にいちばん近い行の境目。見えている行の領域の外は、その端に寄せる。
    fn slot_at(&self, y: f32, len: usize) -> usize {
        let area = self.layout.headers;
        let content = y.clamp(area.y, area.bottom()) - area.y + self.scroll_y;
        match self.rows.at(content) {
            Some(i) if i < len => {
                let middle = (self.rows.top(i) + self.rows.bottom(i)) / 2.0;
                i + usize::from(content > middle)
            }
            _ => len,
        }
    }
}

#[cfg(test)]
mod tests;
