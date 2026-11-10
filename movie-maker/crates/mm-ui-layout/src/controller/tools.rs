//! ツールごとの押下とホバー。Selection はレイヤーの帯を動かし、Trim はカットの中をずらし、Blade はカットを割る。
//! 割る時刻はポインターの時刻を再生位置（Snapping が入のとき）とフレームに合わせたもので、ホバー中の縦線も同じ時刻を指す。

use super::snapping::LANDED_EPS;
use super::{Action, Drag, Mods, TimelineController};
use crate::gesture::{shift_layer_edit, EditKind};
use crate::snap::{quantize, snap, targets, Moving, SnapKind, SnapTarget, SNAP_PX};
use crate::timeline::{hit_test, CutBar, Hit, Part, TimelineModel};
use crate::tool::{BladeLine, Tool};

/// 割る時刻がカットの端に重なったとみなす差（秒）。フレームに丸めた時刻と、有理数の時刻から求めた端の誤差を吸収する。
const EDGE_EPS: f64 = 1e-6;

impl TimelineController {
    /// ツールを選ぶ。ドラッグの途中は変えない（つかんだ編集を途中で別の種類にしない）。
    pub(super) fn set_tool(&mut self, tool: Tool, model: &TimelineModel, playhead: f64) {
        if self.drag == Drag::None {
            self.tool = tool;
            self.update_blade_line(model, playhead);
        }
    }

    /// Blade のホバー中の縦線。ポインターが割れるカットの上にあるときだけ。
    pub fn blade_line(&self) -> Option<BladeLine> {
        self.blade_line
    }

    /// ポインターの下のカットに割る時刻の縦線を置き直す。Blade でないとき、ドラッグ中、割れない所（カットの外、隙間、
    /// 割る時刻がカットの端に丸まる所）では出さない。
    pub(super) fn update_blade_line(&mut self, model: &TimelineModel, playhead: f64) {
        self.blade_line = match self.hover {
            Hit::Cut { item, cut, .. } if self.tool == Tool::Blade && self.drag == Drag::None => {
                self.blade_at((item, cut), self.pointer.0, model, playhead)
            }
            _ => None,
        };
    }

    /// カット `(item, cut)` を x で割るときの線。隙間と、割る時刻がカットの端に丸まる所では `None`（割るものが無い）。
    /// 右クリックのメニューの Split Clip Here も、この時刻で割る。
    pub(super) fn blade_at(
        &self,
        (item, cut): (usize, usize),
        x: f32,
        model: &TimelineModel,
        playhead: f64,
    ) -> Option<BladeLine> {
        let bar = model.items.get(item)?.cuts.get(cut)?;
        let (time, snapped) = self.blade_time(x, model, playhead);
        splits(bar, time).then_some(BladeLine {
            item,
            time,
            snapped,
        })
    }

    /// x の下の時刻を、Snapping が入なら再生位置に吸い付け、いちばん近いフレームの先頭にそろえた割る時刻と、
    /// 再生位置に吸い付いたか。
    fn blade_time(&self, x: f32, model: &TimelineModel, playhead: f64) -> (f64, bool) {
        let t = self.viewport().t_of(x);
        let near = std::iter::once(SnapTarget {
            time: playhead,
            kind: SnapKind::Playhead,
        });
        let snapped = self
            .snapping
            .then(|| snap(t, near, self.view.px_per_sec, SNAP_PX).target)
            .flatten();
        let time = snapped.map_or(t, |target| target.time);
        (quantize(time, model.fps), snapped.is_some())
    }

    /// 押下。Blade のときのカットとレイヤーの上は Blade が受け、ほかはキーフレームの行と通常の押下（`press`）に任せる。
    /// 押すと Blade の線は消す。
    pub(super) fn press_with_tool(
        &mut self,
        (x, y): (f32, f32),
        mods: Mods,
        (model, playhead): (&TimelineModel, f64),
    ) -> Vec<Action> {
        // ステータスバーは文を出すだけで、押しても選択を変えない。
        if self.layout.status.contains(x, y) {
            return Vec::new();
        }
        self.blade_line = None;
        if self.tool == Tool::Blade {
            let hit = hit_test(model, &self.viewport(), x, y);
            if let Some(out) = self.press_blade(hit, x, model, playhead) {
                return out;
            }
        }
        self.press((x, y), mods, model, playhead)
    }

    /// Blade での押下。割れるカットの上なら割る操作を返し、ほかのカットとレイヤーの上では何もしない。
    /// それ以外の所は通常の押下に任せる（`None`）。
    fn press_blade(
        &self,
        hit: Hit,
        x: f32,
        model: &TimelineModel,
        playhead: f64,
    ) -> Option<Vec<Action>> {
        match hit {
            Hit::Cut { item, cut, .. } => {
                let line = self.blade_at((item, cut), x, model, playhead);
                Some(
                    line.map(|l| Action::Split { item, time: l.time })
                        .into_iter()
                        .collect(),
                )
            }
            Hit::Layer { .. } => Some(Vec::new()),
            _ => None,
        }
    }

    /// レイヤーの帯を押したときのドラッグ。端は `trimLayer`、中は Selection のときだけレイヤーを動かす。
    pub(super) fn layer_press(&self, item: usize, part: Part, mods: Mods, x: f32) -> Drag {
        match part {
            Part::Body if self.tool == Tool::Selection => Drag::ShiftLayer {
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
    pub(super) fn shift_layer_drag(
        &mut self,
        x: f32,
        model: &TimelineModel,
        playhead: f64,
    ) -> Option<(EditKind, f64)> {
        let Drag::ShiftLayer { item, from } = self.drag else {
            return None;
        };
        let raw = self.viewport().t_of(x) - from;
        let landed = if self.snaps() {
            self.nearest_edge_snap(model, item, raw, playhead)
        } else {
            None
        };
        let by = landed.map_or_else(|| quantize(raw, model.fps), |(_, by, _)| by);
        let edit = shift_layer_edit(model, item, by);
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

/// カット `bar` を時刻 `time` で割れるか。隙間と、端に丸まる時刻では割れない。
pub(super) fn splits(bar: &CutBar, time: f64) -> bool {
    !bar.gap && time - bar.start > EDGE_EPS && bar.end - time > EDGE_EPS
}
