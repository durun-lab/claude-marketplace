//! `--play-test` のツールの通し。Blade（ホバーの線、クリックで split、取り消し）、Selection（レイヤーを時間の上で動かす、
//! カットの中のドラッグは何も変えない、取り消し）、Ctrl+B の Split Clip と取り消しを、実際のキーとウィンドウへの入力で行う。

use mm_ui_layout::Tool;

use super::*;

/// レイヤーを動かすドラッグの量（秒）。帯が動いたのが画面で分かる長さにする。
const MOVE_SECS: f64 = 1.0;
/// 動かした量の許容（秒）。近くのスナップの目標に吸い付いても通す。
const MOVE_TOLERANCE: f64 = 0.5;
/// カットの中をドラッグする量（論理ピクセル）。
const BODY_DRAG_PX: f32 = 40.0;
/// 割るカットに求める長さ（秒）。中央をクリックしても端に掛からない長さ。
const MIN_CUT_SECS: f64 = 0.5;

/// ツールのキーを押し、描画スレッドが今のツールを共有状態に置くまで待つ。
pub(super) fn select(env: &Env<'_>, tool: Tool) -> bool {
    let key = match tool {
        Tool::Selection => "KeyA",
        Tool::Trim => "KeyT",
        Tool::Blade => "KeyB",
    };
    keys::dispatch(env.shared, "play-test", key);
    wait_until(WAIT, || *env.shared.tool.lock().expect("tool") == tool)
}

/// 行 `row` の、隙間でなく `MIN_CUT_SECS` より長い最初のカットの範囲（秒）。
fn splittable_span(shared: &Shared, row: usize) -> Option<(f64, f64)> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    let model = timeline_model(&doc.project, comp);
    let cuts = &model.items.get(row)?.cuts;
    let cut = cuts
        .iter()
        .find(|c| !c.gap && c.end - c.start > MIN_CUT_SECS)?;
    Some((cut.start, cut.end))
}

/// 行 `row` に、`from` より後ろで `at` の近く（`tol` 秒以内）に始まるカットがあるか。
fn cut_starts_near(shared: &Shared, row: usize, (from, at): (f64, f64), tol: f64) -> bool {
    let Some(doc) = shared.doc() else {
        return false;
    };
    let Ok(comp) = doc.project.composition(&doc.comp) else {
        return false;
    };
    let model = timeline_model(&doc.project, comp);
    model.items.get(row).is_some_and(|it| {
        let near = |c: &&mm_ui_layout::CutBar| c.start > from + 1e-6 && (c.start - at).abs() <= tol;
        it.cuts.iter().filter(near).any(|c| !c.gap)
    })
}

/// 取り消し 1 回で `cuts` の数が `before` に戻ることを確かめる。
fn undo_to(env: &Env<'_>, log: &mut Log, name: &str, before: usize) {
    keys::dispatch(env.shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || cuts(env.shared).len() == before);
    log.step(name, undone, json!({ "cuts_before": before }));
}

/// Blade を選び、カットの上のポインターに割る時刻の線が出た画面を撮り、クリックで割れて取り消しで戻ることを確かめる。
fn blade_steps(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let found = geometry(shared, hwnd).and_then(|g| Some((splittable_span(shared, g.row)?, g)));
    let Some(((start, end), g)) = found else {
        log.step("blade_split", false, json!("割れるカットの位置を引けない"));
        return;
    };
    let mid = (start + end) / 2.0;
    let at = g.at(g.row, mid, 0.0);
    let chosen = select(env, Tool::Blade);
    surface::post_move(hwnd, at);
    shot(env, log, "19-blade-hover");
    let before = cuts(shared).len();
    surface::post_click(hwnd, at.0, at.1);
    let split = wait_until(WAIT, || cuts(shared).len() == before + 1);
    // 再生位置への吸い付き（8 論理ピクセル）の分だけ中央からずれうる。
    let tol = 8.0 / g.place.px_per_sec() + 1.0 / 24.0;
    let placed = cut_starts_near(shared, g.row, (start, mid), tol);
    log.step(
        "blade_split",
        chosen && split && placed,
        json!({ "cuts_before": before, "cuts_after": cuts(shared).len(), "at": mid }),
    );
    shot(env, log, "20-blade-split");
    if split {
        undo_to(env, log, "undo_blade", before);
    }
    select(env, Tool::Selection);
}

