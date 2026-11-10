//! Timeline のキーフレームの行の操作を、編集のキューで適用する操作と、Inspector と Graph で選ぶパラメータにする。
//! 操作は、人が見ていた Timeline のデータ（`DocSync::shown`）とそのバージョンで作る（ほかのドラッグの編集と同じ）。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mm_ops::Op;
use mm_ui_layout::{KeyframeAction, RowPick};

use super::{queue_op, DocSync};
use crate::win::keyframe_edit::{self, RowSelection};
use crate::win::state::Shared;

/// キーフレームの行の操作を 1 つ受ける。行の開閉は `Ui` が表示の状態に当てたので、ここでは何もしない。
pub(super) fn apply(shared: &Arc<Shared>, doc: &DocSync, action: KeyframeAction) {
    match action {
        KeyframeAction::ToggleRows(_) => {}
        KeyframeAction::Select(pick) => select(shared, doc, pick),
        _ => {
            if let (Some(shown), Some(op)) = (&doc.shown, op_of(doc, action)) {
                queue_op(shared, shown.version, op);
            }
        }
    }
}

/// 動かす、外す、打つの操作。
fn op_of(doc: &DocSync, action: KeyframeAction) -> Option<Op> {
    let shown = doc.shown.as_ref()?;
    let (project, comp) = (&shown.project, shown.project.composition(&shown.comp).ok()?);
    let layer = |item: usize| doc.layer_ids.get(item).map(String::as_str);
    match action {
        KeyframeAction::Move { keyframe, to } => keyframe_edit::move_op(
            project,
            comp,
            layer(keyframe.item)?,
            (keyframe.row, keyframe.keyframe),
            to,
        ),
        KeyframeAction::Remove(keyframe) => keyframe_edit::remove_op(
            project,
            comp,
            layer(keyframe.item)?,
            (keyframe.row, keyframe.keyframe),
        ),
        KeyframeAction::Add { item, row, time } => {
            keyframe_edit::add_op(project, comp, layer(item)?, row, time)
        }
        KeyframeAction::ToggleRows(_) | KeyframeAction::Select(_) => None,
    }
}

/// 選んだ行を置き、選択のイベントを送り直させる（Inspector と Graph がそのパラメータを選ぶ）。
/// 選んだら共有のカットの選択も外す。Delete は押せる最初のコマンド（Lift、Remove Keyframe の順）を実行するので、
/// 取り消しで消えたカットの ID が残っていると、キーフレームを選んでいても Lift が先に押せてしまう。
fn select(shared: &Shared, doc: &DocSync, pick: Option<RowPick>) {
    if pick.is_some() {
        shared.edit.select_cut(None);
    }
    let selection = pick.and_then(|p| {
        let shown = doc.shown.as_ref()?;
        let comp = shown.project.composition(&shown.comp).ok()?;
        let layer = doc.layer_ids.get(p.item)?;
        Some(RowSelection {
            path: keyframe_edit::row_path(&shown.project, comp, layer, p.row),
            layer: layer.clone(),
            keyframe: p.keyframe.is_some(),
        })
    });
    shared.keyframes.select(selection);
    shared.selection_serial.fetch_add(1, Ordering::SeqCst);
}
