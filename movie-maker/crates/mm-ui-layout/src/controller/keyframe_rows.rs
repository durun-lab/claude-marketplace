//! キーフレームの操作。帯のマーカーを押すとその時刻へ再生位置を動かす。キーフレームの行では、キーフレームを押すと選び
//! （レイヤーと、Inspector と Graph のパラメータも選ぶ）、ドラッグで動かし（離したときに 1 回の `moveKeyframe`。
//! Esc で取りやめ）、Alt+クリックでその時刻に打ち（Resolve の Option+クリック）、Remove Keyframe（Delete）で外す。
//! 見出しのキーフレームの開閉と Show Keyframe Editor（U）で行を開閉し、Go to Previous / Next Keyframe（[ と ]）で
//! 再生位置を動かす。
//!
//! 選んだキーフレームはレイヤーの ID、行の名前、時刻で持ち、操作の後にモデルが作り直されても同じキーフレームを指す。

use std::sync::Arc;

use super::command::{seek, stop_target};
use super::{deselect_cut, Action, Drag, Mods, TimelineCommand, TimelineController};
use crate::drag_tooltip::DragTooltip;
use crate::gesture::frame_secs;
use crate::keyframe_rows::{KeyframeAction, KeyframeIndex, KeyframeRowState, RowKeyframe, RowPick};
use crate::snap::{quantize, snap, targets, Moving, SnapKind, SnapTarget, SNAP_PX};
use crate::timeline::{hit_test, Hit, TimelineModel};

/// 吸い付いた目標に届いたとみなす差（秒）。動かせる範囲で止まれば、スナップの表示を出さない。
const LANDED_EPS: f64 = 1e-9;
/// 同じキーフレームとみなす時刻の差（秒）。モデルを作り直しても、有理数の時刻を浮動小数にした同じ値になる。
const SAME_TIME: f64 = 1e-6;

/// キーフレームのドラッグ。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct KeyframeDrag {
    /// 動かすキーフレーム。
    at: KeyframeIndex,
    /// 動かす前の時刻。
    from: f64,
    /// 押した点の時刻。ポインターとキーフレームのずれを保って動かす。
    grab_time: f64,
    /// 動かせる範囲。
    span: (f64, f64),
    /// 動かした先。押しただけでまだ動いていなければ `None`。
    to: Option<f64>,
}

/// キーフレームの行で選んだもの。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Picked {
    /// レイヤーの ID と、キーフレームの行の名前。モデルを作り直したら、これで行を引き直す。
    layer: Arc<str>,
    name: Arc<str>,
    item: usize,
    row: usize,
    keyframe: Option<usize>,
    /// 選んだキーフレームの時刻。動かした直後と打った直後は、操作の後の時刻。
    time: Option<f64>,
    /// 動かす前の時刻。操作が拒否されてモデルが変わらなければ、こちらで引く。
    before: Option<f64>,
}

impl Picked {
    fn pick(&self) -> RowPick {
        RowPick {
            item: self.item,
            row: self.row,
            keyframe: self.keyframe,
        }
    }

    /// モデル `model` で引き直した選択。行が無くなっていれば `None`。キーフレームが無くなっていれば行だけを選ぶ。
    fn resolved(&self, model: &TimelineModel) -> Option<Self> {
        let item = match model.items.get(self.item) {
            Some(it) if it.id == self.layer => self.item,
            _ => model.items.iter().position(|it| it.id == self.layer)?,
        };
        let rows = &model.items[item].keyframe_rows;
        let row = match rows.get(self.row) {
            Some(r) if r.name == self.name => self.row,
            _ => rows.iter().position(|r| r.name == self.name)?,
        };
        let keyframes = &rows[row].keyframes;
        let find = |t: f64| {
            keyframes
                .iter()
                .position(|k| (k.time - t).abs() < SAME_TIME)
        };
        let keyframe = self
            .time
            .and_then(find)
            .or_else(|| self.before.and_then(find));
        Some(Self {
            item,
            row,
            keyframe,
            time: keyframe.map(|k| keyframes[k].time),
            before: None,
            ..self.clone()
        })
    }
}

