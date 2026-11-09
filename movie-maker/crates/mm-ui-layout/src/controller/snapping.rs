//! 端のドラッグのスナップ（入/切、ドラッグ中の反転、吸い付いた目標）と、ドラッグ中のツールチップ。
//!
//! ドラッグ中に入/切のキーを押すと、入/切そのものは変えずに、離すまで反転する（FCP の N と同じく押している間だけ）。
//! 吸い付いたことにするのは、端が目標に届いたときだけ（素材の端や 1 フレームの長さで止まれば、ガイド線を出さない）。

use super::{Action, Drag, TimelineController};
use crate::drag_tooltip::DragTooltip;
use crate::gesture::{edge_edit, EditKind};
use crate::snap::{quantize, snap, targets, Moving, SNAP_PX};
use crate::timeline::{Part, TimelineModel};

/// 端が目標に届いたとみなす差（秒）。量はスナップした時刻と端の差なので、止まらなければ浮動小数の誤差しか残らない。
const LANDED_EPS: f64 = 1e-9;

impl TimelineController {
    /// 今のドラッグで吸い付くか。入/切を、押している間の反転と合わせる。
    fn snaps(&self) -> bool {
        self.snapping != self.snap_held
    }

    /// ドラッグを終える（離した、取り消した、モデルが変わった）。スナップの表示と反転も捨てる。
    pub(super) fn end_drag(&mut self) {
        self.drag = Drag::None;
        self.snap = None;
        self.snap_held = false;
        self.last_edit = None;
    }

    /// 端のつまみを x へドラッグしたときの編集。スナップが効いていれば近い目標に寄せ、吸い付かなければいちばん近い
    /// フレームの先頭にそろえる。
    pub(super) fn edge_drag(
        &mut self,
        x: f32,
        model: &TimelineModel,
        playhead: f64,
    ) -> Option<(EditKind, f64)> {
        let Drag::Edge {
            item,
            cut,
            part,
            mods,
        } = self.drag
        else {
            return None;
        };
        let t = self.viewport().t_of(x);
        let moving = cut.map_or(Moving::Layer(item), |cut| Moving::Cut { item, cut });
        let target = if self.snaps() {
            let near = targets(model, moving, playhead);
            snap(t, near, self.view.px_per_sec, SNAP_PX).target
        } else {
            None
        };
        let time = target.map_or_else(|| quantize(t, model.fps), |s| s.time);
        let edit = edge_edit(model, item, cut, part, mods, time);
        let from = edge_time(model, item, cut, part);
        let landed = |s: f64| {
            edit.zip(from)
                .is_some_and(|((_, by), from)| (from + by - s).abs() < LANDED_EPS)
        };
        self.snap = target.filter(|s| landed(s.time));
        edit
    }

    /// スナップの入/切。ドラッグ中は入/切を変えず、入/切のキーを離すまで反転して、今の位置で吸い付き直す。
    pub(super) fn toggle_snapping(&mut self, model: &TimelineModel, playhead: f64) -> Vec<Action> {
        if self.drag == Drag::None {
            self.snapping = !self.snapping;
            return Vec::new();
        }
        self.snap_held = true;
        self.resnap(model, playhead)
    }

    /// ドラッグ中に押した入/切のキーを離した。反転を戻して、今の位置で吸い付き直す。
    pub(super) fn release_snap_key(&mut self, model: &TimelineModel, playhead: f64) -> Vec<Action> {
        if !std::mem::take(&mut self.snap_held) {
            return Vec::new();
        }
        self.resnap(model, playhead)
    }

    /// 端のドラッグを今のポインターの位置で置き直す。押しただけでまだ動かしていなければ、その位置を編集にしない。
    fn resnap(&mut self, model: &TimelineModel, playhead: f64) -> Vec<Action> {
        if !matches!(self.drag, Drag::Edge { .. }) || self.last_edit.is_none() {
            return Vec::new();
        }
        let edit = self.edge_drag(self.pointer.0, model, playhead);
        self.report(edit)
    }

    /// ドラッグ中のツールチップ。編集のドラッグの間だけ。量はポインターの向き（右が正）にし、
    /// `slip` は中身が動いた向きにする（操作の量は素材のイン点の向きで、ポインターと逆）。
    pub(super) fn drag_tooltip(&self) -> Option<DragTooltip> {
        let (kind, by) = self.last_edit?;
        let by = if matches!(kind, EditKind::Slip { .. }) {
            -by
        } else {
            by
        };
        Some(DragTooltip {
            at: self.pointer,
            by,
        })
    }
}

/// 掴んだ端の、動かす前の時刻。
fn edge_time(model: &TimelineModel, item: usize, cut: Option<usize>, part: Part) -> Option<f64> {
    let it = model.items.get(item)?;
    let (start, end) = match cut {
        Some(c) => it.cuts.get(c).map(|bar| (bar.start, bar.end))?,
        None => (it.start, it.end),
    };
    match part {
        Part::InHandle => Some(start),
        Part::OutHandle => Some(end),
        Part::Body => None,
    }
}
