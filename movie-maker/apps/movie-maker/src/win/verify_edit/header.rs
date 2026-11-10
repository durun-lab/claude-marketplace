//! `--play-test` の見出しの通し。実際のウィンドウへのクリックとドラッグで、有効、ロック、Mute のボタン（取り消し 1 回で戻る）、
//! 見出しのドラッグ（Esc での取りやめと、離したときの moveLayer）、Shy で行を隠すことを確かめ、手順ごとにスクリーンショットを残す。

use mm_doc::Layer;
use mm_ui_layout::{HeaderButton, TimelineModel};
use mm_ui_wgpu::{timeline_layer_ids, timeline_model};
use serde_json::json;

use super::*;
use crate::win::commands;

/// Timeline の行の添字とレイヤーの ID。
struct Row {
    index: usize,
    id: String,
}

fn layer_of(shared: &Shared, id: &str) -> Option<Layer> {
    let doc = shared.doc()?;
    let (_, layer) = doc.project.layer(&Id::new(id)).ok()?;
    Some(layer.clone())
}

/// コンポジションのレイヤーの ID（下から）。
fn order(shared: &Shared) -> Vec<String> {
    let comp = shared.doc().and_then(|d| {
        let comp = d.project.composition(&d.comp).ok()?;
        Some(comp.layers.iter().map(|l| l.id.to_string()).collect())
    });
    comp.unwrap_or_default()
}

fn model_of(shared: &Shared) -> Option<(TimelineModel, Vec<String>)> {
    let doc = shared.doc()?;
    let comp = doc.project.composition(&doc.comp).ok()?;
    Some((timeline_model(&doc.project, comp), timeline_layer_ids(comp)))
}

/// 見えている行のうち `want` に合う最初の行。
fn pick(
    place: &Placement,
    model: &TimelineModel,
    ids: &[String],
    want: impl Fn(&mm_ui_layout::Item) -> bool,
) -> Option<Row> {
    let index = place.first_visible(|i| want(&model.items[i]))?;
    Some(Row {
        index,
        id: ids.get(index)?.clone(),
    })
}

/// ボタン `button` を押し、`changed` が真になるのを待って撮り、取り消し 1 回で元に戻ることを確かめる。
/// ボタンを持つレイヤーが無いプロジェクト（音を持てるレイヤーの無いプロジェクトの Mute）ではスキップする。
fn button_step(
    env: &Env<'_>,
    log: &mut Log,
    (name, button): (&str, HeaderButton),
    changed: impl Fn(&Layer) -> bool,
) {
    let shared = env.shared;
    let model = model_of(shared);
    if model
        .as_ref()
        .is_some_and(|(m, _)| !m.items.iter().any(|it| button_exists(it, button)))
    {
        log.skip(name, "このボタンを持つレイヤーが無い");
        return;
    }
    let found = model.and_then(|(model, ids)| {
        let place = placement(shared, env.surface_hwnd, &model)?;
        let row = pick(&place, &model, &ids, |it| button_exists(it, button))?;
        let at = place.header_button(&model.items[row.index], row.index, button)?;
        Some((row, at))
    });
    let Some((row, at)) = found else {
        log.step(name, false, json!("見出しのボタンの位置を求められない"));
        return;
    };
    let before = layer_of(shared, &row.id);
    let selected = shared.selected_layer();
    surface::post_click(env.surface_hwnd, at.0, at.1);
    let pressed = wait_until(WAIT, || {
        layer_of(shared, &row.id).is_some_and(|l| changed(&l))
    });
    let kept = shared.selected_layer() == selected;
    log.step(
        name,
        pressed && kept,
        json!({ "layer": row.id, "selected_unchanged": kept }),
    );
    shot(env, log, &format!("14-header-{name}"));
    if !pressed {
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || layer_of(shared, &row.id) == before);
    log.step(&format!("undo_{name}"), undone, json!({}));
}

/// 音を持てるレイヤーだけが Mute と Solo を持つ。
fn button_exists(item: &mm_ui_layout::Item, button: HeaderButton) -> bool {
    match button {
        HeaderButton::Mute | HeaderButton::Solo => item.audio.is_some(),
        HeaderButton::Enabled | HeaderButton::Lock => item.cuts.is_empty(),
    }
}

/// 見出しをつかむドラッグの位置。先頭の行を、すぐ下の行の下半分へ動かす。
struct Reorder {
    top: Row,
    below: Row,
    from: (i32, i32),
    to: (i32, i32),
}

