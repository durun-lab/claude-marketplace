//! `--play-test` の右クリックのメニューの通し。サーフェスに本物の WM_CONTEXTMENU を投げ、描画スレッドがカットとレイヤーを選んで
//! メニューの並びを作ることを確かめ、メニューの代わりに選んだ項目の ID（`context_menu::item_id`）をメニューバーと同じ道
//! （`menu::run`）に流して実行する。
//! ポップアップは画面の撮影に写らず、モーダルで止まるので、出さずに並びの記録だけを読む。

use mm_ui_layout::HitKind;

use super::tools::{cut_starts_near, layer_start, splittable_span, undo_to};
use super::*;
use crate::win::context_menu;
use crate::win::menu;

pub(super) fn wait_shown(shared: &Shared, kind: HitKind) -> Option<context_menu::ShownMenu> {
    let mut found = None;
    wait_until(WAIT, || {
        found = shared.context.take_shown();
        found.as_ref().is_some_and(|s| s.kind == kind)
    });
    found.filter(|s| s.kind == kind)
}

fn layer_lock(shared: &Shared, id: &str) -> Option<bool> {
    let doc = shared.doc()?;
    let (_, layer) = doc.project.layer(&Id::new(id)).ok()?;
    Some(layer.lock)
}

/// カットを右クリックすると、そのカットを選んだカットのメニューが出て、Split Clip Here が右クリックした時刻で割れて、
/// 取り消し 1 回で戻る。
fn cut_menu_step(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let found = geometry(shared, hwnd).and_then(|g| Some((splittable_span(shared, g.row)?, g)));
    let Some(((start, end), g)) = found else {
        log.step("context_cut", false, json!("割れるカットの位置を引けない"));
        return;
    };
    let mid = (start + end) / 2.0;
    let before = cuts(shared).len();
    context_menu::post_at(hwnd, g.at(g.row, mid, 0.0));
    let shown = wait_shown(shared, HitKind::Cut);
    let selected = wait_until(WAIT, || shared.edit.selected_cut().is_some());
    let items = shown.as_ref().map(|s| s.items.clone());
    let split_listed = items
        .as_ref()
        .is_some_and(|i| i.iter().any(|t| t == "Split Clip Here"));
    log.step(
        "context_cut",
        selected && split_listed,
        json!({ "items": items, "selected_cut": shared.edit.selected_cut() }),
    );
    menu::run(shared, &context_menu::item_id("clip.split_here"));
    let split = wait_until(WAIT, || cuts(shared).len() == before + 1);
    let near = cut_starts_near(shared, g.row, (start, mid), 1.0 / 24.0);
    log.step(
        "context_split_here",
        split && near,
        json!({ "cuts_before": before, "cuts_after": cuts(shared).len(), "at": mid }),
    );
    if split {
        undo_to(env, log, "undo_context_split_here", before);
    }
}

/// レイヤーの帯を右クリックするとレイヤーを選んだメニューが出て、Lock Layer がそのレイヤーのロックを入れる。
/// 続けてメニューキーで、選んでいるレイヤーのメニューが出る。
fn layer_menu_steps(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let found = geometry(shared, hwnd).and_then(|g| {
        let (row, id, end) = trim_target(shared, &g.place)?;
        Some((g, row, layer_start(shared, &id)?, (id, end)))
    });
    let Some((g, row, start, (id, end))) = found else {
        log.step("context_layer", false, json!("レイヤーの位置を引けない"));
        return;
    };
    context_menu::post_at(hwnd, g.at(row, (start + end) / 2.0, 0.0));
    let shown = wait_shown(shared, HitKind::Layer);
    let selected = wait_until(WAIT, || {
        shared.selected_layer().as_deref() == Some(id.as_str())
    });
    let items = shown.as_ref().map(|s| s.items.clone());
    log.step(
        "context_layer",
        selected && items.as_ref().is_some_and(|i| i.len() > 2),
        json!({ "layer": id, "items": items }),
    );
    let before = layer_lock(shared, &id);
    menu::run(shared, &context_menu::item_id("layer.toggle_lock"));
    let locked = wait_until(WAIT, || layer_lock(shared, &id) != before);
    log.step(
        "context_lock_layer",
        locked,
        json!({ "before": before, "after": layer_lock(shared, &id) }),
    );
    if locked {
        keys::dispatch(shared, "play-test", "Ctrl+KeyZ");
        let undone = wait_until(WAIT, || layer_lock(shared, &id) == before);
        log.step("undo_context_lock_layer", undone, json!({}));
    }
    context_menu::post_menu_key(hwnd);
    let key_menu = wait_shown(shared, HitKind::Layer);
    log.step(
        "context_menu_key",
        key_menu.is_some(),
        json!({ "items": key_menu.map(|s| s.items) }),
    );
}

/// 本物のポップアップ（`TrackPopupMenu`）をカットの右クリックで出し、キーで Split Clip Here を選ぶと、メニューバーと同じ
/// `on_menu_event` でカットが割れる。メニューが出ている間もウィンドウのスレッドが止まらないことの確かめ。
fn popup_step(env: &Env<'_>, log: &mut Log) {
    const VK_DOWN: usize = 0x28;
    const VK_RETURN: usize = 0x0D;
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let found = geometry(shared, hwnd).and_then(|g| Some((splittable_span(shared, g.row)?, g)));
    let Some(((start, end), g)) = found else {
        log.step(
            "context_popup",
            false,
            json!("割れるカットの位置を引けない"),
        );
        return;
    };
    let before = cuts(shared).len();
    context_menu::post_at(hwnd, g.at(g.row, (start + end) / 2.0, 0.0));
    let mut menu = None;
    wait_until(WAIT, || {
        menu = context_menu::open_popup();
        menu.is_some()
    });
    let Some(menu) = menu else {
        log.step("context_popup", false, json!("ポップアップが出ない"));
        return;
    };
    context_menu::post_popup_key(menu, VK_DOWN);
    std::thread::sleep(SETTLE);
    context_menu::post_popup_key(menu, VK_RETURN);
    let split = wait_until(WAIT, || cuts(shared).len() == before + 1);
    let closed = wait_until(WAIT, || context_menu::open_popup().is_none());
    log.step(
        "context_popup",
        split && closed,
        json!({ "cuts_before": before, "cuts_after": cuts(shared).len() }),
    );
    if split {
        undo_to(env, log, "undo_context_popup", before);
    }
}

pub(crate) fn run(env: &Env<'_>, log: &mut Log) {
    env.shared.context.set_headless(true);
    cut_menu_step(env, log);
    layer_menu_steps(env, log);
    env.shared.context.set_headless(false);
    popup_step(env, log);
}
