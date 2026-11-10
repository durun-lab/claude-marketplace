//! ウィンドウの割り付け。メニューバーつきの Tauri のウィンドウを作り、その下の左に wgpu のサーフェス（子 HWND。
//! Program と Timeline）、右に子 WebView（`ui/dist/` の shell と再生の操作列）を並べる。

use std::sync::Arc;

use tauri::webview::{WebviewBuilder, WebviewWindowBuilder};
use tauri::window::WindowBuilder;
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Position, Rect, Size, Webview, WebviewUrl,
    Window,
};

use crate::win::commands;
use crate::win::menu;
use crate::win::state::Shared;
use crate::win::surface::{self, PixelRect};

/// サーフェスの幅の割合。残りが WebView。
const SPLIT: f64 = 0.5;
/// ウィンドウの初期の大きさ（論理ピクセル）。
const INITIAL_SIZE: (f64, f64) = (1600.0, 900.0);
/// WebView が最初に読むページ。`ui/dist/` の shell。
const UI_ENTRY: &str = "index.html";
/// WebView に重ねる再生の操作列。
const TRANSPORT_JS: &str = include_str!("../../web/transport.js");

/// 配置を直すのに要るもの。
pub struct Layout {
    pub shared: Arc<Shared>,
    pub surface_hwnd: isize,
}

/// サーフェスと WebView の矩形（物理ピクセル）をウィンドウの大きさから決める。メニューバーは非クライアント領域にあり、
/// `inner_size` も子ウィンドウの位置もその下のクライアント領域で測るので、メニューバーの高さは引かない。
pub fn split_rects(width: u32, height: u32) -> (PixelRect, PixelRect) {
    let left = (f64::from(width) * SPLIT).round() as i32;
    let (w, h) = (width as i32, height as i32);
    ((0, 0, left, h), (left, 0, w - left, h))
}

/// ウィンドウの今の大きさに、サーフェスと WebView の矩形を合わせる。
pub fn apply(window: &Window, webview: &Webview, l: &Layout) {
    let Ok(size) = window.inner_size() else {
        return;
    };
    let (s, v) = split_rects(size.width, size.height);
    surface::set_rect(l.surface_hwnd, s);
    let _ = webview.set_bounds(Rect {
        position: Position::Physical(PhysicalPosition::new(v.0, v.1)),
        size: Size::Physical(PhysicalSize::new(v.2.max(1) as u32, v.3.max(1) as u32)),
    });
    l.shared
        .set_surface_size(s.2.max(1) as u32, s.3.max(1) as u32);
}

/// メニューバーつきのウィンドウの下に、サーフェスの子 HWND と子 WebView を並べる。サーフェスの HWND を返す。
/// メニューバーはウィンドウを作るときに付ける。後から付けると、最初の矩形をメニューバーの分だけ高く測るため。
pub fn create_window(
    app: &mut tauri::App,
    shared: &Arc<Shared>,
) -> Result<(Window, Webview, isize), Box<dyn std::error::Error>> {
    let (bar, items) = menu::build(app.handle(), &commands::table(shared))?;
    let window = WindowBuilder::new(app, "main")
        .title("movie-maker")
        .inner_size(INITIAL_SIZE.0, INITIAL_SIZE.1)
        .menu(bar)
        .build()?;
    menu::connect(app, &window, shared, items);
    let size = window.inner_size()?;
    let (s, v) = split_rects(size.width, size.height);
    let surface_hwnd = surface::create(window.hwnd()?.0 as isize, shared.clone(), s)?;
    let webview = window.add_child(
        WebviewBuilder::new("ui", WebviewUrl::App(UI_ENTRY.into()))
            .initialization_script(TRANSPORT_JS),
        PhysicalPosition::new(v.0, v.1),
        PhysicalSize::new(v.2 as u32, v.3 as u32),
    )?;
    shared.set_surface_size(s.2 as u32, s.3 as u32);
    Ok((window, webview, surface_hwnd))
}

