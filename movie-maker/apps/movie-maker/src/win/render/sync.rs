//! 描画スレッドの、共有状態と UI の行き来。Timeline に出すプロジェクトのデータ、Source のフレーム、
//! ポインターの入力から選択とシークとドラッグの編集へ。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mm_ops::Op;
use mm_render_api::tone_map_peak;
use mm_types::Id;
use mm_ui_layout::{Action, EditKind, HeaderButton, PointerEvent, TimelineModel};
use mm_ui_wgpu::{timeline_cut_ids, timeline_layer_ids, timeline_model, Region, Ui};

use super::View;
use crate::win::playback::Command;
use crate::win::sequence_edit::{gesture_op, header_button_op, move_layer_op, TimelineIds};
use crate::win::state::{FrameImage, OpenDoc, Shared};
use crate::win::view_state::Shy;

/// 編集で変わったプロジェクトのデータを Timeline に反映する。Timeline の添字とレイヤー、カットの ID の対応も持つ。
#[derive(Default)]
pub(super) struct DocSync {
    seen: u64,
    /// Shy の状態の通し番号。変わったら、プロジェクトが同じでも行を作り直す。
    seen_shy: u64,
    /// カットの選択が変わったことを見分ける通し番号。
    seen_cut: u64,
    /// Timeline に出しているデータ。ドラッグの編集は、人が見ていたこのデータとそのバージョンで計画する。
    shown: Option<OpenDoc>,
    layer_ids: Vec<String>,
    /// レイヤーごとのカットの ID（シーケンスでないレイヤーは空）。
    cut_ids: Vec<Vec<String>>,
}

/// Source のフレームを Program と同じサーフェスの Source の領域に出す。
#[derive(Default)]
pub(super) struct SourceSync {
    serial: u64,
    seen: u64,
}

impl SourceSync {
    pub(super) fn refresh(&mut self, shared: &Shared, view: &mut View) {
        let serial = shared.source.serial.load(Ordering::SeqCst);
        let handle = shared.source.handle();
        if serial != self.serial {
            self.serial = serial;
            self.seen = 0;
            if handle.is_none() {
                view.ui.close_source();
                shared.source.set_shown(false);
            }
        }
        let Some(frame) = handle.and_then(|h| h.shared.frames.newer_than(self.seen)) else {
            return;
        };
        self.seen = frame.serial;
        // Source の再生スレッドは別のデバイスなので、CPU の画像で受ける（`SourceSlot::open`）。
        if let FrameImage::Cpu(image) = &frame.image {
            view.ui
                .set_source_frame(&view.gpu.device, &view.gpu.queue, image);
            shared.source.set_shown(true);
        }
    }
}

impl DocSync {
    pub(super) fn refresh(&mut self, shared: &Shared, ui: &mut Ui) {
        let serial = shared.doc_serial.load(Ordering::SeqCst);
        let cut_serial = shared.edit.cut_serial.load(Ordering::SeqCst);
        let shy_serial = shared.view.shy_serial();
        if serial != self.seen || shy_serial != self.seen_shy {
            (self.seen, self.seen_shy) = (serial, shy_serial);
            let Some(doc) = shared.doc() else { return };
            let Ok(comp) = doc.project.composition(&doc.comp) else {
                return;
            };
            let mut model = timeline_model(&doc.project, comp);
            // SDR の書き出しと同じ素材の上限の明るさでトーンマップし、プレビューと書き出しの映像を揃える。
            ui.set_hdr_peak(tone_map_peak(&doc.project));
            self.layer_ids = timeline_layer_ids(comp);
            self.cut_ids = timeline_cut_ids(&doc.project, comp);
            self.drop_shy(&shared.view.shy(), &mut model);
            ui.update_model(model);
            self.shown = Some(doc.clone());
            let selected = shared.selected_layer();
            ui.select(selected.and_then(|id| self.layer_ids.iter().position(|i| *i == id)));
        } else if cut_serial == self.seen_cut {
            return;
        }
        self.seen_cut = cut_serial;
        let selected = shared.edit.selected_cut();
        ui.select_cut(selected.and_then(|id| self.position_of_cut(&id)));
    }

    /// Shy で隠すレイヤーの行を、モデルと ID の対応（同じ並び）から外し、外した数をモデルに持たせる。
    /// 描画と書き出しはプロジェクトのデータを見るので変わらない。
    fn drop_shy(&mut self, shy: &Shy, model: &mut TimelineModel) {
        let keep: Vec<bool> = self.layer_ids.iter().map(|id| !shy.hides(id)).collect();
        model.hidden = keep.iter().filter(|k| !**k).count();
        keep_rows(&mut model.items, &keep);
        keep_rows(&mut self.layer_ids, &keep);
        keep_rows(&mut self.cut_ids, &keep);
    }

