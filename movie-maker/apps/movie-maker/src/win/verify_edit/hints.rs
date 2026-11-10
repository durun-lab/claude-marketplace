//! `--play-test` の手がかりの通し。カットの端にポインターを乗せたとき、ステータスバーに「Drag: Ripple Trim」が出る画面を撮る。
//! カーソルとツールチップは PrintWindow に写らないので、カーソルは決める関数（`cursor_for`）の結果、ツールチップは出した文と
//! 位置の記録と窓が見えているかで確かめる。Blade と Trim のカーソルとステータスバー、ドラッグ中の文、右の文の追従、
//! Alt を押して離してもメニューのモードに入らないことも確かめる。

use mm_ui_layout::{cursor_for, CursorKind, Mods, Tool};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetClassNameW, GetGUIThreadInfo, GetWindowThreadProcessId, IsWindowVisible,
    PostMessageW, GUITHREADINFO, GUI_INMENUMODE, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use super::tools::{select_tool, splittable_span};
use super::*;
use crate::win::tooltip::Shown;

/// 画面の座標の許容（ピクセル）。
const POS_TOLERANCE: i32 = 2;
/// キーの表に無いキー（F24）。押してもコマンドにならず、ツールチップを隠すだけ。
const VK_F24: u32 = 0x87;
/// Alt だけを押したときと離したときの lParam（スキャンコード 0x38。押下は Alt を押している印、離すときは前の状態と離した印）。
const ALT_DOWN: LPARAM = LPARAM(0x2038_0001);
const ALT_UP: LPARAM = LPARAM(0xC038_0001);
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

/// 記録の数が `count` だった後に出したツールチップのうち、文が `prefix` で始まるもの（前の工程で出した同じ文と分ける）。
fn shown_with(shared: &Shared, count: usize, prefix: &str) -> Option<Shown> {
    let later = shared.hint.shown_since(count);
    later.into_iter().rev().find(|s| s.text.starts_with(prefix))
}

/// `at` にポインターを乗せ、文が `prefix` で始まるツールチップが新しく出るのを待つ。出た記録と、ポインターの画面の座標。
fn hover_tooltip(env: &Env<'_>, at: (i32, i32), prefix: &str) -> (Option<Shown>, (i32, i32)) {
    let shared = env.shared;
    let count = shared.hint.shown_count();
    surface::push_hover(at);
    wait_until(WAIT, || shown_with(shared, count, prefix).is_some());
    let shown = shown_with(shared, count, prefix);
    (shown, screen_of(env.surface_hwnd, at))
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
    shot(env, log, "30-status-ripple");

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
    shot(env, log, "31-status-blade");
    select_tool(env, Tool::Selection);
}

/// ツールバーのボタンと帯の上で、止まるとツールチップが出る。文はコマンドの表と帯の情報から作ったもの。
fn tooltip_steps(env: &Env<'_>, log: &mut Log, g: &Geometry, (start, end): (f64, f64)) {
    let Some(button) = g.place.toolbar_button("timeline.selection_tool") else {
        log.step(
            "hint_tooltip_toolbar",
            false,
            json!("ボタンの位置を引けない"),
        );
        return;
    };
    let (shown, want) = hover_tooltip(env, button, "Selection Tool (A)\n");
    let placed = shown.as_ref().is_some_and(|s| below(s.at, want));
    log.step(
        "hint_tooltip_toolbar",
        placed,
        json!({ "shown": shown.map(|s| (s.text, s.at)), "pointer": want }),
    );

    // 部品が変わったら、文と位置を新しい部品とポインターの位置に差し替える。
    let bar = g.at(g.row, (start + end) / 2.0, 0.0);
    let (shown, want) = hover_tooltip(env, bar, "Clip \u{B7} ");
    let ranged = shown
        .as_ref()
        .is_some_and(|s| s.text.contains('\u{2013}') && below(s.at, want));
    log.step(
        "hint_tooltip_bar",
        ranged,
        json!({ "shown": shown.map(|s| (s.text, s.at)), "pointer": want }),
    );
}

/// ツールチップの左上 `at` が、ポインター `pointer` の下に x をそろえて出ているか（画面の座標）。
fn below(at: (i32, i32), pointer: (i32, i32)) -> bool {
    (at.0 - pointer.0).abs() <= POS_TOLERANCE && at.1 > pointer.1
}

/// ツールチップを出したままキーの表に無いキーを押すと、ツールチップが隠れる（押下とホイールも同じ振り分けで隠す）。
fn tooltip_dismiss_step(env: &Env<'_>, log: &mut Log) {
    let hwnd = env.surface_hwnd;
    let shown = wait_until(WAIT, || tooltip_visible(hwnd));
    surface::post_key(hwnd, VK_F24);
    let hidden = wait_until(WAIT, || !tooltip_visible(hwnd));
    log.step(
        "hint_tooltip_dismiss",
        shown && hidden,
        json!({ "shown": shown, "hidden": hidden }),
    );
}

