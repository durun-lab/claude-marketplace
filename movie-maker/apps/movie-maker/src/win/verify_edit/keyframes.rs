//! `--play-test` のキーフレームの行の通し。実際のウィンドウへのクリック、ドラッグ、キーで、見出しのキーフレームの開閉で
//! 行を開き、キーフレームを押して選び（Inspector と Graph のパラメータも選ぶ）、ドラッグを Esc で取りやめ、離して 1 回の
//! `moveKeyframe` にして取り消し 1 回で戻し、Delete と右クリックのメニューの Remove Keyframe で外して戻し、Alt+クリックで打って
//! 戻し、レイヤーの帯を引いて行のひし形が帯と一緒に動く所を撮って取りやめ、] と [ で前後のキーフレームへ動かし、U で閉じて
//! 開く。手順ごとにスクリーンショットを残す。

use mm_ui_layout::{HitKind, Mods, PointerEvent, TimelineModel};
use mm_ui_wgpu::timeline_layer_ids;
use serde_json::json;

use super::*;
use crate::win::{context_menu, menu};

/// キーフレームを動かす量（秒）。
const MOVE_SECS: f64 = 0.5;
/// Inspector と Graph が選択のイベントを受けて描き直すのを待つ時間。
const WEBVIEW_SETTLE: Duration = Duration::from_millis(700);
/// 再生位置がキーフレームの時刻に着いたとみなす差（秒）。
const AT_EPS: f64 = 1e-3;
/// 行を開いたままレイヤーの帯を引く量（秒）。
const SHIFT_SECS: f64 = 1.0;
/// 帯を掴むのに求める、見えている帯の幅（論理ピクセル）。中央が端のつまみに掛からない幅。
const GRAB_W: f32 = 60.0;
/// ひし形を掴んで動かす縮尺（1 秒の論理ピクセル）。0.5 秒の移動が 30 ピクセルになり、隣のキーフレームの当たりに掛からない。
const MIN_PX_PER_SEC: f64 = 60.0;

/// 行を開いたレイヤー（Timeline の行の添字と ID）。
struct OpenedLayer {
    index: usize,
    id: String,
}

fn model_of(shared: &Shared) -> Option<(TimelineModel, Vec<String>)> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    Some((timeline_model(&doc.project, comp), timeline_layer_ids(comp)))
}

/// レイヤー `id` の最初のキーフレームの行のキーフレームの時刻。
fn row_times(shared: &Shared, id: &str) -> Vec<f64> {
    let Some((model, ids)) = model_of(shared) else {
        return Vec::new();
    };
    let item = ids
        .iter()
        .position(|i| i == id)
        .and_then(|i| model.items.get(i));
    let row = item.and_then(|it| it.keyframe_rows.first());
    row.map(|r| r.keyframes.iter().map(|k| k.time).collect())
        .unwrap_or_default()
}

fn is_open(shared: &Shared, id: &str) -> bool {
    let state = shared.view.current();
    state.is_some_and(|s| s.heights.keyframe_rows_open.contains(id))
}

/// 最初のキーフレームの行の、時刻 `t` から `dx` ずらした点（クライアントの物理ピクセル）。ひし形が見えていなければ `None`。
fn diamond_point(env: &Env<'_>, layer: &OpenedLayer, time: f64, dx: f32) -> Option<(i32, i32)> {
    let (model, _) = model_of(env.shared)?;
    let place = placement(env.shared, env.surface_hwnd, &model)?;
    place
        .keyframe_visible((layer.index, 0), time)
        .then(|| place.keyframe_point((layer.index, 0), time, dx))
}

/// 表示の状態の縦のスクロール。
fn scroll_y(shared: &Shared) -> f32 {
    shared.view.current().map_or(0.0, |s| s.scroll_y)
}

/// 表示の状態の 1 秒の論理ピクセル。
fn px_per_sec(shared: &Shared) -> f64 {
    shared.view.current().map_or(0.0, |s| s.time.px_per_sec)
}