    /// カットの ID の Timeline での位置（レイヤーの添字、カットの添字）。
    fn position_of_cut(&self, id: &str) -> Option<(usize, usize)> {
        self.cut_ids
            .iter()
            .enumerate()
            .find_map(|(item, cuts)| cuts.iter().position(|c| c == id).map(|cut| (item, cut)))
    }
}

/// `keep` が真の行だけを残す。`rows` と `keep` は同じ並び。
fn keep_rows<T>(rows: &mut Vec<T>, keep: &[bool]) {
    let mut keep = keep.iter();
    rows.retain(|_| keep.next().copied().unwrap_or(true));
}

/// ポインターの入力を UI に渡し、起きた操作を送る。選択は共有状態へ、シークは再生スレッドへ、
/// 端のつまみと中のドラッグの確定は編集の操作として適用する。
/// 1 周に溜まったシークは最後の 1 つだけ送る（途中の位置を描いても次に上書きされる）。
pub(super) fn handle_input(shared: &Arc<Shared>, ui: &mut Ui, doc: &DocSync) {
    let events = std::mem::take(&mut *shared.input.lock().expect("input"));
    let mut seek = None;
    for ev in events {
        if let PointerEvent::Down { x, y, .. } = ev {
            focus_viewer(shared, ui.region_at(x, y));
        }
        for action in ui.pointer(ev) {
            match action {
                Action::Seek(t) => seek = Some(t),
                Action::Select(index) => {
                    shared.select_layer(index.and_then(|i| doc.layer_ids.get(i).cloned()));
                }
                Action::SelectCut(at) => {
                    let id = at.and_then(|(i, c)| doc.cut_ids.get(i)?.get(c).cloned());
                    shared.edit.select_cut(id);
                }
                Action::Edit {
                    kind,
                    by,
                    done: true,
                } => apply_gesture(shared, doc, kind, by),
                Action::Edit { .. } => {}
                Action::HeaderButton { index, button } => {
                    apply_header_button(shared, doc, index, button);
                }
                Action::MoveLayer { from, slot } => apply_move_layer(shared, doc, from, slot),
            }
        }
    }
    if let Some(t) = seek {
        // スクラブは再生を止めて位置を追う。
        if shared.playing.load(Ordering::SeqCst) {
            shared.send(Command::Pause);
        }
        shared.send(Command::Seek(t));
    }
}

/// クリックした領域に、再生のキーの向き先を合わせる。Source の領域なら Source、それ以外は Program。
fn focus_viewer(shared: &Shared, region: Region) {
    match region {
        Region::Source => shared.source.set_active(true),
        Region::Program | Region::Timeline => shared.source.set_active(false),
        Region::Outside => {}
    }
}

/// ドラッグの確定を、Timeline に出しているデータで操作にし、そのバージョンを付けて編集のキューで適用する。
/// 描画を止めないよう、適用と結果の待ちはキューのスレッドで行う。
fn apply_gesture(shared: &Arc<Shared>, doc: &DocSync, kind: EditKind, by: f64) {
    let Some(shown) = &doc.shown else { return };
    let Ok(comp) = shown.project.composition(&shown.comp) else {
        return;
    };
    let ids = TimelineIds {
        layers: &doc.layer_ids,
        cuts: &doc.cut_ids,
    };
    let Some(op) = gesture_op(kind, by, &ids, comp) else {
        return;
    };
    queue_op(shared, shown.version, op);
}

/// 見出しのボタンを、Timeline に出しているデータのレイヤーの今の状態から操作にして適用する。
fn apply_header_button(shared: &Arc<Shared>, doc: &DocSync, index: usize, button: HeaderButton) {
    let Some(shown) = &doc.shown else { return };
    let layer = doc.layer_ids.get(index).and_then(|id| {
        let (_, layer) = shown.project.layer(&Id::new(id.as_str())).ok()?;
        Some(layer)
    });
    if let Some(layer) = layer {
        queue_op(shared, shown.version, header_button_op(button, layer));
    }
}

/// 見出しのドラッグの確定を `moveLayer` にして適用する。
fn apply_move_layer(shared: &Arc<Shared>, doc: &DocSync, from: usize, slot: usize) {
    let Some(shown) = &doc.shown else { return };
    let Ok(comp) = shown.project.composition(&shown.comp) else {
        return;
    };
    let ids = TimelineIds {
        layers: &doc.layer_ids,
        cuts: &doc.cut_ids,
    };
    if let Some(op) = move_layer_op(from, slot, &ids, comp) {
        queue_op(shared, shown.version, op);
    }
}

/// 操作を、Timeline に出しているデータのバージョンを付けて編集のキューで適用する。
fn queue_op(shared: &Arc<Shared>, base: Option<i64>, op: Op) {
    let s = shared.clone();
    crate::win::edit::queue(shared, move || {
        let result = crate::win::edit::submit_based(&s, vec![op], base);
        s.edit.set_message(result.err().unwrap_or_default());
    });
}
