//! `--play-test` のレイヤーの端のつまみの通し。アウト点のつまみを引いたまま Esc で取りやめ、離して trimLayer、取り消し 1 回で戻す。

use super::*;

/// 行の領域に見えている、シーケンスでない最初のレイヤーの、Timeline の行の添字、ID、終わり（秒）。
pub(super) fn trim_target(shared: &Shared, place: &Placement) -> Option<(usize, String, f64)> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    let ids = mm_ui_wgpu::timeline_layer_ids(comp);
    let model = timeline_model(&doc.project, comp);
    let row = place.first_visible(|i| model.items[i].cuts.is_empty())?;
    Some((row, ids.get(row)?.clone(), model.items[row].end))
}

fn layer_out(shared: &Shared, id: &str) -> Option<f64> {
    let doc = shared.doc()?;
    let (_, layer) = doc.project.layer(&Id::new(id)).ok()?;
    Some(layer.out_point.to_f64())
}

/// レイヤーのアウト点のつまみを引くドラッグ。レイヤーの ID、掴む所、引いた先（サーフェスの座標）。
struct TrimDrag {
    id: String,
    from: (i32, i32),
    to: (i32, i32),
}

fn trim_drag(shared: &Shared, hwnd: isize) -> Option<TrimDrag> {
    let g = geometry(shared, hwnd)?;
    let (row, id, end) = trim_target(shared, &g.place)?;
    Some(TrimDrag {
        id,
        from: g.at(row, end, -HANDLE_INSET),
        to: g.at(row, end - TRIM_SECS, -HANDLE_INSET),
    })
}

/// アウト点のつまみを引いたまま Esc を押して離し、データが変わらないことを確かめる。
fn trim_cancel_step(env: &Env<'_>, log: &mut Log, drag: &TrimDrag) {
    let before = layer_out(env.shared, &drag.id);
    surface::post_press(env.surface_hwnd, drag.from, drag.to);
    std::thread::sleep(SETTLE);
    surface::post_escape(env.surface_hwnd);
    surface::post_release(env.surface_hwnd, drag.to);
    std::thread::sleep(SETTLE);
    let kept = layer_out(env.shared, &drag.id) == before;
    log.step("trim_cancel", kept, json!({ "out_before": before }));
}

/// レイヤーのアウト点のつまみを引いて、離す前（帯だけが動く）と離した後（trimLayer）を撮り、取り消し 1 回で戻す。
pub(super) fn trim_step(env: &Env<'_>, log: &mut Log) {
    let shared = env.shared;
    let Some(drag) = trim_drag(shared, env.surface_hwnd) else {
        log.step("trim_drag", false, json!("レイヤーの位置を引けない"));
        return;
    };
    trim_cancel_step(env, log, &drag);
    let TrimDrag { id, from, to } = drag;
    let before = layer_out(shared, &id);
    surface::post_press(env.surface_hwnd, from, to);
    std::thread::sleep(SETTLE);
    let held = layer_out(shared, &id) == before;
    shot(env, log, "8-trim-dragging");
    surface::post_release(env.surface_hwnd, to);
    let trimmed = wait_until(WAIT, || layer_out(shared, &id) != before);
    let after = layer_out(shared, &id);
    log.step(
        "trim_drag",
        held && trimmed,
        json!({ "layer": id, "out_before": before, "out_after": after, "unchanged_while_dragging": held }),
    );
    shot(env, log, "9-trim");
    if !trimmed {
        log.step(
            "undo_trim",
            false,
            json!("trimLayer が適用されなかったので取り消さない"),
        );
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || layer_out(shared, &id) == before);
    log.step("undo_trim", undone, json!({}));
}
