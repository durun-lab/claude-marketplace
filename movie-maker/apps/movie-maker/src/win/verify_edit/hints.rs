//! `--play-test` の手がかりの通し。カットの端にポインターを乗せたとき、ステータスバーに「Drag: Ripple Trim」が出る画面を撮る。
//! カーソルとツールチップは PrintWindow に写らないので、カーソルは決める関数（`cursor_for`）の結果、ツールチップは出した文と
//! 位置の記録で確かめる。Blade と Trim のカーソルとステータスバーも確かめる。

use mm_ui_layout::{cursor_for, CursorKind, Mods, Tool};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::ClientToScreen;

use super::tools::{select_tool, splittable_span};
use super::*;

/// 画面の座標の許容（ピクセル）。
const POS_TOLERANCE: i32 = 2;
const ALT: Mods = Mods {
    shift: false,
    alt: true,
};
const SHIFT: Mods = Mods {
    shift: true,
    alt: false,
};

fn status_left(shared: &Shared) -> String {
    shared.hint.status().0
}

/// ステータスバーの左の文が `want` になるのを待つ。
fn wait_status(shared: &Shared, want: &str) -> bool {
    wait_until(WAIT, || status_left(shared) == want)
}

/// 出したツールチップのうち、文が `prefix` で始まるもの。
fn shown_with(shared: &Shared, prefix: &str) -> Option<crate::win::tooltip::Shown> {
    let shown = shared.hint.shown();
    shown.into_iter().rev().find(|s| s.text.starts_with(prefix))
}

/// クライアントの座標（物理ピクセル）の画面の座標。
fn screen_of(hwnd: isize, (x, y): (i32, i32)) -> (i32, i32) {
    let mut p = POINT { x, y };
    let _ = unsafe { ClientToScreen(HWND(hwnd as *mut _), &mut p) };
    (p.x, p.y)
}

/// Selection のカットの端で Ripple と Roll のステータスバーとカーソル、Trim の中で Slip と Slide、Blade の刃を確かめる。
fn cut_steps(env: &Env<'_>, log: &mut Log, g: &Geometry, (start, end): (f64, f64)) {
    let shared = env.shared;
    select_tool(env, Tool::Selection);
    surface::push_hover(g.at(g.row, end, -HANDLE_INSET));
    let status = wait_status(shared, "Drag: Ripple Trim \u{B7} Alt+Drag: Roll");
    let pointee = shared.hint.current().pointee;
    let cursors = (
        cursor_for(&pointee, Mods::default()),
        cursor_for(&pointee, ALT),
    );
    log.step(
        "hint_status_ripple",
        status && cursors == (CursorKind::SizeWE, CursorKind::Roll),
        json!({ "status": status_left(shared), "cursors": format!("{cursors:?}") }),
    );
    shot(env, log, "21-status-ripple");

    let mid = g.at(g.row, (start + end) / 2.0, 0.0);
    select_tool(env, Tool::Trim);
    surface::push_hover(mid);
    let status = wait_status(shared, "Drag: Slip \u{B7} Shift+Drag: Slide");
    let pointee = shared.hint.current().pointee;
    let cursors = (
        cursor_for(&pointee, Mods::default()),
        cursor_for(&pointee, SHIFT),
    );
    log.step(
        "hint_status_slip",
        status && cursors == (CursorKind::Slip, CursorKind::Slide),
        json!({ "status": status_left(shared), "cursors": format!("{cursors:?}") }),
    );

    select_tool(env, Tool::Blade);
    surface::push_hover(mid);
    let status = wait_status(shared, "Click: Split at Pointer");
    let blade = cursor_for(&shared.hint.current().pointee, Mods::default());
    let right = shared.hint.status().1;
    let snapping = if shared.view.snapping() { "On" } else { "Off" };
    log.step(
        "hint_status_blade",
        status
            && blade == CursorKind::Blade
            && right == format!("Snapping {snapping} \u{B7} Blade Tool"),
        json!({ "status": status_left(shared), "cursor": format!("{blade:?}"), "right": right }),
    );
    shot(env, log, "22-status-blade");
    select_tool(env, Tool::Selection);
}

/// ツールバーのボタンと帯の上で、止まるとツールチップが出る。文はコマンドの表と帯の情報から作ったもの。
fn tooltip_steps(env: &Env<'_>, log: &mut Log, g: &Geometry, (start, end): (f64, f64)) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let Some(button) = g.place.toolbar_button("timeline.selection_tool") else {
        log.step(
            "hint_tooltip_toolbar",
            false,
            json!("ボタンの位置を引けない"),
        );
        return;
    };
    surface::push_hover(button);
    let found = wait_until(WAIT, || {
        shown_with(shared, "Selection Tool (A)\n").is_some()
    });
    let shown = shown_with(shared, "Selection Tool (A)\n");
    let want = screen_of(hwnd, button);
    let placed = shown
        .as_ref()
        .is_some_and(|s| (s.at.0 - want.0).abs() <= POS_TOLERANCE && s.at.1 >= want.1);
    log.step(
        "hint_tooltip_toolbar",
        found && placed,
        json!({ "shown": shown.map(|s| (s.text, s.at)), "pointer": want }),
    );

    surface::push_hover(g.at(g.row, (start + end) / 2.0, 0.0));
    let found = wait_until(WAIT, || shown_with(shared, "Clip \u{B7} ").is_some());
    let shown = shown_with(shared, "Clip \u{B7} ");
    let ranged = shown.as_ref().is_some_and(|s| s.text.contains('\u{2013}'));
    log.step(
        "hint_tooltip_bar",
        found && ranged,
        json!({ "shown": shown.map(|s| (s.text, s.at)) }),
    );
}

/// 手がかり（ステータスバー、カーソル、ツールチップ）の通し。
pub(super) fn run(env: &Env<'_>, log: &mut Log) {
    let shared = env.shared;
    let found =
        geometry(shared, env.surface_hwnd).and_then(|g| Some((splittable_span(shared, g.row)?, g)));
    let Some((span, g)) = found else {
        log.step(
            "hint_status_ripple",
            false,
            json!("割れるカットの位置を引けない"),
        );
        return;
    };
    cut_steps(env, log, &g, span);
    tooltip_steps(env, log, &g, span);
}
