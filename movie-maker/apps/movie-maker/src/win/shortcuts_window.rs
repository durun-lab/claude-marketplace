//! Keyboard Shortcuts のウィンドウ。メインウィンドウをオーナーにした別のウィンドウ（`ui/dist/shortcuts.html`）をコードで作り、
//! メインウィンドウの中央の上に出す。閉じたら、開く前にフォーカスを持っていたメインウィンドウの子 HWND にフォーカスを戻す。

use std::sync::atomic::{AtomicIsize, Ordering};

use tauri::webview::WebviewWindowBuilder;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, Window, WindowEvent};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetGUIThreadInfo, GetWindowThreadProcessId, IsChild, GUITHREADINFO,
};

use crate::win::surface;

/// ウィンドウの label。
const LABEL: &str = "shortcuts";
/// ページ（`ui/dist/`）。
const ENTRY: &str = "shortcuts.html";
/// ウィンドウの中の大きさと、その下限（論理ピクセル）。
const SIZE: (f64, f64) = (560.0, 640.0);
const MIN_SIZE: (f64, f64) = (360.0, 320.0);

/// 開く前にメインウィンドウでフォーカスを持っていた子 HWND（サーフェスか WebView）。閉じると Windows はオーナーの
/// メインウィンドウを前面に出してフレームにフォーカスを置くので、戻さないとキーがサーフェスにも WebView にも届かない。
static FOCUS_BEFORE: AtomicIsize = AtomicIsize::new(0);

/// 矩形（x、y、幅、高さ。物理ピクセル）。
pub type Rect = (i32, i32, u32, u32);

/// 大きさ `size` のウィンドウの中心を `main` の中心に合わせ、作業領域 `area` からはみ出す分だけ寄せた左上の位置。
/// 作業領域より大きければ作業領域の左上に置き、タイトルバーを画面の中に残す。
pub fn place(main: Rect, size: (u32, u32), area: Option<Rect>) -> (i32, i32) {
    let center =
        |at: i32, len: u32, inner: u32| at + (i64::from(len) - i64::from(inner)) as i32 / 2;
    let (x, y) = (
        center(main.0, main.2, size.0),
        center(main.1, main.3, size.1),
    );
    let Some((ax, ay, aw, ah)) = area else {
        return (x, y);
    };
    let fit = |v: i32, at: i32, len: u32, inner: u32| v.min(at + len as i32 - inner as i32).max(at);
    (fit(x, ax, aw, size.0), fit(y, ay, ah, size.1))
}

/// Keyboard Shortcuts のウィンドウを出す。開いていれば前面に出すだけ。ウィンドウの生成はメインスレッドの返りを待つので、
/// メニューのイベントと IPC のスレッドを止めないよう別のスレッドで作る。
pub fn open(app: &AppHandle, main: &Window) {
    remember_focus(main);
    if let Some(existing) = app.get_webview_window(LABEL) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return;
    }
    let (app, main) = (app.clone(), main.clone());
    let spawned = std::thread::Builder::new()
        .name("shortcuts".into())
        .spawn(move || {
            if let Err(e) = build(&app, &main) {
                tracing::warn!(error = %e, "Keyboard Shortcuts のウィンドウを作れない");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("Keyboard Shortcuts のスレッドを起こせない: {e}");
    }
}

/// メインウィンドウをオーナーにして、メインウィンドウの前に出し続ける（メインウィンドウを閉じると一緒に閉じる）。
/// メインウィンドウの中心で隠したまま作り、そのモニターの倍率で大きさを決めてから、外形の大きさで中央に置いて出す。
fn build(app: &AppHandle, main: &Window) -> tauri::Result<()> {
    let scale = main.scale_factor()?;
    let (pos, outer) = (main.outer_position()?, main.outer_size()?);
    let main_rect = (pos.x, pos.y, outer.width, outer.height);
    let at = PhysicalPosition::new(
        pos.x + outer.width as i32 / 2,
        pos.y + outer.height as i32 / 2,
    )
    .to_logical::<f64>(scale);
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(ENTRY.into()))
        .title("Keyboard Shortcuts")
        .position(at.x, at.y)
        .inner_size(SIZE.0, SIZE.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .maximizable(false)
        .minimizable(false)
        .owner_raw(main.hwnd()?)
        .visible(false)
        .build()?;
    let area = main.current_monitor()?.map(|m| {
        let a = m.work_area();
        (a.position.x, a.position.y, a.size.width, a.size.height)
    });
    let size = window.outer_size()?;
    let (x, y) = place(main_rect, (size.width, size.height), area);
    window.set_position(PhysicalPosition::new(x, y))?;
    window.show()?;
    window.set_focus()?;
    restore_focus_on_close(&window, main);
    Ok(())
}

/// 閉じたら（Enter の実行の後、Esc、閉じるボタンのどれでも）フォーカスをメインウィンドウの子 HWND に戻す。
fn restore_focus_on_close(window: &WebviewWindow, main: &Window) {
    let main = main.clone();
    window.on_window_event(move |e| {
        if matches!(e, WindowEvent::Destroyed) {
            restore_focus(&main);
        }
    });
}

/// Tauri の `Window` の HWND を、このクレートの `windows` の版の HWND にする（Tauri は別の版の型を返す）。
fn hwnd_of(window: &Window) -> Option<HWND> {
    window.hwnd().ok().map(|h| HWND(h.0 as isize as *mut _))
}

/// メインウィンドウでフォーカスを持っている子 HWND を覚える。呼んだスレッド（メニュー、キー、IPC のどれか）によらず、
/// メインウィンドウのスレッドの入力の状態から引く。フレームにフォーカスがあれば覚えない（閉じたらサーフェスに戻す）。
fn remember_focus(main: &Window) {
    let Some(main) = hwnd_of(main) else { return };
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        let thread = GetWindowThreadProcessId(main, None);
        if GetGUIThreadInfo(thread, &mut info).is_ok() && IsChild(main, info.hwndFocus).as_bool() {
            FOCUS_BEFORE.store(info.hwndFocus.0 as isize, Ordering::SeqCst);
        }
    }
}