/// クライアントの点 `at` に、サーフェスが積むのと同じホイールの入力を `dy` 目盛り積む（`ctrl` で拡大縮小、`shift` で縦の
/// スクロール）。修飾キーの状態はメッセージで送れないため。
fn send_wheel(shared: &Shared, at: (i32, i32), dy: f32, (ctrl, shift): (bool, bool)) {
    let wheel = PointerEvent::Wheel {
        x: at.0 as f32,
        y: at.1 as f32,
        dy,
        ctrl,
        shift,
    };
    shared.input.lock().expect("input").push(wheel);
}

/// 行の領域の中央に Shift+ホイールを `dy` 目盛り送り、縦のスクロールが変わるのを待って、変わった量を返す。
fn wheel_rows(env: &Env<'_>, place: &Placement, dy: f32) -> f32 {
    let before = scroll_y(env.shared);
    send_wheel(env.shared, place.row_area_center(), dy, (false, true));
    wait_until(WAIT, || scroll_y(env.shared) != before);
    scroll_y(env.shared) - before
}

/// 縮めた表示（長いプロジェクトの全体表示は 1 秒が 2 論理ピクセルに満たず、0.5 秒の移動が 1 ピクセルに届かない）なら、最初の
/// キーフレームの所を中心に Ctrl+ホイールで `MIN_PX_PER_SEC` まで拡大する。1 目盛りの倍率を測り、残りを 1 回で送る。
fn zoom_to_keyframes(env: &Env<'_>, layer: &OpenedLayer) {
    let shared = env.shared;
    let first = row_times(shared, &layer.id).first().copied();
    let at = first.and_then(|t| diamond_point(env, layer, t, 0.0));
    let Some(at) = at.filter(|_| px_per_sec(shared) < MIN_PX_PER_SEC) else {
        return;
    };
    let zoom = |dy: f32| {
        let before = px_per_sec(shared);
        send_wheel(shared, at, dy, (true, false));
        wait_until(WAIT, || px_per_sec(shared) != before);
        px_per_sec(shared) / before
    };
    let step = zoom(1.0);
    let left = (MIN_PX_PER_SEC / px_per_sec(shared)).ln() / step.ln();
    if step > 1.0 && left > 0.0 {
        zoom(left.ceil() as f32);
    }
}

/// キーフレームの行のあるレイヤーが見えていなければ、見えている所にいちばん近いものの上端まで縦にスクロールする
/// （レイヤーの多いプロジェクトでは、開いたときの表示に入らない）。1 目盛りで動く量を測り、残りを 1 回のホイールで送る。
fn reveal_keyed(env: &Env<'_>, model: &TimelineModel) {
    let keyed = |i: usize| !model.items[i].keyframe_rows.is_empty();
    let Some(place) = placement(env.shared, env.surface_hwnd, model) else {
        return;
    };
    if place.first_visible(keyed).is_some() {
        return;
    }
    let vp = place.viewport();
    let near = |a: &f32, b: &f32| (a - vp.scroll_y).abs().total_cmp(&(b - vp.scroll_y).abs());
    let tops = (0..model.items.len()).filter(|i| keyed(*i));
    let Some(top) = tops.map(|i| vp.rows.top(i)).min_by(near) else {
        return;
    };
    let step = wheel_rows(env, &place, if top < vp.scroll_y { 1.0 } else { -1.0 }).abs();
    if step > 0.0 {
        let left = top - scroll_y(env.shared);
        wheel_rows(env, &place, -left / step);
    }
}

