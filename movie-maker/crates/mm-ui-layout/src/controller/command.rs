//! キーとメニューから Timeline に送るコマンド。表示の範囲を変えるもの（拡大縮小、全体表示）、再生位置を前後の帯の端へ
//! 動かすもの（Previous Edit、Next Edit）、スナップの入/切。

use super::zoom::ZOOM_KEY_STEP;
use super::{Action, TimelineController};
use crate::snap::quantize;
use crate::timeline::TimelineModel;

/// キーとメニューから Timeline に送るコマンド。UI のコマンドの表の Timeline の行が指し、ポインターの入力
/// （[`super::PointerEvent::Command`]）で届く。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineCommand {
    /// 再生位置を中心に拡大する（Zoom In）。
    ZoomIn,
    /// 再生位置を中心に縮小する（Zoom Out）。
    ZoomOut,
    /// 全体表示にする。全体表示のままならその前の表示に戻す（Full Extent Zoom）。
    FullExtentZoom,
    /// 再生位置を前の帯の端へ動かす（Previous Edit）。
    PreviousEdit,
    /// 再生位置を次の帯の端へ動かす（Next Edit）。
    NextEdit,
    /// スナップの入/切（Snapping）。ドラッグ中は入/切を変えず、[`super::PointerEvent::SnapKeyUp`] まで反転する。
    ToggleSnapping,
}

impl TimelineController {
    /// コマンドを 1 つ受けて、起きた操作を返す。前後の帯の端へ動かすコマンドは再生位置のシークを返す。
    pub(super) fn command(
        &mut self,
        command: TimelineCommand,
        model: &TimelineModel,
        playhead: f64,
    ) -> Vec<Action> {
        let duration = model.duration;
        match command {
            TimelineCommand::ZoomIn => self.zoom_at_playhead(ZOOM_KEY_STEP, playhead, duration),
            TimelineCommand::ZoomOut => {
                self.zoom_at_playhead(1.0 / ZOOM_KEY_STEP, playhead, duration);
            }
            TimelineCommand::FullExtentZoom => self.toggle_full_extent(duration),
            TimelineCommand::PreviousEdit => return seek(edit_target(model, playhead, false)),
            TimelineCommand::NextEdit => return seek(edit_target(model, playhead, true)),
            TimelineCommand::ToggleSnapping => return self.toggle_snapping(model, playhead),
        }
        Vec::new()
    }
}

fn seek(target: Option<f64>) -> Vec<Action> {
    target.map(Action::Seek).into_iter().collect()
}

/// Previous Edit（`forward` が false）と Next Edit の移動先。再生位置の前か後ろの、いちばん近い帯の端（レイヤーとカットの
/// 始まりと終わり）か 0 か終わりを、いちばん近いフレームの先頭にそろえ、最後のフレームの始まりまでに収める。
/// 再生位置から半フレームより近い端は今いる所として数えず、端に止まった再生位置から同じ端へ戻らない。無ければ `None`。
pub fn edit_target(model: &TimelineModel, playhead: f64, forward: bool) -> Option<f64> {
    let frame = if model.fps > 0.0 {
        1.0 / model.fps
    } else {
        0.0
    };
    let last = (model.duration - frame).max(0.0);
    let stops = edges(model).map(|t| quantize(t, model.fps).clamp(0.0, last));
    let half = frame / 2.0;
    if forward {
        stops
            .filter(|t| *t > playhead + half)
            .min_by(f64::total_cmp)
    } else {
        stops
            .filter(|t| *t < playhead - half)
            .max_by(f64::total_cmp)
    }
}

/// 帯の端（0、終わり、レイヤーとカットの始まりと終わり）。スナップの目標とは別に持ち、キーフレームやマーカーで止まらない。
fn edges(model: &TimelineModel) -> impl Iterator<Item = f64> + '_ {
    let items = model.items.iter().flat_map(|it| {
        let cuts = it.cuts.iter().flat_map(|c| [c.start, c.end]);
        [it.start, it.end].into_iter().chain(cuts)
    });
    [0.0, model.duration].into_iter().chain(items)
}
