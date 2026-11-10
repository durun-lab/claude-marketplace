//! wgpu のサーフェスを張る子 HWND。Tauri のウィンドウの下に自前で作り、フォーカスとキーとポインターを自分で受ける。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use mm_ui_layout::{Mods, PointerEvent};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::Ime::{ImmAssociateContextEx, HIMC};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, GetKeyState, ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE,
    TRACKMOUSEEVENT, VK_CONTROL, VK_ESCAPE, VK_F10, VK_F4, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, IsWindow, LoadCursorW, PostMessageW, RegisterClassW,
    SetWindowPos, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, SWP_NOACTIVATE, SWP_NOZORDER,
    WM_CAPTURECHANGED, WM_CONTEXTMENU, WM_ERASEBKGND, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSW,
    WS_CHILD, WS_CLIPSIBLINGS, WS_EX_NOPARENTNOTIFY, WS_VISIBLE,
};

use crate::win::context_menu;
use crate::win::hotpatch;
use crate::win::keyboard;
use crate::win::keys;
use crate::win::pointer_hint;
use crate::win::splitter;
use crate::win::state::Shared;

/// 1 ノッチあたりのホイールの値。
const WHEEL_DELTA: f32 = 120.0;
/// Ctrl キーと Shift キーの修飾（WM_MOUSEWHEEL の wParam の下位語）。
const MK_CONTROL: usize = 0x0008;
const MK_SHIFT: usize = 0x0004;
/// マウスがウィンドウから出た通知（`TrackMouseEvent` で頼んだもの）。
const WM_MOUSELEAVE: u32 = 0x02A3;
/// WM_KEYDOWN の lParam で、直前もキーが押されていた（押し続けの繰り返し）ことを示すビット。
const KEY_REPEAT_BIT: isize = 1 << 30;
/// 描画が止まっているときに入力が溜まり続けない上限。
const INPUT_LIMIT: usize = 512;
/// 表示の倍率が 1 のときの DPI（Windows の既定）。
const BASE_DPI: f32 = 96.0;

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();
static TRACKING: AtomicBool = AtomicBool::new(false);
/// 直前の `WM_SYSKEYDOWN` がキーの表にあるキーだったか。そのキーの `WM_SYSCHAR` を既定のウィンドウプロシージャに渡すと、
/// メニューのアクセスキーとして探され、合う字が無いと警告音が鳴るため、渡さない。
static SYSKEY_IN_TABLE: AtomicBool = AtomicBool::new(false);

/// 矩形（x、y、幅、高さ。物理ピクセル）。
pub type PixelRect = (i32, i32, i32, i32);

/// サーフェスの子 HWND のウィンドウクラスの名前。Keyboard Shortcuts のウィンドウを閉じたとき、これで探してフォーカスを戻す。
pub const CLASS: PCWSTR = w!("MmSurface");

/// 親の HWND の下に子 HWND を作る。位置と大きさは物理ピクセル。
pub fn create(parent: isize, shared: Arc<Shared>, rect: PixelRect) -> windows::core::Result<isize> {
    // wndproc から共有状態に届く道はこの静的変数だけなので、作る前に置く。
    let _ = SHARED.set(shared);
    let (x, y, w, h) = rect;
    unsafe {
        let hinstance = GetModuleHandleW(PCWSTR::null())?;
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: CLASS,
            ..Default::default()
        };
        // 2 回目以降の登録は失敗するが、同じクラスが既にあるだけなので続ける。
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_NOPARENTNOTIFY,
            CLASS,
            w!("wgpu surface"),
            WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
            x,
            y,
            w,
            h,
            Some(HWND(parent as *mut _)),
            None,
            Some(hinstance.into()),
            None,
        )?;
        detach_ime(hwnd);
        Ok(hwnd.0 as isize)
    }
}

/// HWND から IME を外す。日本語の IME が付いていると英字のキーが `VK_PROCESSKEY` で届いてキーの表に無い名前になるので、
/// 文字を打たないサーフェスでは IME を使わない。
fn detach_ime(hwnd: HWND) {
    // IME の無い環境では失敗するが、そのときはキーがそのまま届くので、結果は破棄する。
    let _ = unsafe { ImmAssociateContextEx(hwnd, HIMC::default(), 0) };
}