/// 見えている行のうちキーフレームの行を持つ最初のレイヤーの、見出しのキーフレームの開閉を押して行を開く（見えていなければ
/// 先にスクロールする）。再生テストは `.mm/view.json` を読まないので、行は閉じた状態から始まる。キーフレームのあるレイヤーが
/// 無いプロジェクトでは、キーフレームの行の手順をスキップする。
fn open_step(env: &Env<'_>, log: &mut Log) -> Option<OpenedLayer> {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let model = model_of(shared);
    let keyed = |m: &TimelineModel| m.items.iter().any(|it| !it.keyframe_rows.is_empty());
    if model.as_ref().is_some_and(|(m, _)| !keyed(m)) {
        log.skip("keyframe_rows_open", "キーフレームのあるレイヤーが無い");
        return None;
    }
    let found = model.and_then(|(model, ids)| {
        reveal_keyed(env, &model);
        let place = placement(shared, hwnd, &model)?;
        let index = place.first_visible(|i| !model.items[i].keyframe_rows.is_empty())?;
        let at = place.disclosure(&model.items[index], index);
        Some((index, ids.get(index)?.clone(), at))
    });
    // スクロールしても行の領域に見えなければ、ツールの手順と同じく行わずにスキップする。
    let Some((index, id, at)) = found else {
        let why = "キーフレームの行のあるレイヤーが行の領域に見えない";
        log.skip("keyframe_rows_open", why);
        return None;
    };
    surface::post_click(hwnd, at.0, at.1);
    let opened = wait_until(WAIT, || is_open(shared, &id));
    log.step("keyframe_rows_open", opened, json!({ "layer": id }));
    shot(env, log, "24-keyframe-rows-open");
    opened.then_some(OpenedLayer { index, id })
}

/// 最初のキーフレームの行の最初のキーフレームを押し、レイヤーとキーフレームと、Inspector と Graph のパラメータを選ぶ。
fn select_step(env: &Env<'_>, log: &mut Log, layer: &OpenedLayer) -> Option<f64> {
    let shared = env.shared;
    let time = *row_times(shared, &layer.id).first()?;
    let Some(at) = diamond_point(env, layer, time, 0.0) else {
        log.step("keyframe_select", false, json!("ひし形が見えていない"));
        return None;
    };
    shared.keyframes.select(None);
    surface::post_click(env.surface_hwnd, at.0, at.1);
    let selected = wait_until(WAIT, || shared.keyframes.keyframe_selected());
    let chosen = shared.selected_layer();
    let param = shared
        .keyframes
        .param_for(chosen.as_deref())
        .map(|p| p.path);
    let ok = selected && chosen.as_deref() == Some(layer.id.as_str()) && param.is_some();
    log.step(
        "keyframe_select",
        ok,
        json!({ "layer": chosen, "param": param, "time": time }),
    );
    std::thread::sleep(WEBVIEW_SETTLE);
    shot(env, log, "25-keyframe-selected");
    ok.then_some(time)
}

/// キーフレームを掴んで動かしたまま撮り、Esc で取りやめ、もう一度動かして離し、取り消し 1 回で戻す。
fn drag_step(env: &Env<'_>, log: &mut Log, layer: &OpenedLayer, time: f64) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let before = row_times(shared, &layer.id);
    let (Some(from), Some(to)) = (
        diamond_point(env, layer, time, 0.0),
        diamond_point(env, layer, time + MOVE_SECS, 0.0),
    ) else {
        log.step("keyframe_drag", false, json!("ひし形の位置を求められない"));
        return;
    };
    surface::post_press(hwnd, from, to);
    shot(env, log, "26-keyframe-dragging");
    cancel_drag(hwnd, from);
    let kept = row_times(shared, &layer.id) == before;
    log.step("keyframe_drag_cancel", kept, json!({ "times": before }));
    surface::post_drag(hwnd, from, to);
    let moved = wait_until(WAIT, || row_times(shared, &layer.id) != before);
    let after = row_times(shared, &layer.id);
    log.step(
        "keyframe_drag",
        moved,
        json!({ "before": before, "after": after }),
    );
    shot(env, log, "27-keyframe-moved");
    if !moved {
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || row_times(shared, &layer.id) == before);
    log.step("undo_keyframe_drag", undone, json!({}));
}

/// キーフレームを押して選び、Delete で外して、取り消し 1 回で戻す。
fn delete_step(env: &Env<'_>, log: &mut Log, layer: &OpenedLayer, time: f64) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let before = row_times(shared, &layer.id);
    let Some(at) = diamond_point(env, layer, time, 0.0) else {
        log.step(
            "keyframe_delete",
            false,
            json!("ひし形の位置を求められない"),
        );
        return;
    };
    surface::post_click(hwnd, at.0, at.1);
    wait_until(WAIT, || shared.keyframes.keyframe_selected());
    surface::post_key(hwnd, VK_DELETE);
    let removed = wait_until(WAIT, || {
        row_times(shared, &layer.id).len() + 1 == before.len()
    });
    log.step(
        "keyframe_delete",
        removed,
        json!({ "before": before, "after": row_times(shared, &layer.id) }),
    );
    shot(env, log, "28-keyframe-deleted");
    if !removed {
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || row_times(shared, &layer.id) == before);
    log.step("undo_keyframe_delete", undone, json!({}));
}

