//! ウィンドウの割り付け。Tauri のウィンドウを作り、その下の左に wgpu のサーフェス（子 HWND。
//! Program と Timeline）、右に子 WebView（`ui/dist/` の shell と再生の操作列）を並べる。

use std::sync::Arc;

use tauri::webview::WebviewBuilder;
use tauri::window::WindowBuilder;
use tauri::{PhysicalPosition, PhysicalSize, Position, Rect, Size, Webview, WebviewUrl, Window};

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

/// サーフェスと WebView の矩形（物理ピクセル）をウィンドウの大きさから決める。
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

/// ウィンドウの下に、サーフェスの子 HWND と子 WebView を並べる。サーフェスの HWND を返す。
pub fn create_window(
    app: &mut tauri::App,
    shared: &Arc<Shared>,
) -> Result<(Window, Webview, isize), Box<dyn std::error::Error>> {
    let window = WindowBuilder::new(app, "main")
        .title("movie-maker")
        .inner_size(INITIAL_SIZE.0, INITIAL_SIZE.1)
        .build()?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!((s.3, v.3), (h as i32, h as i32));
        }
    }
}