/// 子 HWND の位置と大きさを変える（親のクライアント座標、物理ピクセル）。
pub fn set_rect(hwnd: isize, rect: PixelRect) {
    let (x, y, w, h) = rect;
    unsafe {
        // 失敗しても次の Resized で同じ値を入れ直すので、結果は破棄する。
        let _ = SetWindowPos(
            HWND(hwnd as *mut _),
            None,
            x,
            y,
            w,
            h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// HWND がまだ生きているか。破棄された HWND への present を止めるのに使う。
pub fn is_alive(hwnd: isize) -> bool {
    unsafe { IsWindow(Some(HWND(hwnd as *mut _))).as_bool() }
}

/// HWND の表示の倍率（DPI / 96）。モニターをまたいで動かすと変わるので、描くたびに読む。
pub fn scale_factor(hwnd: isize) -> f32 {
    let dpi = unsafe { GetDpiForWindow(HWND(hwnd as *mut _)) };
    // 破棄された HWND は 0 を返すので、そのときは等倍にする。
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / BASE_DPI
    }
}

/// サーフェスの HWND に左クリックのメッセージを投げる（自動試験用。座標はクライアントの物理ピクセル）。
pub fn post_click(hwnd: isize, x: i32, y: i32) {
    let lparam = LPARAM((((y & 0xFFFF) << 16) | (x & 0xFFFF)) as isize);
    let h = HWND(hwnd as *mut _);
    unsafe {
        let _ = PostMessageW(Some(h), WM_LBUTTONDOWN, WPARAM(0), lparam);
        let _ = PostMessageW(Some(h), WM_LBUTTONUP, WPARAM(0), lparam);
    }
}

/// サーフェスの HWND にキー押下のメッセージを投げる（自動試験用）。
pub fn post_key(hwnd: isize, vk: u32) {
    post_key_message(hwnd, WM_KEYDOWN, vk);
}

/// サーフェスの HWND にキーを離したメッセージを投げる（押している間だけ効くキーの自動試験用）。
pub fn post_key_up(hwnd: isize, vk: u32) {
    post_key_message(hwnd, WM_KEYUP, vk);
}

fn post_key_message(hwnd: isize, msg: u32, vk: u32) {
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd as *mut _)),
            msg,
            WPARAM(vk as usize),
            LPARAM(0),
        );
    }
}

/// 押している間のキーか（`GetKeyState` の上位ビット）。
fn key_down(vk: u16) -> bool {
    unsafe { GetKeyState(i32::from(vk)) < 0 }
}

/// いま押している修飾キー。ドラッグの種類（ripple か roll、slip か slide）を選ぶ。
pub(crate) fn pointer_mods() -> Mods {
    Mods {
        shift: key_down(VK_SHIFT.0),
        alt: key_down(VK_MENU.0),
    }
}

/// いま押している修飾キーを前に付けたキーの名前。記号のキーはこのスレッドの今のキー配列で文字にする。
fn key_name(vk: u32) -> String {
    let mods = (
        key_down(VK_CONTROL.0),
        key_down(VK_SHIFT.0),
        key_down(VK_MENU.0),
    );
    keyboard::key_name(vk, mods)
}

/// サーフェスの HWND に、左ボタンを押して `from` から `to` まで動かして離すメッセージを投げる（自動試験用）。
pub fn post_drag(hwnd: isize, from: (i32, i32), to: (i32, i32)) {
    post_press(hwnd, from, to);
    post_release(hwnd, to);
}

/// 左ボタンを押して `from` から `to` まで動かし、離さずに止める（ドラッグ中の画面を撮る自動試験用）。
pub fn post_press(hwnd: isize, from: (i32, i32), to: (i32, i32)) {
    post_mouse(hwnd, WM_LBUTTONDOWN, from);
    post_mouse(hwnd, WM_MOUSEMOVE, to);
}

/// ポインターが `at`（クライアントの物理ピクセル）に乗った入力を、ポインターの入力のキューに直に積む（ホバーの画面を撮る
/// `--play-test` 用）。WM_MOUSEMOVE を投げると、`TrackMouseEvent` が実際のカーソルの位置で WM_MOUSELEAVE を返し、ホバーが消える。
pub fn push_hover(at: (i32, i32)) {
    let (x, y) = (at.0 as f32, at.1 as f32);
    push(PointerEvent::Move { x, y });
}

/// `at` で左ボタンを離す。
pub fn post_release(hwnd: isize, at: (i32, i32)) {
    post_mouse(hwnd, WM_LBUTTONUP, at);
}

/// Esc を押すメッセージを投げる（ドラッグの取り消しの自動試験用）。
pub fn post_escape(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd as *mut _)),
            WM_KEYDOWN,
            WPARAM(usize::from(VK_ESCAPE.0)),
            LPARAM(0),
        );
    }
}

fn post_mouse(hwnd: isize, msg: u32, (x, y): (i32, i32)) {
    let lparam = LPARAM((((y & 0xFFFF) << 16) | (x & 0xFFFF)) as isize);
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(0), lparam);
    }
}

