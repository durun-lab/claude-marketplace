//! ウィンドウの割り付け。メニューバーつきの Tauri のウィンドウを作り、その下の左に wgpu のサーフェス（子 HWND。
//! Program と Timeline。右端に WebView との仕切りの帯を含む）、右に子 WebView（`ui/dist/` の shell の 2 列と、キーと
//! 再生位置の中継スクリプト）を並べる。WebView の幅と列の開け閉めはユーザーの割り付け（`workspace.rs`）が決める。

use std::sync::{Arc, Mutex, OnceLock};

use tauri::webview::{PageLoadEvent, PageLoadPayload, WebviewBuilder};
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
use crate::win::workspace::{Workspace, MIN_SURFACE_W};

/// ウィンドウの初期の大きさ（論理ピクセル）。
const INITIAL_SIZE: (f64, f64) = (1600.0, 900.0);
/// WebView が最初に読むページ。`ui/dist/` の shell。
const UI_ENTRY: &str = "index.html";
/// WebView に入れる中継スクリプト。WebView にフォーカスがあるときのキーと、再生位置と選択のイベントをつなぐ。
const TRANSPORT_JS: &str = include_str!("../../web/transport.js");

/// 配置を直すのに要るもの。
pub struct Layout {
    pub shared: Arc<Shared>,
    pub surface_hwnd: isize,
    /// WebView に最後に送った列の見た目。変わったときだけ送り直す。
    sent: Mutex<String>,
}

impl Layout {
    pub fn new(shared: Arc<Shared>, surface_hwnd: isize) -> Self {
        Self {
            shared,
            surface_hwnd,
            sent: Mutex::new(String::new()),
        }
    }
}

/// サーフェスと WebView の矩形（物理ピクセル）を、ウィンドウの大きさと表示の倍率と割り付けから決める。WebView は割り付けの
/// 列の幅の和を取り、サーフェスは残りを取る（閉じた列の分だけサーフェスが広がる）。ウィンドウを広げた分は Program と
/// Timeline に回る。メニューバーは非クライアント領域にあり、`inner_size` も子ウィンドウの位置もその下のクライアント領域で
/// 測るので、メニューバーの高さは引かない。
pub fn split_rects(
    width: u32,
    height: u32,
    scale: f64,
    workspace: &Workspace,
) -> (PixelRect, PixelRect) {
    let (w, h) = (width as i32, height as i32);
    let web_w = workspace.columns(f64::from(width) / scale).web();
    let web = ((web_w * scale).round() as i32).clamp(0, w);
    let left = w - web;
    ((0, 0, left, h), (left, 0, web, h))
}

/// ウィンドウの幅の下限。サーフェスが潰れないよう、WebView の 2 列の既定の幅に、サーフェスの幅の下限と縦の仕切りの帯を足す。
fn size_constraints() -> WindowSizeConstraints {
    WindowSizeConstraints {
        min_width: Some(PixelUnit::from(LogicalUnit::new(
            theme::webview_width() + MIN_SURFACE_W + f64::from(mm_ui_layout::SPLITTER_W),
        ))),
        ..WindowSizeConstraints::default()
    }
}

/// WebView の 2 列に渡す見た目。右の列の幅（左の列は残り）と、列の開け閉め。CSS の変数と属性で渡し、シェルの CSS が
/// 読む。ページを読み込む前は `documentElement` が無いので、読み込み後にも当てる。
fn shell_script(workspace: &Workspace, window_w: f64) -> String {
    let columns = workspace.columns(window_w);
    let state = |open: bool| if open { "open" } else { "closed" };
    format!(
        "{{const a=()=>{{const r=document.documentElement;if(!r)return;\
         r.style.setProperty('--mm-right-w','{}px');r.dataset.mmLeft='{}';r.dataset.mmRight='{}';}};\
         a();document.addEventListener('DOMContentLoaded',a);}}",
        columns.right,
        state(workspace.left_open),
        state(workspace.right_open),
    )
}

/// ウィンドウの今の大きさと割り付けに、サーフェスと WebView の矩形を合わせる。
pub fn apply(window: &Window, webview: &Webview, l: &Layout) {
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    let ws = l.shared.workspace.get();
    let (s, v) = split_rects(size.width, size.height, scale, &ws);
    surface::set_rect(l.surface_hwnd, s);
    let _ = webview.set_bounds(Rect {
        position: Position::Physical(PhysicalPosition::new(v.0, v.1)),
        size: Size::Physical(PhysicalSize::new(v.2.max(1) as u32, v.3.max(1) as u32)),
    });
    l.shared
        .set_surface_size(s.2.max(1) as u32, s.3.max(1) as u32);
    let script = shell_script(&ws, f64::from(size.width) / scale);
    let last = std::mem::replace(&mut *l.sent.lock().expect("layout sent"), script.clone());
    if last != script {
        let _ = webview.eval(&script);
    }
}

/// 割り付けを変えたとき（縦の仕切りのドラッグ、列の開け閉め、Reset UI Layout）と WebView がページを読み直したときに、
/// ウィンドウの配置を直すのに要るもの。
struct Installed {
    window: Window,
    webview: Webview,
    layout: Arc<Layout>,
}

static INSTALLED: OnceLock<Installed> = OnceLock::new();

/// ウィンドウの配置を直すのに要るものを設定する。2 回目からは無視する。
pub fn install_refresh(window: Window, webview: Webview, layout: Arc<Layout>) {
    let _ = INSTALLED.set(Installed {
        window,
        webview,
        layout,
    });
}

/// 割り付けに合わせてウィンドウの配置を直す。ウィンドウを作る前は何もしない。
pub fn refresh() {
    if let Some(i) = INSTALLED.get() {
        apply(&i.window, &i.webview, &i.layout);
    }
}

/// WebView がページを読み終えたら、列の見た目を送り直す。初期化のスクリプトはウィンドウを作ったときの割り付けを
/// 持ったままなので、読み直し（F5 など）の後は今の割り付けに戻す。
fn on_page_load(_: Webview, payload: PageLoadPayload<'_>) {
    if payload.event() != PageLoadEvent::Finished {
        return;
    }
    if let Some(i) = INSTALLED.get() {
        i.layout.sent.lock().expect("layout sent").clear();
        apply(&i.window, &i.webview, &i.layout);
    }
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
    let scale = window.scale_factor()?;
    let ws = shared.workspace.get();
    let (s, v) = split_rects(size.width, size.height, scale, &ws);
    let surface_hwnd = surface::create(window.hwnd()?.0 as isize, shared.clone(), s)?;
    let init = format!(
        "{TRANSPORT_JS}\n{}",
        shell_script(&ws, f64::from(size.width) / scale)
    );
    let webview = window.add_child(
        WebviewBuilder::new("ui", WebviewUrl::App(UI_ENTRY.into()))
            .initialization_script(init)
            .on_page_load(on_page_load),
        PhysicalPosition::new(v.0, v.1),
        PhysicalSize::new(v.2.max(1) as u32, v.3 as u32),
    )?;
    shared.set_surface_size(s.2 as u32, s.3 as u32);
    Ok((window, webview, surface_hwnd))
}

#[cfg(test)]
mod tests;