/// Keyboard Shortcuts のウィンドウの label。
pub const SHORTCUTS_LABEL: &str = "shortcuts";
/// Keyboard Shortcuts のページ（`ui/dist/`）。
const SHORTCUTS_ENTRY: &str = "shortcuts.html";
/// Keyboard Shortcuts のウィンドウの大きさ（論理ピクセル）。
const SHORTCUTS_SIZE: (f64, f64) = (560.0, 640.0);
const SHORTCUTS_MIN_SIZE: (f64, f64) = (360.0, 320.0);

/// 本体のウィンドウ `(x, y, 幅, 高さ)` の中央に、大きさ `size` のウィンドウの中心を合わせたときの左上の位置。
pub fn centered_over(main: (i32, i32, u32, u32), size: (u32, u32)) -> (i32, i32) {
    let (x, y, w, h) = main;
    (
        x + (i64::from(w) - i64::from(size.0)) as i32 / 2,
        y + (i64::from(h) - i64::from(size.1)) as i32 / 2,
    )
}

/// Keyboard Shortcuts のウィンドウを本体の中央の上に出す。開いていれば前に出すだけ。ウィンドウの生成は
/// メインスレッドの返りを待つので、メニューのイベントと IPC のスレッドを止めないよう別のスレッドで作る。
pub fn open_shortcuts(app: &AppHandle, main: &Window) {
    if let Some(existing) = app.get_webview_window(SHORTCUTS_LABEL) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return;
    }
    let (app, main) = (app.clone(), main.clone());
    let spawned = std::thread::Builder::new()
        .name("shortcuts".into())
        .spawn(move || {
            if let Err(e) = build_shortcuts(&app, &main) {
                tracing::warn!(error = %e, "Keyboard Shortcuts のウィンドウを作れない");
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("Keyboard Shortcuts のスレッドを起こせない: {e}");
    }
}

/// 本体をオーナーにして、本体の前に出し続ける。位置は作った後に置く（作る前の位置は論理座標で、画面の縮尺が違うと外れる）。
fn build_shortcuts(app: &AppHandle, main: &Window) -> tauri::Result<()> {
    let scale = main.scale_factor()?;
    let size = (
        (SHORTCUTS_SIZE.0 * scale).round() as u32,
        (SHORTCUTS_SIZE.1 * scale).round() as u32,
    );
    let (pos, outer) = (main.outer_position()?, main.outer_size()?);
    let (x, y) = centered_over((pos.x, pos.y, outer.width, outer.height), size);
    let url = WebviewUrl::App(SHORTCUTS_ENTRY.into());
    let window = WebviewWindowBuilder::new(app, SHORTCUTS_LABEL, url)
        .title("Keyboard Shortcuts")
        .inner_size(SHORTCUTS_SIZE.0, SHORTCUTS_SIZE.1)
        .min_inner_size(SHORTCUTS_MIN_SIZE.0, SHORTCUTS_MIN_SIZE.1)
        .maximizable(false)
        .minimizable(false)
        .owner_raw(main.hwnd()?)
        .focused(true)
        .build()?;
    window.set_position(PhysicalPosition::new(x, y))
}

/// Keyboard Shortcuts のウィンドウを閉じる。ページが Enter（実行の後）と Esc で呼ぶ。
#[tauri::command]
pub async fn close_shortcuts(app: AppHandle) -> Result<(), String> {
    match app.get_webview_window(SHORTCUTS_LABEL) {
        Some(window) => window.close().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_のウィンドウは本体の中央に重なる() {
        assert_eq!(centered_over((100, 50, 1600, 900), (560, 640)), (620, 180));
        assert_eq!(centered_over((0, 0, 400, 300), (560, 640)), (-80, -170));
    }

    #[test]
    fn サーフェスと_webview_は隙間も重なりもなくウィンドウを埋める() {
        for (w, h) in [(1600, 1000), (1125, 750), (2400, 1350), (1, 1)] {
            let (s, v) = split_rects(w, h);
            assert_eq!(
                s.0 + s.2,
                v.0,
                "サーフェスの右端と WebView の左端が一致する"
            );
            assert_eq!(v.0 + v.2, w as i32);
            assert_eq!((s.1, v.1), (0, 0), "どちらもメニューバーのすぐ下から");
            assert_eq!((s.3, v.3), (h as i32, h as i32));
        }
    }
}
