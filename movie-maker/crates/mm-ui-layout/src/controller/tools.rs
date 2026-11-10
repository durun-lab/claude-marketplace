//! ツールごとの押下とホバー。Selection はレイヤーの帯を動かし、Trim はカットの中をずらし、Blade はカットを割る。
//! 割る時刻はポインターの時刻を再生位置（Snapping が入のとき）とフレームに合わせたもので、ホバー中の縦線も同じ時刻を指す。

use super::{Action, Drag, Mods, TimelineController};
use crate::gesture::{layer_move, EditKind};
use crate::snap::{quantize, snap, targets, Moving, SnapKind, SnapTarget, SNAP_PX};
use crate::timeline::{Hit, Part, TimelineModel};
use crate::tool::{BladeLine, Tool};

/// 吸い付いた帯の端が目標に届いたとみなす差（秒）。
const LANDED_EPS: f64 = 1e-9;

impl TimelineController {
    /// ツールを選ぶ。ドラッグの途中は変えない（つかんだ編集を途中で別の種類にしない）。
    pub(super) fn set_tool(&mut self, tool: Tool, model: &TimelineModel, playhead: f64) {
        if self.drag == Drag::None {
            self.tool = tool;
            self.update_blade(model, playhead);
        }
    }

    /// Blade のホバー中の縦線。ポインターが割れるカットの上にあるときだけ。
    pub fn blade_line(&self) -> Option<BladeLine> {
        self.blade
    }

    /// ポインターの下のカットに合わせて割る時刻の縦線を置き直す。Blade でなければ、隙間の上とカットの外では出さない。
    pub(super) fn update_blade(&mut self, model: &TimelineModel, playhead: f64) {
        self.blade = None;
        if self.tool != Tool::Blade || self.drag != Drag::None {
            return;
        }
        let Hit::Cut { item, cut, .. } = self.hover else {
            return;
        };
        let splittable = model
            .items
            .get(item)
            .and_then(|it| it.cuts.get(cut))
            .is_some_and(|bar| !bar.gap);
        if splittable {
            let time = self.blade_time(self.pointer.0, model, playhead);
            self.blade = Some(BladeLine { item, time });
        }
    }

    /// x の下の時刻を、Snapping が入なら再生位置に吸い付け、いちばん近いフレームの先頭にそろえた割る時刻。
    fn blade_time(&self, x: f32, model: &TimelineModel, playhead: f64) -> f64 {
        let t = self.viewport().t_of(x);
        let near = std::iter::once(SnapTarget {
            time: playhead,
            kind: SnapKind::Playhead,
        });
        let time = if self.snapping {
            snap(t, near, self.view.px_per_sec, SNAP_PX).time
        } else {
            t
        };
        quantize(time, model.fps)
    }

    /// Blade での押下。カットの上なら割る操作を返し、レイヤーの上では何もしない。それ以外の所は通常の押下に任せる（`None`）。
    pub(super) fn press_blade(
        &self,
        hit: Hit,
        x: f32,
        model: &TimelineModel,
        playhead: f64,
    ) -> Option<Vec<Action>> {
        match hit {
            Hit::Cut { item, cut, .. } => {
                let gap = model.items.get(item)?.cuts.get(cut)?.gap;
                let time = self.blade_time(x, model, playhead);
                Some(if gap {
                    Vec::new()
                } else {
                    vec![Action::Split { item, time }]
                })
            }
            Hit::Layer { .. } => Some(Vec::new()),
            _ => None,
        }
    }

    /// レイヤーの帯を押したときのドラッグ。端は `trimLayer`、中は Selection のときだけレイヤーを動かす。
    pub(super) fn layer_press(&self, item: usize, part: Part, mods: Mods, x: f32) -> Drag {
        match part {
            Part::Body if self.tool == Tool::Selection => Drag::Move {
                item,
                from: self.viewport().t_of(x),
            },
            Part::Body => Drag::None,
            _ => Drag::Edge {
                item,
                cut: None,
                part,
                mods,
            },
        }
    }

    /// カットを押したときのドラッグ。端は `ripple` と `roll`、中は Trim のときだけ `slip` と `slide`。
    pub(super) fn cut_press(
        &self,
        (item, cut, part): (usize, usize, Part),
        mods: Mods,
        x: f32,
    ) -> Drag {
        match part {
            Part::Body if self.tool == Tool::Trim => Drag::Body {
                item,
                cut,
                mods,
                from: self.viewport().t_of(x),
            },
            Part::Body => Drag::None,
            _ => Drag::Edge {
                item,
                cut: Some(cut),
                part,
                mods,
            },
        }
    }

    /// レイヤーの帯を x へドラッグしたときの編集。Snapping が入なら、帯の始まりと終わりのうち近い方を近い目標に寄せ、
    /// 吸い付かなければいちばん近いフレームに量をそろえる。0 で止めた量では吸い付いた表示を出さない。
    pub(super) fn move_drag(
        &mut self,
        x: f32,
        model: &TimelineModel,
        playhead: f64,
    ) -> Option<(EditKind, f64)> {
        let Drag::Move { item, from } = self.drag else {
            return None;
        };
        let raw = self.viewport().t_of(x) - from;
        let landed = if self.snaps() {
            self.nearest_edge_snap(model, item, raw, playhead)
        } else {
            None
        };
        let by = landed.map_or_else(|| quantize(raw, model.fps), |(_, by, _)| by);
        let edit = layer_move(model, item, by);
        self.snap = landed
            .zip(edit)
            .filter(|((target, _, edge), (_, by))| (edge + by - target.time).abs() < LANDED_EPS)
            .map(|((target, ..), _)| target);
        edit
    }

    /// 帯 `item` を `raw` 動かしたときの始まりと終わりのうち、目標に近い方への吸い付き（目標、その量、動かす前の端の時刻）。
    /// 動かす帯の端と、同じ帯のキーフレーム（一緒に動く）は目標にしない。
    fn nearest_edge_snap(
        &self,
        model: &TimelineModel,
        item: usize,
        raw: f64,
        playhead: f64,
    ) -> Option<(SnapTarget, f64, f64)> {
        let it = model.items.get(item)?;
        let own = SnapKind::Keyframe { item };
        let near = || targets(model, Moving::Layer(item), playhead).filter(move |t| t.kind != own);
        let px = self.view.px_per_sec;
        [it.start, it.end]
            .into_iter()
            .filter_map(|edge| {
                let target = snap(edge + raw, near(), px, SNAP_PX).target?;
                Some((target, target.time - edge, edge))
            })
            .min_by(|a, b| (a.1 - raw).abs().total_cmp(&(b.1 - raw).abs()))
    }
}
