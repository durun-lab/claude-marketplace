//! Blade のクリックを、Timeline に出しているデータの上の `split` にして適用する。ポインターの時刻と、クリックしたレイヤーを
//! 計画に渡し（再生位置のキーの Split Clip と同じ計画）、割った後ろの半分を選ぶ。

use std::sync::Arc;

use super::sync::DocSync;
use crate::win::edit;
use crate::win::sequence_edit::{plan_split, Ctx};
use crate::win::state::Shared;

/// レイヤー `item` の時刻 `time`（秒）にあるカットを割る。計画に使ったデータのバージョンを付けて編集のキューで適用し、
/// 失敗（カットの端、隙間）は文言で返す。
pub(super) fn apply_split(shared: &Arc<Shared>, doc: &DocSync, item: usize, time: f64) {
    let (Some(shown), Some(layer)) = (&doc.shown, doc.layer_ids.get(item)) else {
        return;
    };
    let (shown, layer) = (shown.clone(), layer.clone());
    let s = shared.clone();
    edit::queue(shared, move || {
        let result = split_at(&s, &shown, &layer, time);
        s.edit.set_message(result.err().unwrap_or_default());
    });
}

fn split_at(
    shared: &Shared,
    shown: &crate::win::state::OpenDoc,
    layer: &str,
    time: f64,
) -> Result<(), String> {
    let comp = shown
        .project
        .composition(&shown.comp)
        .map_err(|e| e.to_string())?;
    let ctx = Ctx {
        project: &shown.project,
        comp,
        layer: Some(layer),
        cut: None,
        playhead: time,
    };
    let planned = plan_split(&ctx, shared.edit.fresh_cut_id(&shown.project))?;
    edit::submit_based(shared, planned.ops, shown.version)?;
    shared
        .edit
        .select_cut(planned.select.map(|id| id.to_string()));
    Ok(())
}