/// ひし形を右クリックすると、そのひし形を選んだキーフレームのメニューが出て、Remove Keyframe で外れ、取り消し 1 回で戻る。
/// ポップアップはモーダルで止まるので出さず、並びの記録を読み、選んだ項目の ID をメニューと同じ道に流す。
fn context_step(env: &Env<'_>, log: &mut Log, layer: &OpenedLayer, time: f64) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let before = row_times(shared, &layer.id);
    let Some(at) = diamond_point(env, layer, time, 0.0) else {
        let why = json!("ひし形の位置を求められない");
        log.step("keyframe_context", false, why);
        return;
    };
    shared.context.set_headless(true);
    context_menu::post_at(hwnd, at);
    let shown = super::context::wait_shown(shared, HitKind::Keyframe);
    shared.context.set_headless(false);
    let selected = wait_until(WAIT, || shared.keyframes.keyframe_selected());
    let items = shown.map(|s| s.items);
    let first = items.as_ref().and_then(|i| i.first());
    let listed = first.is_some_and(|t| t.starts_with("Remove Keyframe"));
    log.step(
        "keyframe_context",
        selected && listed,
        json!({ "items": items }),
    );
    menu::run(shared, &context_menu::item_id("timeline.remove_keyframe"));
    let removed = wait_until(WAIT, || {
        row_times(shared, &layer.id).len() + 1 == before.len()
    });
    let after = row_times(shared, &layer.id);
    let detail = json!({ "before": before, "after": after });
    log.step("keyframe_context_remove", removed, detail);
    if removed {
        keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
        let undone = wait_until(WAIT, || row_times(shared, &layer.id) == before);
        log.step("undo_keyframe_context_remove", undone, json!({}));
    }
}

/// 最初の 2 つのキーフレームの間を Alt+クリックしてキーフレームを打ち、取り消し 1 回で戻す。修飾キーの状態はメッセージで
/// 送れないので、サーフェスが積むのと同じポインターの入力を入力のキューに積む。
fn add_step(env: &Env<'_>, log: &mut Log, layer: &OpenedLayer) {
    let shared = env.shared;
    let before = row_times(shared, &layer.id);
    let at = match before[..] {
        [a, b, ..] => diamond_point(env, layer, (a + b) / 2.0, 0.0),
        _ => None,
    };
    let Some((x, y)) = at else {
        log.step(
            "keyframe_add",
            false,
            json!("キーフレームの間の位置を求められない"),
        );
        return;
    };
    let (x, y) = (x as f32, y as f32);
    let alt = Mods {
        shift: false,
        alt: true,
    };
    let mut input = shared.input.lock().expect("input");
    input.extend([PointerEvent::Down { x, y, mods: alt }, PointerEvent::Up]);
    drop(input);
    let added = wait_until(WAIT, || {
        row_times(shared, &layer.id).len() == before.len() + 1
    });
    log.step(
        "keyframe_add",
        added,
        json!({ "before": before, "after": row_times(shared, &layer.id) }),
    );
    shot(env, log, "29-keyframe-added");
    if !added {
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || row_times(shared, &layer.id) == before);
    log.step("undo_keyframe_add", undone, json!({}));
}

/// 行を開いたレイヤーの始まり（秒）。
fn layer_start(shared: &Shared, layer: &OpenedLayer) -> Option<f64> {
    let (model, _) = model_of(shared)?;
    model.items.get(layer.index).map(|it| it.start)
}

