//! 描画スレッドの、再生の操作のバーと共有状態の行き来。バーの状態（再生中か、押せないコマンド、Source の位置とイン点・アウト点）を
//! `Ui` に渡し、バーのボタンとジョグバーの操作を、キーと同じコマンドの表と再生のコマンドに送る。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mm_ui_layout::{TransportAction, Viewer, ViewerState};
use mm_ui_wgpu::Ui;

use crate::win::commands;
use crate::win::playback::Command;
use crate::win::state::Shared;

/// 押せるかの計算を、変わったときだけやり直すための覚え。
#[derive(Default)]
pub(super) struct TransportSync {
    /// 押せるかを決める状態の通し番号（編集、カットの選択、Source を開く・閉じる）。
    key: Option<(u64, u64, u64)>,
    disabled: Vec<&'static str>,
    /// 開いている素材のイン点とアウト点（素材の秒）。
    marks: (Option<f64>, Option<f64>),
}

impl TransportSync {
    pub(super) fn refresh(&mut self, shared: &Shared, ui: &mut Ui) {
        let source = shared.source.handle();
        let key = (
            shared.doc_serial.load(Ordering::SeqCst),
            shared.edit.cut_serial.load(Ordering::SeqCst),
            shared.source.serial.load(Ordering::SeqCst),
        );
        if self.key != Some(key) {
            self.key = Some(key);
            self.disabled = commands::disabled_ids(shared);
            self.marks = source.as_ref().map_or((None, None), |h| {
                crate::win::source::marks_of(shared, &h.asset)
            });
        }
        let program = ViewerState {
            playing: shared.playing.load(Ordering::SeqCst),
            disabled: self.disabled.clone(),
            ..ViewerState::default()
        };
        let source = source.map(|h| ViewerState {
            t: h.shared.playhead(),
            duration: h.duration,
            fps: h.fps.to_f64(),
            playing: h.shared.playing.load(Ordering::SeqCst),
            mark_in: self.marks.0,
            mark_out: self.marks.1,
            disabled: self.disabled.clone(),
        });
        ui.set_transport(program, source);
    }
}

/// バーの操作を実行する。ボタンはコマンドの表で（押せないものはバーが送らない）、ジョグバーは該当のビューアーのシークで。
/// Program のシークは Timeline のスクラブと同じ経路（1 周で最後の 1 つ）で送るので、呼ぶ側が扱う。
pub(super) fn run(shared: &Arc<Shared>, action: TransportAction) {
    match action {
        TransportAction::Command(id) => {
            if let Err(e) = commands::run(shared, id) {
                shared.edit.set_message(e);
            }
        }
        TransportAction::Seek {
            viewer: Viewer::Source,
            t,
        } => {
            if let Some(h) = shared.source.handle() {
                if h.shared.playing.load(Ordering::SeqCst) {
                    shared.source.send(Command::Pause);
                }
                shared.source.send(Command::Seek(t));
            }
        }
        TransportAction::Seek {
            viewer: Viewer::Program,
            ..
        } => {}
    }
}