/// 押し続けの繰り返しか。Space の押し続けで再生と停止を繰り返さないよう、繰り返しはズームと移動のキー
/// （`keys::repeats`）だけをキー操作にする。
fn is_key_repeat(lparam: LPARAM) -> bool {
    lparam.0 & KEY_REPEAT_BIT != 0
}

fn push(ev: PointerEvent) {
    if let Some(shared) = SHARED.get() {
        let mut q = shared.input.lock().expect("input");
        if q.len() < INPUT_LIMIT {
            q.push(ev);
        }
    }
}

fn client_xy(lparam: LPARAM) -> (f32, f32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i16;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16;
    (f32::from(x), f32::from(y))
}

fn track_leave(hwnd: HWND) {
    if TRACKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut tme = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    unsafe {
        let _ = TrackMouseEvent(&mut tme);
    }
}

fn wheel_event(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> PointerEvent {
    let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16;
    // WM_MOUSEWHEEL の座標は画面の座標なので、クライアントの座標に直す。
    let (sx, sy) = client_xy(lparam);
    let mut p = POINT {
        x: sx as i32,
        y: sy as i32,
    };
    unsafe {
        let _ = ScreenToClient(hwnd, &mut p);
    }
    PointerEvent::Wheel {
        x: p.x as f32,
        y: p.y as f32,
        dy: f32::from(delta) / WHEEL_DELTA,
        ctrl: wparam.0 & MK_CONTROL != 0,
        shift: wparam.0 & MK_SHIFT != 0,
    }
}

/// `WM_SYSKEYDOWN` と `WM_SYSKEYUP` のうち、既定のウィンドウプロシージャに渡すキー。Alt+F4（ウィンドウを閉じる）と、
/// F10（離したときにメニューモードに入る。Windows の標準のキー）。
fn passes_to_system(vk: u32) -> bool {
    vk == u32::from(VK_F4.0) || vk == u32::from(VK_F10.0)
}

fn is_escape(wparam: WPARAM) -> bool {
    wparam.0 as u32 == u32::from(VK_ESCAPE.0)
}

/// キーの押下をキーの表に渡す（押し続けの繰り返しは、繰り返すキーだけ）。キーの表にあるキーなら（繰り返しでも）true を返す。
fn key_pressed(wparam: WPARAM, lparam: LPARAM) -> bool {
    let name = key_name(wparam.0 as u32);
    if !is_key_repeat(lparam) || keys::repeats(&name) {
        if let Some(shared) = SHARED.get() {
            keys::dispatch(shared, "surface", &name);
        }
    }
    keys::command_for(&name).is_some()
}

/// キーを離したことをキーの表に渡し（ドラッグ中のスナップの反転を戻す）、既定のウィンドウプロシージャにもそのまま渡す。
fn key_released(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(shared) = SHARED.get() {
        keys::release(
            shared,
            &keyboard::key_name(wparam.0 as u32, keyboard::NO_MODS),
        );
    }
    unsafe { DefWindowProcW(hwnd, WM_KEYUP, wparam, lparam) }
}

/// 共有状態。仕切りのポインター操作（`splitter.rs`）が割り付けを読み書きするのに使う。
pub fn shared() -> Option<&'static Arc<Shared>> {
    SHARED.get()
}