impl TimelineController {
    /// 押した。キーフレームの行と見出しのキーフレームの開閉はここで受け、ほかは今の押し方（`down`）に渡す。
    /// レイヤーやカットの選択が変わる所を押したら、キーフレームの行の選択を外す。
    pub(super) fn press(
        &mut self,
        (x, y): (f32, f32),
        mods: Mods,
        model: &TimelineModel,
        playhead: f64,
    ) -> Vec<Action> {
        let hit = hit_test(model, &self.viewport(), x, y);
        match hit {
            Hit::Disclosure { index } => {
                return vec![Action::Keyframe(KeyframeAction::ToggleRows(Some(index)))];
            }
            Hit::RowKeyframe(at) => return self.press_keyframe(at, x, model),
            Hit::KeyframeRow { item, row } if mods.alt => {
                return self.add_keyframe((item, row), x, model, playhead);
            }
            Hit::KeyframeRow { item, row } | Hit::KeyframeRowHeader { item, row } => {
                return self.pick_row(model, (item, row), None);
            }
            _ => {}
        }
        let clears = matches!(
            hit,
            Hit::Layer { .. }
                | Hit::Cut { .. }
                | Hit::BarMark { .. }
                | Hit::Header { .. }
                | Hit::Empty
        );
        let had = if clears { self.pick.take() } else { None };
        let mut out = self.down(x, y, mods, model);
        if had.is_some() {
            out.push(Action::Keyframe(KeyframeAction::Select(None)));
        }
        out
    }

    /// 帯のマーカーを押した。レイヤーを選び、再生位置をそのキーフレームの時刻へ動かす（ドラッグは始めない）。
    pub(super) fn press_bar_mark(
        &mut self,
        (item, keyframe): (usize, usize),
        had_cut: Option<(usize, usize)>,
        model: &TimelineModel,
    ) -> Vec<Action> {
        self.selected = Some(item);
        let t = model
            .items
            .get(item)
            .and_then(|it| it.keyframes.get(keyframe));
        let mut out = deselect_cut(Action::Select(Some(item)), had_cut);
        out.extend(t.map(|t| Action::Seek(Self::seek_time(*t, model))));
        out
    }

    /// キーフレームを押した。レイヤーと行とキーフレームを選び、ドラッグの準備をする。
    fn press_keyframe(&mut self, at: KeyframeIndex, x: f32, model: &TimelineModel) -> Vec<Action> {
        let Some(k) = row_keyframe(model, at) else {
            return Vec::new();
        };
        self.drag = Drag::Keyframe(KeyframeDrag {
            at,
            from: k.time,
            grab_time: self.viewport().t_of(x),
            span: (k.min, k.max.max(k.min)),
            to: None,
        });
        self.pick_row(model, (at.item, at.row), Some((at.keyframe, k.time)))
    }

    /// レイヤーとキーフレームの行（とキーフレーム）を選ぶ。カットの選択は外す。
    fn pick_row(
        &mut self,
        model: &TimelineModel,
        (item, row): (usize, usize),
        keyframe: Option<(usize, f64)>,
    ) -> Vec<Action> {
        let Some(it) = model.items.get(item) else {
            return Vec::new();
        };
        let Some(r) = it.keyframe_rows.get(row) else {
            return Vec::new();
        };
        let picked = Picked {
            layer: it.id.clone(),
            name: r.name.clone(),
            item,
            row,
            keyframe: keyframe.map(|k| k.0),
            time: keyframe.map(|k| k.1),
            before: None,
        };
        let pick = picked.pick();
        self.pick = Some(picked);
        self.selected = Some(item);
        let mut out = deselect_cut(Action::Select(Some(item)), self.selected_cut.take());
        out.push(Action::Keyframe(KeyframeAction::Select(Some(pick))));
        out
    }