/// サーフェスと同じスレッドのツールチップの窓が見えているか。
fn tooltip_visible(hwnd: isize) -> bool {
    unsafe extern "system" fn visit(window: HWND, found: LPARAM) -> BOOL {
        let mut class = [0u16; 32];
        let n = unsafe { GetClassNameW(window, &mut class) } as usize;
        let tooltip = String::from_utf16_lossy(&class[..n]) == "tooltips_class32";
        if tooltip && unsafe { IsWindowVisible(window) }.as_bool() {
            // 見つけた印を立てて列挙を止める。
            unsafe { *(found.0 as *mut bool) = true };
            return false.into();
        }
        true.into()
    }
    let mut found = false;
    let thread = unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), None) };
    let flag = LPARAM(std::ptr::from_mut(&mut found) as isize);
    let _ = unsafe { EnumThreadWindows(thread, Some(visit), flag) };
    found
}

/// Alt だけを押して離しても、メニューバーにフォーカスが移らない（サーフェスは Alt を既定のウィンドウプロシージャに渡さないので、
/// Alt+ドラッグの roll と Alt を押したときのカーソルの替え直しがメニューを開かない）。
fn alt_menu_step(env: &Env<'_>, log: &mut Log) {
    let hwnd = HWND(env.surface_hwnd as *mut _);
    let alt = WPARAM(usize::from(VK_MENU.0));
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_SYSKEYDOWN, alt, ALT_DOWN);
        let _ = PostMessageW(Some(hwnd), WM_SYSKEYUP, alt, ALT_UP);
    }
    std::thread::sleep(SETTLE);
    let in_menu = in_menu_mode(hwnd);
    if in_menu {
        // 後の工程のキーをメニューに取られないよう、メニューのモードを抜ける。
        surface::post_escape(env.surface_hwnd);
        surface::post_escape(env.surface_hwnd);
    }
    log.step(
        "hint_alt_menu",
        !in_menu,
        json!({ "in_menu_mode": in_menu }),
    );
}

/// `hwnd` の UI のスレッドがメニューのモードか。
fn in_menu_mode(hwnd: HWND) -> bool {
    let thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetGUIThreadInfo(thread, &mut info) }.is_ok() && (info.flags & GUI_INMENUMODE).0 != 0
}

/// カットの終わりのつまみを再生位置の近くへ引いたまま、ドラッグ中の文（操作の名前と量、吸い付いた先、N と Esc）を確かめ、
/// N を押している間は吸い付きが外れた文になり、Esc で取りやめると元の文に戻る。
fn drag_status_step(env: &Env<'_>, log: &mut Log, g: &Geometry, (start, end): (f64, f64)) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let from = g.at(g.row, end, -HANDLE_INSET);
    seek_main(shared, (start + end) / 2.0);
    surface::push_hover(from);
    surface::post_press(hwnd, from, g.at(g.row, shared.playhead(), SNAP_NEAR));
    let tail = "\u{B7} N: Toggle Snapping \u{B7} Esc: Cancel";
    let snapped = wait_until(WAIT, || {
        let s = status_left(shared);
        s.starts_with("Ripple Trim \u{2212}")
            && s.contains("Snapped to Playhead")
            && s.ends_with(tail)
    });
    let during = status_left(shared);
    surface::post_key(hwnd, VK_N);
    let held = wait_until(WAIT, || !status_left(shared).contains("Snapped to"));
    surface::post_key_up(hwnd, VK_N);
    cancel_drag(hwnd, from);
    surface::push_hover(from);
    let back = wait_status(shared, "Drag: Ripple Trim \u{B7} Alt+Drag: Roll");
    log.step(
        "hint_status_dragging",
        snapped && held && back,
        json!({ "dragging": during, "n_held": held, "after_cancel": status_left(shared) }),
    );
}

/// 右の文は Snapping の入/切に追従する（ツールの切り替えは `cut_steps` の Blade で確かめる）。
fn right_status_step(env: &Env<'_>, log: &mut Log) {
    let (shared, hwnd) = (env.shared, env.surface_hwnd);
    let right = || shared.hint.status().1;
    let before = shared.view.snapping();
    let flipped = if before { "Off" } else { "On" };
    let want = format!("Snapping {flipped} \u{B7} Selection Tool");
    surface::post_key(hwnd, VK_N);
    let toggled = wait_until(WAIT, || right() == want);
    let seen = right();
    surface::post_key(hwnd, VK_N);
    let restored = wait_until(WAIT, || shared.view.snapping() == before);
    log.step(
        "hint_status_right",
        toggled && restored,
        json!({ "right": seen, "want": want }),
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
    drag_status_step(env, log, &g, span);
    right_status_step(env, log);
    tooltip_steps(env, log, &g, span);
    tooltip_dismiss_step(env, log);
    alt_menu_step(env, log);
}
