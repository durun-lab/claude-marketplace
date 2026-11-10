//! 描画スレッドの、再生の操作のバーと共有状態の行き来。バーの状態（再生中か、押せないコマンド、Source の位置とイン点・アウト点）を
//! `Ui` に渡し、バーのボタンをキーと同じコマンドの表で実行し、ジョグバーと Timeline のシークを再生のコマンドにする。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mm_ui_layout::{Viewer, ViewerState};
use mm_ui_wgpu::Ui;

use crate::win::commands;
use crate::win::playback::Command;
use crate::win::state::Shared;

/// 押せるかとイン点・アウト点を、通し番号が変わったときだけ計算し直すために持つ、直前の計算の結果。
#[derive(Default)]
pub(super) struct TransportSync {
    /// 押せるかを決める状態の通し番号（編集、カットの選択、Source を開く・閉じる）。
    key: Option<(u64, u64, u64)>,
    disabled: Vec<&'static str>,
    /// 開いている素材のイン点とアウト点（素材の秒）。
    in_out: (Option<f64>, Option<f64>),
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
            self.in_out = source.as_ref().map_or((None, None), |h| {
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
            mark_in: self.in_out.0,
            mark_out: self.in_out.1,
            disabled: self.disabled.clone(),
        });
        ui.set_transport(program, source);
    }
}

/// バーのボタンのコマンドを、キーやメニューと同じコマンドの表で実行する。押せないボタンはバーが送らない。
pub(super) fn run_command(shared: &Arc<Shared>, id: &str) {
    if let Err(e) = commands::run(shared, id) {
        shared.edit.set_message(e);
    }
}

/// 描画の 1 周に溜まったシーク。ビューアーごとに最後の 1 つだけを送る。途中の位置は次の位置に上書きされるので、
/// マウスの移動のたびに再生スレッドにフレームを描かせない。
#[derive(Debug, Default, PartialEq)]
pub(super) struct Seeks {
    program: Option<f64>,
    source: Option<f64>,
}

impl Seeks {
    pub(super) fn set(&mut self, viewer: Viewer, t: f64) {
        match viewer {
            Viewer::Program => self.program = Some(t),
            Viewer::Source => self.source = Some(t),
        }
    }

    /// キーとメニューの Timeline のコマンドのシーク（前後の帯の端）。Home と End と同じく再生を止めずにすぐ送り、
    /// それより前に溜めた Program のシークは捨てる（後の入力が勝つ）。
    pub(super) fn jump(&mut self, shared: &Shared, t: f64) {
        self.program = None;
        shared.send(Command::Seek(t));
    }

    /// 溜めたシークを送る。スクラブは再生を止めて位置を追う（Timeline の目盛りのクリックと同じ）。
    pub(super) fn send(self, shared: &Shared) {
        if let Some(t) = self.program {
            if shared.playing.load(Ordering::SeqCst) {
                shared.send(Command::Pause);
            }
            shared.send(Command::Seek(t));
        }
        if let (Some(t), Some(h)) = (self.source, shared.source.handle()) {
            if h.shared.playing.load(Ordering::SeqCst) {
                shared.source.send(Command::Pause);
            }
            shared.source.send(Command::Seek(t));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use super::*;

    #[test]
    fn シークはビューアーごとに最後の_1_つだけを送る() {
        let mut seeks = Seeks::default();
        for t in [1.0, 2.0, 3.0] {
            seeks.set(Viewer::Program, t);
        }
        seeks.set(Viewer::Source, 0.5);
        seeks.set(Viewer::Source, 0.75);
        assert_eq!(
            seeks,
            Seeks {
                program: Some(3.0),
                source: Some(0.75)
            }
        );
        let shared = Shared::new();
        let (tx, rx) = channel();
        *shared.commands.lock().unwrap() = Some(tx);
        seeks.send(&shared);
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Command::Seek(3.0)]);
    }

    #[test]
    fn 再生中のスクラブは止めてから動かす() {
        let shared = Shared::new();
        let (tx, rx) = channel();
        *shared.commands.lock().unwrap() = Some(tx);
        shared.playing.store(true, Ordering::SeqCst);
        let mut seeks = Seeks::default();
        seeks.set(Viewer::Program, 2.0);
        seeks.send(&shared);
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            [Command::Pause, Command::Seek(2.0)]
        );
    }

    #[test]
    fn 前後の帯の端へのシークは再生を止めずにすぐ送り_前に溜めたスクラブを捨てる() {
        let shared = Shared::new();
        let (tx, rx) = channel();
        *shared.commands.lock().unwrap() = Some(tx);
        shared.playing.store(true, Ordering::SeqCst);
        let mut seeks = Seeks::default();
        seeks.set(Viewer::Program, 1.0);
        seeks.jump(&shared, 2.0);
        seeks.send(&shared);
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Command::Seek(2.0)]);
    }
}