    /// Alt+クリック。押した時刻（スナップが効いていれば近い目標、吸い付かなければいちばん近いフレームの先頭）に
    /// キーフレームを打ち、打ったキーフレームを選ぶ（モデルが作り直されると、その時刻で引ける）。
    fn add_keyframe(
        &mut self,
        (item, row): (usize, usize),
        x: f32,
        model: &TimelineModel,
        playhead: f64,
    ) -> Vec<Action> {
        let last = (model.duration - frame_secs(model)).max(0.0);
        let (time, _) = self.place_time(self.viewport().t_of(x), model, playhead, (item, row));
        let time = time.clamp(0.0, last);
        let mut out = self.pick_row(model, (item, row), None);
        if let Some(p) = &mut self.pick {
            p.time = Some(time);
        }
        out.push(Action::Keyframe(KeyframeAction::Add { item, row, time }));
        out
    }

    /// 時刻 `t` を、スナップが効いていれば近い目標（行 `(item, row)` の自分のキーフレームは除く）に寄せ、吸い付かなければ
    /// いちばん近いフレームの先頭にそろえる。吸い付いた目標も返す。
    fn place_time(
        &self,
        t: f64,
        model: &TimelineModel,
        playhead: f64,
        (item, row): (usize, usize),
    ) -> (f64, Option<SnapTarget>) {
        let own: &[RowKeyframe] = model
            .items
            .get(item)
            .and_then(|it| it.keyframe_rows.get(row))
            .map_or(&[], |r| &r.keyframes);
        let is_own = |s: &SnapTarget| {
            matches!(s.kind, SnapKind::Keyframe { item: i } if i == item)
                && own.iter().any(|k| (k.time - s.time).abs() < SAME_TIME)
        };
        let target = if self.snaps() {
            let near = targets(model, Moving::Keyframe, playhead).filter(|s| !is_own(s));
            snap(t, near, self.view.px_per_sec, SNAP_PX).target
        } else {
            None
        };
        (
            target.map_or_else(|| quantize(t, model.fps), |s| s.time),
            target,
        )
    }

    /// キーフレームのドラッグ中にポインターが `x` へ動いた。動かせる範囲に収め、届いた目標だけをスナップの表示にする。
    pub(super) fn drag_keyframe(
        &mut self,
        drag: KeyframeDrag,
        x: f32,
        model: &TimelineModel,
        playhead: f64,
    ) -> Vec<Action> {
        let t = drag.from + (self.viewport().t_of(x) - drag.grab_time);
        let (time, target) = self.place_time(t, model, playhead, (drag.at.item, drag.at.row));
        let to = time.clamp(drag.span.0, drag.span.1);
        self.snap = target.filter(|s| (s.time - to).abs() < LANDED_EPS);
        self.drag = Drag::Keyframe(KeyframeDrag {
            to: Some(to),
            ..drag
        });
        Vec::new()
    }

    /// キーフレームのドラッグを離した。半フレーム以上動かし、同じ行のほかのキーフレームと重ならなければ `moveKeyframe` にする。
    pub(super) fn release_keyframe(
        &mut self,
        drag: KeyframeDrag,
        model: &TimelineModel,
    ) -> Vec<Action> {
        let half = frame_secs(model) / 2.0;
        let Some(to) = drag.to.filter(|to| (to - drag.from).abs() >= half) else {
            return Vec::new();
        };
        let keyframes = model
            .items
            .get(drag.at.item)
            .and_then(|it| it.keyframe_rows.get(drag.at.row))
            .map_or(&[][..], |r| &r.keyframes);
        let taken = keyframes
            .iter()
            .enumerate()
            .any(|(j, k)| j != drag.at.keyframe && (k.time - to).abs() < half);
        if taken {
            return Vec::new();
        }
        if let Some(p) = &mut self.pick {
            (p.time, p.before) = (Some(to), Some(drag.from));
        }
        vec![Action::Keyframe(KeyframeAction::Move {
            keyframe: drag.at,
            to,
        })]
    }