/// 閉じた後にメインウィンドウが前面に戻っていれば、覚えた子 HWND（無ければサーフェス）にフォーカスを戻す。
/// ほかのアプリに移っていれば触らない。Tauri はウィンドウのイベントをメインスレッドで呼ぶので、ここで `SetFocus` できる。
fn restore_focus(main: &Window) {
    let saved = HWND(FOCUS_BEFORE.swap(0, Ordering::SeqCst) as *mut _);
    let Some(main) = hwnd_of(main) else { return };
    unsafe {
        if GetActiveWindow() != main {
            return;
        }
        let target = if IsChild(main, saved).as_bool() {
            Some(saved)
        } else {
            FindWindowExW(Some(main), None, surface::CLASS, None).ok()
        };
        if let Some(target) = target {
            let _ = SetFocus(Some(target));
        }
    }
}

/// Keyboard Shortcuts のウィンドウを閉じる。ページが Enter（実行の後）と Esc で呼ぶ。
#[tauri::command]
pub async fn close_shortcuts(app: AppHandle) -> Result<(), String> {
    match app.get_webview_window(LABEL) {
        Some(window) => window.close().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 外形の中心をメインウィンドウの中心に合わせる() {
        // 外形は 125% の倍率の 560x640 論理ピクセルに、タイトルバーと縁を足した大きさ。
        assert_eq!(place((100, 50, 1600, 900), (718, 847), None), (541, 76));
        // 作業領域に収まっていれば寄せない。
        let area = Some((0, 0, 2560, 1528));
        assert_eq!(place((0, 0, 2560, 1528), (718, 847), area), (921, 340));
        // 150% の倍率（840x960 の中と縁）でも同じ式。
        assert_eq!(place((0, 0, 2560, 1528), (858, 1007), area), (851, 260));
    }

    #[test]
    fn 作業領域からはみ出す分だけ寄せ_大きすぎれば左上に置く() {
        let area = Some((0, 0, 1920, 1040));
        // メインウィンドウが小さく左上にあると、中心に合わせた位置は画面の外になる。
        assert_eq!(place((0, 0, 400, 300), (718, 847), area), (0, 0));
        // 右下に寄せたメインウィンドウなら、作業領域の右下の角に収める。
        assert_eq!(place((1700, 900, 400, 300), (718, 847), area), (1202, 193));
        // 作業領域より大きいウィンドウは左上（タイトルバーを画面に残す）。
        assert_eq!(place((0, 0, 1920, 1040), (2000, 1200), area), (0, 0));
        // 2 枚目のモニター（左が負）でも、そのモニターの作業領域に収める。
        let left = Some((-1920, 0, 1920, 1040));
        assert_eq!(place((-2100, 100, 600, 400), (718, 847), left), (-1920, 0));
    }
}
