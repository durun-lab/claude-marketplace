//! サーフェスの右クリックのメニュー。WM_CONTEXTMENU（右クリックとメニューキー）をポインターの入力のキューで描画スレッドへ渡し、
//! 描画スレッドが当たった所を決めて選択を置いた後、UI のスレッド（メインスレッド）がネイティブのポップアップで出す。
//! 項目の並びは `menu.rs`、名前とキーと押せるかはコマンドの表。選んだ項目はメニューバーと同じ `on_menu_event` で実行する。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use mm_ui_layout::{Hit, Place, PointerEvent};
use tauri::{AppHandle, PhysicalPosition, Window};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{ClientToScreen, ScreenToClient};
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CONTEXTMENU};

use crate::win::commands::{self, Forward};
use crate::win::menu::{self, Line};
use crate::win::state::Shared;

/// 出したメニューの記録。ポップアップは画面の撮影に写らないので、自動試験は並びをここから読む。
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub place: Place,
    /// 項目の文字と、区切り線の `-`。押せない項目には ` (disabled)`、入の項目には ` (checked)` を付ける。
    pub items: Vec<String>,
}

/// 右クリックのメニューの状態。
#[derive(Default)]
pub struct ContextSlot {
    /// 右クリックした時刻（秒）。Split Clip Here の割る時刻。次の左クリックまで持つ。
    time: Mutex<Option<f64>>,
    shown: Mutex<Option<Shown>>,
    /// ポップアップを出さず、並びの記録だけを残す（自動試験は、モーダルのメニューで止まらないため）。
    headless: AtomicBool,
}

impl ContextSlot {
    pub fn set_time(&self, time: f64) {
        *self.time.lock().expect("context time") = Some(time);
    }

    pub fn clear_time(&self) {
        *self.time.lock().expect("context time") = None;
    }

    /// 右クリックした時刻。右クリックのメニューを出していなければ `None`。
    pub fn time(&self) -> Option<f64> {
        *self.time.lock().expect("context time")
    }

    /// 直近に出したメニューを取り出す（取り出すと空に戻る）。
    pub fn take_shown(&self) -> Option<Shown> {
        self.shown.lock().expect("context shown").take()
    }

    /// ポップアップを出さずに記録だけ残す（自動試験用）。
    pub fn set_headless(&self, headless: bool) {
        self.headless.store(headless, Ordering::SeqCst);
    }
}

/// `WM_CONTEXTMENU` を Timeline への入力にする。マウスの右クリックは画面の座標（lParam）を、メニューキー（Shift+F10 と
/// アプリケーションキー。lParam は -1）は選んでいるものの位置を使う。マウスのときはキーを受けられるようサーフェスにフォーカスを移す。
pub fn requested(hwnd: HWND, lparam: LPARAM) -> PointerEvent {
    if lparam.0 == -1 {
        return PointerEvent::ContextMenuKey;
    }
    let (x, y) = (lparam.0 & 0xFFFF, (lparam.0 >> 16) & 0xFFFF);
    let mut p = POINT {
        x: i32::from(x as u16 as i16),
        y: i32::from(y as u16 as i16),
    };
    // 画面の座標からサーフェスのクライアントの座標に直す。失敗しても点が画面の座標のままになるだけで、メニューは出る。
    unsafe {
        let _ = ScreenToClient(hwnd, &mut p);
        let _ = SetFocus(Some(hwnd));
    }
    PointerEvent::ContextMenu {
        x: p.x as f32,
        y: p.y as f32,
    }
}

/// サーフェスの HWND に、クライアントの点 `(x, y)`（物理ピクセル）の右クリックのメッセージを投げる（自動試験用）。
pub fn post_at(hwnd: isize, (x, y): (i32, i32)) {
    let h = HWND(hwnd as *mut _);
    let mut p = POINT { x, y };
    unsafe {
        let _ = ClientToScreen(h, &mut p);
        let lparam = LPARAM((((p.y & 0xFFFF) << 16) | (p.x & 0xFFFF)) as isize);
        let _ = PostMessageW(Some(h), WM_CONTEXTMENU, WPARAM(hwnd as usize), lparam);
    }
}

