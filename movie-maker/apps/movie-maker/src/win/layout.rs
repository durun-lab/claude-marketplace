//! ウィンドウの割り付け。メニューバーつきの Tauri のウィンドウを作り、その下の左に wgpu のサーフェス（子 HWND。
//! Program と Timeline）、右に子 WebView（`ui/dist/` の shell の 2 列と、キーと再生位置の中継スクリプト）を並べる。

use std::sync::Arc;

use tauri::webview::WebviewBuilder;
use tauri::window::WindowBuilder;
use tauri::{
    LogicalUnit, PhysicalPosition, PhysicalSize, PixelUnit, Position, Rect, Size, Webview,
    WebviewUrl, Window, WindowSizeConstraints,
};

use crate::win::commands;
use crate::win::menu;
use crate::win::state::Shared;
use crate::win::surface::{self, PixelRect};
use crate::win::theme;

/// ウィンドウの初期の大きさ（論理ピクセル）。
const INITIAL_SIZE: (f64, f64) = (1600.0, 900.0);
/// サーフェスの幅の下限（論理ピクセル）。WebView の 2 列は幅を変えないので、ウィンドウの幅の下限はこれと 2 列の和。
/// Program の再生の操作のバーのタイムコードとボタンが 1 行に入る幅。
const MIN_SURFACE_W: f64 = 400.0;
/// WebView が最初に読むページ。`ui/dist/` の shell。
const UI_ENTRY: &str = "index.html";
/// WebView に入れる中継スクリプト。WebView にフォーカスがあるときのキーと、再生位置と選択のイベントをつなぐ。
const TRANSPORT_JS: &str = include_str!("../../web/transport.js");

/// 配置を直すのに要るもの。
pub struct Layout {
    pub shared: Arc<Shared>,
    pub surface_hwnd: isize,
}

/// サーフェスと WebView の矩形（物理ピクセル）を、ウィンドウの大きさと表示の倍率から決める。WebView は 2 列の幅の和
/// （theme.json の `left-column` と `right-column`）を取り、サーフェスは残りを取る。ウィンドウを広げた分は Program と
/// Timeline に回る。メニューバーは非クライアント領域にあり、`inner_size` も子ウィンドウの位置もその下のクライアント領域で
/// 測るので、メニューバーの高さは引かない。
pub fn split_rects(width: u32, height: u32, scale: f64) -> (PixelRect, PixelRect) {
    let (w, h) = (width as i32, height as i32);
    let web = ((theme::webview_width() * scale).round() as i32).clamp(0, w);
    let left = w - web;
    ((0, 0, left, h), (left, 0, web, h))
}

/// ウィンドウの幅の下限。サーフェスが潰れないよう、WebView の 2 列の幅にサーフェスの幅の下限を足す。
fn size_constraints() -> WindowSizeConstraints {
    WindowSizeConstraints {
        min_width: Some(PixelUnit::from(LogicalUnit::new(
            theme::webview_width() + MIN_SURFACE_W,
        ))),
        ..WindowSizeConstraints::default()
    }
}

/// ウィンドウの今の大きさに、サーフェスと WebView の矩形を合わせる。
pub fn apply(window: &Window, webview: &Webview, l: &Layout) {
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    let (s, v) = split_rects(size.width, size.height, scale);
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
        .inner_size_constraints(size_constraints())
        .menu(bar)
        .build()?;
    menu::connect(app, &window, shared, items);
    let size = window.inner_size()?;
    let (s, v) = split_rects(size.width, size.height, window.scale_factor()?);
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
        for (w, h, scale) in [
            (1600, 1000, 1.0),
            (1125, 750, 1.0),
            (2400, 1350, 1.5),
            (1, 1, 1.0),
        ] {
            let (s, v) = split_rects(w, h, scale);
            assert_eq!(
                s.0 + s.2,
                v.0,
                "サーフェスの右端と WebView の左端が一致する"
            );
            assert_eq!(v.0 + v.2, w as i32);
            assert!(s.2 >= 0 && v.2 >= 0, "{w}x{h}: {s:?} {v:?}");
            assert_eq!((s.1, v.1), (0, 0), "どちらもメニューバーのすぐ下から");
            assert_eq!((s.3, v.3), (h as i32, h as i32));
        }
    }

    #[test]
    fn webview_は_2_列の幅の和を取り_サーフェスがウィンドウの残りを取る() {
        let web = theme::webview_width();
        assert_eq!(web, 660.0);
        // 既定のウィンドウ（幅 1600）でサーフェスは 940（約 59%）、幅 1920 で 1260（約 66%）。
        assert_eq!(
            split_rects(1600, 900, 1.0),
            ((0, 0, 940, 900), (940, 0, 660, 900))
        );
        assert_eq!(split_rects(1920, 1080, 1.0).0, (0, 0, 1260, 1080));
        // 表示の倍率が 150% なら、同じ論理ピクセルの幅を物理ピクセルで取る。
        assert_eq!(
            split_rects(2400, 1350, 1.5),
            ((0, 0, 1410, 1350), (1410, 0, 990, 1350))
        );
        // ウィンドウが 2 列より狭くても、矩形は負にならない（幅の下限はウィンドウが持つ）。
        assert_eq!(
            split_rects(500, 400, 1.0),
            ((0, 0, 0, 400), (0, 0, 500, 400))
        );
    }

    #[test]
    fn ウィンドウの幅の下限は_2_列とサーフェスの下限の和で_高さは決めない() {
        let c = size_constraints();
        assert_eq!(
            c.min_width,
            Some(PixelUnit::from(LogicalUnit::new(
                theme::webview_width() + MIN_SURFACE_W
            )))
        );
        assert_eq!(c.min_height, None);
        assert!(INITIAL_SIZE.0 >= theme::webview_width() + MIN_SURFACE_W);
    }

    #[test]
    fn 中継スクリプトがボタンから送らない編集のキーはキーの表の編集のキーと同じ() {
        let start = TRANSPORT_JS
            .find("const EDIT_KEYS = new Set([")
            .expect("中継スクリプトに EDIT_KEYS がある");
        let body = &TRANSPORT_JS[start..];
        let list = &body[..body.find("]);").expect("EDIT_KEYS の終わり")];
        let mut bridge: Vec<&str> = list.split('"').skip(1).step_by(2).collect();
        let mut table: Vec<&str> = commands::TABLE
            .iter()
            .filter(|s| {
                ["clip.", "edit.", "source."]
                    .iter()
                    .any(|p| s.id.starts_with(p))
            })
            .flat_map(|s| crate::win::keys::keys_of(s.id))
            .collect();
        bridge.sort_unstable();
        table.sort_unstable();
        assert_eq!(bridge, table);
    }
}