/// Timeline の操作より先に受けるメッセージ。カーソルとツールチップ（`pointer_hint.rs`）、仕切りの操作（`splitter.rs`）。
fn intercept(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let shared = SHARED.get()?;
    pointer_hint::on_message(&shared.hint, hwnd, (msg, wparam, lparam))
        .or_else(|| splitter::message(hwnd, msg, lparam))
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(done) = intercept(hwnd, msg, wparam, lparam) {
        return done;
    }
    match msg {
        // 子 HWND は自動ではフォーカスを取らないので、クリックで自分に移す。ドラッグ中は外に出ても追う。
        WM_LBUTTONDOWN => {
            unsafe {
                let _ = SetFocus(Some(hwnd));
                SetCapture(hwnd);
            }
            hotpatch::set_input(true);
            let (x, y) = client_xy(lparam);
            push(PointerEvent::Down {
                x,
                y,
                mods: pointer_mods(),
            });
            LRESULT(0)
        }
        // 離したことを先に送る。キャプチャーを放すと続けて WM_CAPTURECHANGED（取り消し）が届くため。
        WM_LBUTTONUP => {
            push(PointerEvent::Up);
            unsafe {
                let _ = ReleaseCapture();
            }
            hotpatch::set_input(false);
            LRESULT(0)
        }
        // 離す前にキャプチャーを失ったとき（Esc、別のウィンドウが取った）は、ドラッグを確定せずに取り消す。
        WM_CAPTURECHANGED => {
            push(PointerEvent::Cancel);
            hotpatch::set_input(false);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            track_leave(hwnd);
            let (x, y) = client_xy(lparam);
            push(PointerEvent::Move { x, y });
            LRESULT(0)
        }
        WM_CONTEXTMENU => {
            push(context_menu::requested(hwnd, lparam));
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            TRACKING.store(false, Ordering::SeqCst);
            push(PointerEvent::Leave);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            push(wheel_event(hwnd, wparam, lparam));
            LRESULT(0)
        }
        // ドラッグ中の Esc はキャプチャーを放してドラッグを取り消し、キーの表には渡さない。
        WM_KEYDOWN if is_escape(wparam) && unsafe { GetCapture() } == hwnd => {
            unsafe {
                let _ = ReleaseCapture();
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            key_pressed(wparam, lparam);
            LRESULT(0)
        }
        WM_KEYUP => key_released(hwnd, wparam, lparam),
        // Alt を押している間のキーと F10 は WM_SYSKEYDOWN で届く。Alt+F4 と F10 は既定のウィンドウプロシージャに渡す。
        WM_SYSKEYDOWN if !passes_to_system(wparam.0 as u32) => {
            SYSKEY_IN_TABLE.store(key_pressed(wparam, lparam), Ordering::SeqCst);
            LRESULT(0)
        }
        // キーの表が受けた Alt+英字はアクセスキーとして扱わず、表に無い Alt+英字は既定のウィンドウプロシージャがメニューを開く。
        WM_SYSCHAR if SYSKEY_IN_TABLE.swap(false, Ordering::SeqCst) => LRESULT(0),
        // Alt を離したときの既定のウィンドウプロシージャはメニューモード（SC_KEYMENU）に入り、次のキーを奪うので渡さない。
        WM_SYSKEYUP if !passes_to_system(wparam.0 as u32) => LRESULT(0),
        WM_ERASEBKGND => LRESULT(1),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 押し続けの繰り返しは最初の押下と見分ける() {
        assert!(!is_key_repeat(LPARAM(1)), "最初の押下は直前のビットが 0");
        assert!(is_key_repeat(LPARAM((1 << 30) | 1)));
    }

    #[test]
    fn システムのキーは_f4_と_f10_だけを既定のウィンドウプロシージャに渡す() {
        assert!(passes_to_system(u32::from(VK_F4.0)));
        assert!(
            passes_to_system(u32::from(VK_F10.0)),
            "F10 でメニューモードに入る"
        );
        assert!(
            !passes_to_system(u32::from(VK_MENU.0)),
            "Alt を離してもメニューモードに入らない"
        );
        assert!(!passes_to_system(0x4B), "Alt+K はキーの表へ");
    }

    #[test]
    fn ime_を外すのはその_hwnd_だけで_親と隣の窓の入力のコンテキストは変えない() {
        use windows::Win32::UI::Input::Ime::{ImmGetContext, ImmReleaseContext};
        use windows::Win32::UI::WindowsAndMessaging::{
            DestroyWindow, WINDOW_EX_STYLE, WINDOW_STYLE,
        };
        let window = |parent: Option<HWND>| {
            let style = parent.map_or(WINDOW_STYLE::default(), |_| WS_CHILD);
            let class = w!("STATIC");
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    class,
                    w!("ime"),
                    style,
                    0,
                    0,
                    10,
                    10,
                    parent,
                    None,
                    None,
                    None,
                )
            }
            .expect("試験の HWND")
        };
        let has_context = |hwnd: HWND| unsafe {
            let context = ImmGetContext(hwnd);
            let _ = ImmReleaseContext(hwnd, context);
            !context.is_invalid()
        };
        // サーフェスと WebView の HWND は同じ親の子どうし。
        let parent = window(None);
        let (surface, sibling) = (window(Some(parent)), window(Some(parent)));
        let before = (has_context(parent), has_context(sibling));
        detach_ime(surface);
        let after = (has_context(parent), has_context(sibling));
        let detached = !has_context(surface);
        let _ = unsafe { DestroyWindow(parent) };
        assert!(detached, "IME のコンテキストが残っている");
        assert_eq!(after, before, "親と隣の窓の IME は変えない");
    }

    #[test]
    fn 押したキーがキーの表にあるかを返し_押し続けでも同じ() {
        let first = LPARAM(1);
        let repeat = LPARAM((1 << 30) | 1);
        assert!(key_pressed(WPARAM(0x4B), first), "K は Stop");
        assert!(
            key_pressed(WPARAM(0x4B), repeat),
            "押し続けも WM_SYSCHAR を渡さない"
        );
        assert!(!key_pressed(WPARAM(0x51), first), "Q は表に無い");
    }
}