    /// キーフレームの行のコマンド（Go to Previous / Next Keyframe、Show Keyframe Editor、Remove Keyframe）。
    pub(super) fn keyframe_command(
        &mut self,
        command: TimelineCommand,
        model: &TimelineModel,
        playhead: f64,
    ) -> Vec<Action> {
        let selected = self.selected;
        let items = model.items.iter().enumerate();
        let mine = items.filter(move |(i, _)| selected.is_none_or(|s| s == *i));
        let times = mine.flat_map(|(_, it)| it.keyframes.iter().copied());
        match command {
            TimelineCommand::PreviousKeyframe => seek(stop_target(model, times, playhead, false)),
            TimelineCommand::NextKeyframe => seek(stop_target(model, times, playhead, true)),
            TimelineCommand::ShowKeyframeEditor => {
                vec![Action::Keyframe(KeyframeAction::ToggleRows(self.selected))]
            }
            TimelineCommand::RemoveKeyframe => self.remove_keyframe(),
            _ => Vec::new(),
        }
    }

    /// 選んだキーフレームを外す。行の選択は残す（Inspector と Graph のパラメータもそのまま）。
    fn remove_keyframe(&mut self) -> Vec<Action> {
        let Some(p) = &mut self.pick else {
            return Vec::new();
        };
        let Some(at) = p.pick().keyframe_index() else {
            return Vec::new();
        };
        (p.keyframe, p.time) = (None, None);
        let row = p.pick();
        vec![
            Action::Keyframe(KeyframeAction::Remove(at)),
            Action::Keyframe(KeyframeAction::Select(Some(row))),
        ]
    }

    /// 行 `i` の下のキーフレームの行が全部見える所までスクロールする。行の領域より高ければレイヤーの行の上端を合わせる。
    pub fn reveal_keyframe_rows(&mut self, i: usize) {
        if i >= self.rows.len() {
            return;
        }
        let (top, below) = (self.rows.top(i), self.rows.below(i));
        let h = self.layout.row_area.h;
        let y = if below - top > h || top < self.scroll_y {
            top
        } else if below > self.scroll_y + h {
            below - h
        } else {
            self.scroll_y
        };
        self.scroll_to(y);
    }

    /// キーフレームの行で選んだもの。
    pub fn keyframe_pick(&self) -> Option<RowPick> {
        self.pick.as_ref().map(Picked::pick)
    }

    /// モデルが変わった。選択を引き直す。
    pub(super) fn repick(&mut self, model: &TimelineModel) {
        self.pick = self.pick.take().and_then(|p| p.resolved(model));
    }

    /// 行の並びが変わった。閉じたキーフレームの行の選択を外す。
    pub(super) fn keep_pick_in_rows(&mut self) {
        let rows = &self.rows;
        self.pick = self
            .pick
            .take()
            .filter(|p| p.item < rows.len() && p.row < rows.keyframe_rows(p.item));
    }

    /// 描く時点のキーフレームの行の状態。
    pub(super) fn keyframe_row_state(&self) -> KeyframeRowState {
        let dragging = match self.drag {
            Drag::Keyframe(d) => d.to.map(|to| (d.at, to)),
            _ => None,
        };
        KeyframeRowState {
            selected: self.keyframe_pick(),
            dragging,
        }
    }

    /// キーフレームのドラッグ中のツールチップ（動かした量）。
    pub(super) fn keyframe_tooltip(&self) -> Option<DragTooltip> {
        let Drag::Keyframe(d) = self.drag else {
            return None;
        };
        d.to.map(|to| DragTooltip {
            at: self.pointer,
            by: to - d.from,
        })
    }
}

/// 位置 `at` のキーフレーム。
fn row_keyframe(model: &TimelineModel, at: KeyframeIndex) -> Option<RowKeyframe> {
    let row = model.items.get(at.item)?.keyframe_rows.get(at.row)?;
    row.keyframes.get(at.keyframe).copied()
}

#[cfg(test)]
mod tests;