fn reorder(env: &Env<'_>) -> Option<Reorder> {
    let shared = env.shared;
    let (model, ids) = model_of(shared)?;
    let place = placement(shared, env.surface_hwnd, &model)?;
    let n = model.items.len();
    let first = place.first_visible(|i| i + 1 < n)?;
    place.first_visible(|i| i == first + 1)?;
    let item = |i: usize| &model.items[i];
    let row = |i: usize| Row {
        index: i,
        id: ids[i].clone(),
    };
    Some(Reorder {
        from: place.header_name(item(first), first, 0.0),
        to: place.header_name(item(first + 1), first + 1, 8.0),
        top: row(first),
        below: row(first + 1),
    })
}

/// 見出しを `from` で掴んで `to` までドラッグし、Esc で取りやめて並びが変わらないこと、
/// 離すと 1 回の操作で前後が入れ替わり、取り消し 1 回で戻ることを確かめる。
fn reorder_step(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let Some(r) = reorder(env) else {
        log.step("header_drag", false, json!("見出しの位置を求められない"));
        return;
    };
    let before = order(shared);
    // 記録には動かす 2 つのレイヤーの位置（下から）だけを書く（レイヤーが数千あるプロジェクトでも短く）。
    let ranks = |order: &[String]| {
        let rank = |id: &str| order.iter().position(|l| l == id);
        (rank(&r.top.id), rank(&r.below.id))
    };
    surface::post_press(hwnd, r.from, r.to);
    shot(env, log, "15-header-drag-line");
    cancel_drag(hwnd, r.from);
    let canceled = order(shared) == before;
    log.step(
        "header_drag_cancel",
        canceled,
        json!({ "ranks": ranks(&before) }),
    );
    let in_front = |order: &[String]| {
        let (top, below) = ranks(order);
        top > below
    };
    surface::post_drag(hwnd, r.from, r.to);
    let moved = wait_until(WAIT, || !in_front(&order(shared)));
    log.step(
        "header_drag",
        moved && in_front(&before),
        json!({ "layer": r.top.id, "behind": r.below.id, "ranks_after": ranks(&order(shared)) }),
    );
    shot(env, log, "16-header-drag-done");
    if !moved {
        return;
    }
    keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
    let undone = wait_until(WAIT, || order(shared) == before);
    log.step("undo_header_drag", undone, json!({}));
}

/// 見出しを押してレイヤーを選び、View > Shy Selected Layers で Shy のフラグを立て（見出しの名前の色が変わる）、Hide Shy Layers で
/// 行を隠し、戻す。
fn shy_step(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let Some(r) = reorder(env) else {
        log.step("shy_hide", false, json!("見出しの位置を求められない"));
        return;
    };
    // クリックが届いたことを選択の変化で見分けるため、先に選択を外す（届く前に隠すと、押した行が無くなる）。
    shared.select_layer(None);
    surface::post_click(hwnd, r.from.0, r.from.1);
    let selected = wait_until(WAIT, || shared.selected_layer() == Some(r.top.id.clone()));
    let marked = commands::run(shared, "timeline.shy_selected_layers").is_ok();
    std::thread::sleep(SETTLE);
    shot(env, log, "17-header-shy-marked");
    let hidden = commands::run(shared, "timeline.hide_shy_layers").is_ok();
    let state = shared.view.shy();
    let ok = selected && marked && hidden && state.hides(&r.top.id);
    log.step(
        "shy_hide",
        ok,
        json!({ "layer": r.top.id, "shy": state.layers, "hide": state.hide }),
    );
    std::thread::sleep(SETTLE);
    shot(env, log, "18-header-shy-hidden");
    let shown = commands::run(shared, "timeline.hide_shy_layers");
    let unmarked = commands::run(shared, "timeline.shy_selected_layers");
    let state = shared.view.shy();
    let restored = shown.is_ok() && unmarked.is_ok() && state == Default::default();
    log.step(
        "shy_restore",
        restored,
        json!({ "shy": state.layers, "hide": state.hide, "selected": shared.selected_layer() }),
    );
}

/// 見出しの通し。
pub(super) fn run(env: &Env<'_>, log: &mut Log) {
    button_step(env, log, ("header_enabled", HeaderButton::Enabled), |l| {
        !l.enabled
    });
    button_step(env, log, ("header_lock", HeaderButton::Lock), |l| l.lock);
    button_step(env, log, ("header_mute", HeaderButton::Mute), |l| {
        l.audio.as_ref().is_some_and(|a| a.mute)
    });
    reorder_step(env, log);
    shy_step(env, log);
}
