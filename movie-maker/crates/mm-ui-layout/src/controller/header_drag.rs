//! 見出しの縦のドラッグ（重なりの順を変える）。After Effects と同じく、ドラッグ中は入る位置に線を出し、
//! 離したときに 1 回の操作にする。Esc で取りやめる（ドラッグの取り消しと同じ経路）。
//!
//! 位置は行の境目の番号 `slot`（0 が先頭の行の上、行の数が最後の行の下）で表す。

use super::{Action, Drag, TimelineController};

/// 押した所からこれだけ縦に動くまでは、ただのクリックとして選ぶだけにする（論理ピクセル）。
const DRAG_START_PX: f32 = 4.0;

impl TimelineController {
    /// 見出しを押した。レイヤーを選び、縦のドラッグの準備をする。
    pub(super) fn press_header(&mut self, index: usize, y: f32) {
        self.selected = Some(index);
        self.drag = Drag::Header { index, grab_y: y };
    }

    /// 見出しのドラッグ中。動き始めたら、入る位置を `drop_slot` に置く。
    pub(super) fn drag_header(&mut self, grab_y: f32, y: f32, len: usize) {
        if self.drop_slot.is_some() || (y - grab_y).abs() >= DRAG_START_PX {
            self.drop_slot = Some(self.slot_at(y, len));
        }
    }

    /// 見出しのドラッグを離した。動かして別の位置に入るなら `MoveLayer`。元の位置のままなら何もしない。
    pub(super) fn release_header(&mut self, index: usize, slot: Option<usize>) -> Vec<Action> {
        match slot {
            Some(slot) if !moves_nowhere(index, slot) => {
                vec![Action::MoveLayer { from: index, slot }]
            }
            _ => Vec::new(),
        }
    }

    /// 縦の位置 `y`（Timeline の座標）にいちばん近い行の境目。
    fn slot_at(&self, y: f32, len: usize) -> usize {
        let content = y - self.layout.headers.y + self.scroll_y;
        match self.rows.at(content) {
            Some(i) if i < len => {
                let middle = (self.rows.top(i) + self.rows.bottom(i)) / 2.0;
                if content > middle {
                    i + 1
                } else {
                    i
                }
            }
            _ if content <= 0.0 => 0,
            _ => len,
        }
    }
}

/// 行 `index` を境目 `slot` に入れても並びが変わらない（自分の上か下の境目）。
fn moves_nowhere(index: usize, slot: usize) -> bool {
    slot == index || slot == index + 1
}

#[cfg(test)]
mod tests;