/// サーフェスの HWND に、メニューキー（Shift+F10 とアプリケーションキー）で Windows が送る `WM_CONTEXTMENU`（lParam は -1）を
/// 投げる（自動試験用。キーのメッセージを投げても、OS は WM_CONTEXTMENU にしない）。
pub fn post_menu_key(hwnd: isize) {
    let result = unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut _)),
            WM_CONTEXTMENU,
            WPARAM(hwnd as usize),
            LPARAM(-1),
        )
    };
    if let Err(e) = result {
        tracing::warn!("メニューキーを投げられない: {e}");
    }
}

/// このアプリが出しているポップアップのメニュー（ウィンドウクラス `#32768`）。自動試験が出たことを待つのに使う。
/// 同じクラスのメニューはどのアプリのものも見つかるので、このプロセスのものだけを返し、ほかのアプリにキーを送らない。
pub fn open_popup() -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};
    let menu = unsafe { FindWindowW(w!("#32768"), None) }.ok()?;
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(menu, Some(&mut pid)) };
    (pid == std::process::id()).then_some(menu.0 as isize)
}

/// 出ているポップアップのメニューにキーを送る（自動試験用）。メニューはモーダルのループでキーを読むので、キーボードの
/// 操作と同じに、押している間の項目を動かす。
pub fn post_popup_key(menu: isize, vk: usize) {
    use windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN;
    let h = HWND(menu as *mut _);
    unsafe {
        let _ = PostMessageW(Some(h), WM_KEYDOWN, WPARAM(vk), LPARAM(1));
    }
}

/// 右クリックのメニューを出す。描画スレッドが呼び、ポップアップはメインスレッドで出す（メニューを閉じるまで返らない）。
/// `at` はサーフェスのクライアントの点（物理ピクセル）。サーフェスはウィンドウのクライアント領域の左上から始まるので、
/// そのままウィンドウの位置になる。
pub fn show(app: &AppHandle, window: &Window, shared: &Arc<Shared>, hit: Hit, at: (i32, i32)) {
    let (app_for_menu, window, shared) = (app.clone(), window.clone(), shared.clone());
    let queued = app.run_on_main_thread(move || popup(&app_for_menu, &window, &shared, hit, at));
    if let Err(e) = queued {
        tracing::warn!("右クリックのメニューを出せない: {e}");
    }
}

fn popup(app: &AppHandle, window: &Window, shared: &Arc<Shared>, hit: Hit, at: (i32, i32)) {
    let Some(place) = Place::of(&hit) else {
        return;
    };
    let table = commands::table(shared);
    let lines = menu::lines(menu::context_groups(place), &table);
    if lines.is_empty() {
        return;
    }
    *shared.context.shown.lock().expect("context shown") = Some(Shown {
        place,
        items: describe(&lines),
    });
    if shared.context.headless.load(Ordering::SeqCst) {
        return;
    }
    let result = menu::context_menu(app, &lines)
        .and_then(|m| window.popup_menu_at(&m, PhysicalPosition::new(at.0, at.1)));
    if let Err(e) = result {
        tracing::warn!("右クリックのメニューを出せない: {e}");
    }
}

fn describe(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| match line {
            Line::Separator => "-".to_owned(),
            Line::Command(c) => {
                let mut text = menu::label(c).replace('\t', "  ");
                if !c.state.enabled {
                    text.push_str(" (disabled)");
                }
                if c.state.checked == Some(true) {
                    text.push_str(" (checked)");
                }
                text
            }
        })
        .collect()
}

/// 描画スレッドが `Action::ContextMenu` を受けたとき。右クリックした時刻を置き、メニューを出すようウィンドウのフックへ頼む。
pub fn on_action(shared: &Arc<Shared>, hit: Hit, time: f64, at: (f32, f32)) {
    shared.context.set_time(time);
    let at = (at.0.round() as i32, at.1.round() as i32);
    if let Err(e) = shared.window_hook.send(Forward::ContextMenu(hit, at)) {
        tracing::warn!("右クリックのメニューを出せない: {e}");
    }
}

#[cfg(test)]
mod tests;