/// 行を開いたまま Selection でレイヤーの帯の中を引き、帯と帯のマーカーと行のひし形が一緒に動く所を撮って、Esc で取りやめる。
/// シーケンスのレイヤー（帯の中はカット）と、掴める幅の帯が見えていないときはスキップする。
fn shift_layer_step(env: &Env<'_>, log: &mut Log, layer: &OpenedLayer) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let found = model_of(shared).and_then(|(model, _)| {
        let item = model
            .items
            .get(layer.index)
            .filter(|it| it.cuts.is_empty())?;
        let place = placement(shared, hwnd, &model)?;
        let from = place.bar_center(layer.index, (item.start, item.end), GRAB_W)?;
        let dx = (SHIFT_SECS * place.px_per_sec()) as f32 * place.scale.get();
        Some((from, (from.0 + dx.round() as i32, from.1), item.start))
    });
    let Some((from, to, start)) = found else {
        log.skip(
            "keyframe_rows_shift_layer",
            "引いて動かせる帯が見えていない",
        );
        return;
    };
    tools::select_tool(env, Tool::Selection);
    surface::post_press(hwnd, from, to);
    shot(env, log, "30-keyframe-rows-shift-layer");
    cancel_drag(hwnd, from);
    let kept = layer_start(shared, layer) == Some(start);
    log.step("keyframe_rows_shift_layer", kept, json!({ "start": start }));
}

/// 先頭から ] を 2 回と [ を 1 回押し、選んだレイヤーの 1 つ目、2 つ目、1 つ目のキーフレームへ動くことを確かめる。
fn navigate_step(env: &Env<'_>, log: &mut Log, layer: &OpenedLayer) {
    let shared = env.shared;
    let Some((model, _)) = model_of(shared).filter(|(m, _)| m.fps > 0.0) else {
        log.step("keyframe_navigate", false, json!("フレームレートが無い"));
        return;
    };
    let fps = model.fps;
    let times = model.items.get(layer.index).map(|it| it.keyframes.to_vec());
    let mut stops: Vec<f64> = times
        .unwrap_or_default()
        .into_iter()
        .map(|k| mm_ui_layout::quantize(k, fps))
        .filter(|k| *k > 0.5 / fps)
        .collect();
    stops.dedup_by(|a, b| (*a - *b).abs() < AT_EPS);
    let [first, second, ..] = stops[..] else {
        log.step("keyframe_navigate", false, json!({ "stops": stops }));
        return;
    };
    seek_main(shared, 0.0);
    let mut reached = Vec::new();
    for (key, want) in [("]", first), ("]", second), ("[", first)] {
        keys::dispatch(shared, "play-test", key);
        let ok = wait_until(WAIT, || (shared.playhead() - want).abs() < AT_EPS);
        reached.push(json!({ "key": key, "want": want, "at": shared.playhead(), "ok": ok }));
    }
    let ok = reached.iter().all(|r| r["ok"] == true);
    log.step("keyframe_navigate", ok, json!(reached));
}

/// 選んだレイヤーの行を U で閉じ、もう一度 U で開く。
fn toggle_key_step(env: &Env<'_>, log: &mut Log, layer: &OpenedLayer) {
    let shared = env.shared;
    keys::dispatch(shared, "play-test", "KeyU");
    let closed = wait_until(WAIT, || !is_open(shared, &layer.id));
    keys::dispatch(shared, "play-test", "KeyU");
    let reopened = wait_until(WAIT, || is_open(shared, &layer.id));
    log.step(
        "show_keyframe_editor",
        closed && reopened,
        json!({ "closed": closed, "opened": reopened }),
    );
}

/// キーフレームの行の通し。
pub(super) fn run(env: &Env<'_>, log: &mut Log) {
    let Some(layer) = open_step(env, log) else {
        return;
    };
    zoom_to_keyframes(env, &layer);
    if let Some(time) = select_step(env, log, &layer) {
        drag_step(env, log, &layer, time);
        delete_step(env, log, &layer, time);
        context_step(env, log, &layer, time);
    }
    add_step(env, log, &layer);
    shift_layer_step(env, log, &layer);
    navigate_step(env, log, &layer);
    toggle_key_step(env, log, &layer);
    shot(env, log, "31-keyframe-rows");
}