/// Selection のカットの中のドラッグは、カットを選ぶだけで中身を動かさない。
fn selection_cut_step(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let found = geometry(shared, hwnd).and_then(|g| Some((splittable_span(shared, g.row)?, g)));
    let Some(((start, end), g)) = found else {
        log.step("selection_cut", false, json!("カットの位置を引けない"));
        return;
    };
    let points = |s: &Shared| -> Vec<(f64, f64)> {
        let cuts = cuts(s);
        cuts.iter()
            .map(|c| (c.in_point.to_f64(), c.out_point.to_f64()))
            .collect()
    };
    let before = points(shared);
    let mid = (start + end) / 2.0;
    surface::post_drag(hwnd, g.at(g.row, mid, 0.0), g.at(g.row, mid, BODY_DRAG_PX));
    std::thread::sleep(SETTLE);
    let kept = points(shared) == before;
    log.step("selection_cut", kept, json!({ "cuts": before.len() }));
}

fn layer_start(shared: &Shared, id: &str) -> Option<f64> {
    let doc = shared.doc()?;
    let (_, layer) = doc.project.layer(&Id::new(id)).ok()?;
    Some(layer.range().ok()?.start.to_f64())
}

/// Selection でレイヤーの帯の中を引いて、離す前（帯だけが動く）と離した後（shiftLayer）を撮り、取り消し 1 回で戻す。
fn move_steps(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let found = geometry(shared, hwnd).and_then(|g| {
        let (row, id, end) = trim_target(shared, &g.place)?;
        Some((g, row, layer_start(shared, &id)?, (id, end)))
    });
    let Some((g, row, start, (id, end))) = found else {
        log.step("move_layer", false, json!("レイヤーの位置を引けない"));
        return;
    };
    let mid = (start + end) / 2.0;
    let (from, to) = (g.at(row, mid, 0.0), g.at(row, mid + MOVE_SECS, 0.0));
    surface::post_press(hwnd, from, to);
    std::thread::sleep(SETTLE);
    let held = layer_start(shared, &id) == Some(start);
    shot(env, log, "21-move-dragging");
    surface::post_release(hwnd, to);
    let moved = wait_until(WAIT, || {
        layer_start(shared, &id).is_some_and(|s| (s - start).abs() > 1e-6)
    });
    let after = layer_start(shared, &id);
    let near = after.is_some_and(|s| (s - start - MOVE_SECS).abs() <= MOVE_TOLERANCE);
    log.step(
        "move_layer",
        held && moved && near,
        json!({ "layer": id, "start_before": start, "start_after": after, "unchanged_while_dragging": held }),
    );
    shot(env, log, "22-move-layer");
    if !moved {
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || layer_start(shared, &id) == Some(start));
    log.step("undo_move", undone, json!({}));
}

/// Ctrl+B（Split Clip）は再生位置でカットを割り、取り消し 1 回で戻る。
fn split_clip_step(env: &Env<'_>, log: &mut Log) {
    let shared = env.shared;
    let found = geometry(shared, env.surface_hwnd).and_then(|g| splittable_span(shared, g.row));
    let Some((start, end)) = found else {
        log.step("split_clip", false, json!("割れるカットの位置を引けない"));
        return;
    };
    let before = cuts(shared).len();
    seek_main(shared, (start + end) / 2.0);
    keys::dispatch(shared, "play-test", "Ctrl+KeyB");
    let split = wait_until(WAIT, || cuts(shared).len() == before + 1);
    log.step("split_clip", split, json!({ "cuts_before": before }));
    shot(env, log, "23-split-clip");
    if split {
        undo_to(env, log, "undo_split_clip", before);
    }
}

pub(crate) fn run(env: &Env<'_>, log: &mut Log) {
    blade_steps(env, log);
    selection_cut_step(env, log);
    move_steps(env, log);
    split_clip_step(env, log);
}

/// ツールの通しで押す帯（カットのある行と、シーケンスでないレイヤーの行）が、どちらも行の領域に見えているか。
pub(crate) fn targets_visible(env: &Env<'_>) -> bool {
    let shared = env.shared;
    geometry(shared, env.surface_hwnd).is_some_and(|g| trim_target(shared, &g.place).is_some())
}
