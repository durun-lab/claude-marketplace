//! サーフェスの上の 2 つの仕切りのポインター操作。サーフェスと WebView の境界（縦。サーフェスの右端の帯）と、Program と
//! Timeline の境界（横）。どちらも押した所を `mm-ui-layout` の `splitter_at` で当たり判定し、ドラッグ中はキャプチャーして
//! 割り付け（`workspace.rs`）を動かす。離したときに設定に書き、キャプチャーを失ったとき（Esc）は押す前に戻す。
//! 仕切りの色は描画スレッドが [`hot`] を読んで変える。カーソルは仕切りの上で左右・上下の矢印にする。

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

use mm_ui_layout::{content_width, splitter_at, splitter_rect, Splitter, SPLITTER_W};
use mm_ui_wgpu::{SplitterState, Ui};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetParent, LoadCursorW, SetCursor, IDC_SIZENS, IDC_SIZEWE,
    WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_SETCURSOR,
};

use crate::win::hotpatch;
use crate::win::layout;
use crate::win::state::Shared;
use crate::win::surface;
use crate::win::workspace::Workspace;

/// マウスがウィンドウから出た通知（`TrackMouseEvent` で頼んだもの）。
const WM_MOUSELEAVE: u32 = 0x02A3;
/// `WM_SETCURSOR` の下位語で、クライアント領域の上を示す値。
const HTCLIENT: isize = 1;

/// 乗っているか、つかんでいる仕切り。0 は無し、1 は縦、2 は横。描画スレッドが読む。
static HOT: AtomicU8 = AtomicU8::new(0);
/// つかんでいる仕切り。
static DRAG: Mutex<Option<Drag>> = Mutex::new(None);

/// つかんでいる仕切りと、つかんだ位置（仕切りの見た目の線の上端か左端からのずれ。論理ピクセル）と、つかむ前の割り付け。
#[derive(Debug, Clone, Copy)]
struct Drag {
    which: Splitter,
    grab: f32,
    before: Workspace,
}

/// サーフェスと親のウィンドウの大きさ（論理ピクセル）。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Geo {
    w: f32,
    h: f32,
    window_w: f64,
}

fn hot_code(which: Option<Splitter>) -> u8 {
    match which {
        None => 0,
        Some(Splitter::Vertical) => 1,
        Some(Splitter::Horizontal) => 2,
    }
}

fn set_hot(which: Option<Splitter>) {
    HOT.store(hot_code(which), Ordering::SeqCst);
}

/// ポインターが乗っているか、つかんでいる仕切り。
pub fn hot() -> Option<Splitter> {
    match HOT.load(Ordering::SeqCst) {
        1 => Some(Splitter::Vertical),
        2 => Some(Splitter::Horizontal),
        _ => None,
    }
}

/// 描画スレッドが毎フレーム、割り付けと仕切りの色を UI に渡す。
pub fn sync_ui(shared: &Shared, ui: &mut Ui) {
    let ws = shared.workspace.get();
    ui.set_splitters(SplitterState {
        timeline_ratio: ws.timeline_ratio,
        vertical: ws.has_web(),
        hot: hot(),
    });
}

fn drag() -> Option<Drag> {
    *DRAG.lock().expect("splitter drag")
}

fn client_size(hwnd: HWND) -> Option<(f64, f64)> {
    let mut r = RECT::default();
    unsafe { GetClientRect(hwnd, &mut r) }.ok()?;
    Some((f64::from(r.right - r.left), f64::from(r.bottom - r.top)))
}

/// サーフェスと親のウィンドウの大きさ。親が無い（試験の HWND）か、大きさが 0 なら `None`。
fn geometry(hwnd: HWND) -> Option<(Geo, f32)> {
    let scale = surface::scale_factor(hwnd.0 as isize);
    let (w, h) = client_size(hwnd)?;
    let (window_w, _) = client_size(unsafe { GetParent(hwnd) }.ok()?)?;
    let s = f64::from(scale);
    let geo = Geo {
        w: (w / s) as f32,
        h: (h / s) as f32,
        window_w: window_w / s,
    };
    (geo.w > 0.0 && geo.h > 0.0).then_some((geo, scale))
}

fn xy(lparam: LPARAM, scale: f32) -> (f32, f32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i16;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16;
    (f32::from(x) / scale, f32::from(y) / scale)
}

fn hit(ws: &Workspace, geo: Geo, (x, y): (f32, f32)) -> Option<Splitter> {
    splitter_at(geo.w, geo.h, ws.timeline_ratio, ws.has_web(), x, y)
}

/// つかんだ位置 `grab` を保ったまま、ポインターが `(x, y)` にあるときの割り付けにする。
fn dragged(drag: &Drag, ws: &Workspace, geo: Geo, (x, y): (f32, f32)) -> Workspace {
    let mut next = *ws;
    match drag.which {
        Splitter::Vertical => {
            // 仕切りの左端が `x - grab`。WebView はその右の `SPLITTER_W` の後ろから右端まで。
            let web_start = f64::from(x - drag.grab + SPLITTER_W);
            next.drag_web(geo.window_w - web_start, geo.window_w);
        }
        Splitter::Horizontal => {
            // 仕切りの上端が `y - grab`。Timeline はその下の隙間の後ろから下端まで。
            let gap = splitter_rect(content_width(geo.w, ws.has_web()), geo.h, 0.0).h;
            next.drag_timeline(geo.h - (y - drag.grab + gap), geo.h);
        }
    }
    next
}

/// 仕切りの見た目の線の上端か左端から、押した位置までのずれ。
fn grab_offset(ws: &Workspace, geo: Geo, which: Splitter, (x, y): (f32, f32)) -> f32 {
    match which {
        Splitter::Vertical => x - (geo.w - SPLITTER_W),
        Splitter::Horizontal => {
            y - splitter_rect(content_width(geo.w, ws.has_web()), geo.h, ws.timeline_ratio).y
        }
    }
}

/// サーフェスの `WM_LBUTTONDOWN`、`WM_MOUSEMOVE`、`WM_LBUTTONUP`、`WM_CAPTURECHANGED`、`WM_MOUSELEAVE`、`WM_SETCURSOR` を先に受ける。
/// 仕切りの操作なら `Some` で、サーフェスの Timeline の操作には渡さない。
pub fn message(hwnd: HWND, msg: u32, lparam: LPARAM) -> Option<LRESULT> {
    let shared = surface::shared()?;
    match msg {
        WM_LBUTTONDOWN => press(hwnd, shared, lparam),
        WM_MOUSEMOVE => moved(hwnd, shared, lparam),
        WM_LBUTTONUP => release(shared),
        WM_CAPTURECHANGED => cancel(shared),
        WM_MOUSELEAVE => leave(),
        WM_SETCURSOR => cursor(hwnd, shared, lparam),
        _ => None,
    }
}

fn press(hwnd: HWND, shared: &Shared, lparam: LPARAM) -> Option<LRESULT> {
    let (geo, scale) = geometry(hwnd)?;
    let ws = shared.workspace.get();
    let at = xy(lparam, scale);
    let which = hit(&ws, geo, at)?;
    *DRAG.lock().expect("splitter drag") = Some(Drag {
        which,
        grab: grab_offset(&ws, geo, which, at),
        before: ws,
    });
    set_hot(Some(which));
    hotpatch::set_input(true);
    unsafe {
        let _ = SetFocus(Some(hwnd));
        SetCapture(hwnd);
    }
    Some(LRESULT(0))
}

fn moved(hwnd: HWND, shared: &Shared, lparam: LPARAM) -> Option<LRESULT> {
    let (geo, scale) = geometry(hwnd)?;
    let ws = shared.workspace.get();
    let at = xy(lparam, scale);
    let Some(drag) = drag() else {
        // ホバー。Timeline の操作にもそのまま渡す。
        set_hot(hit(&ws, geo, at));
        return None;
    };
    let next = dragged(&drag, &ws, geo, at);
    if next != ws {
        shared.workspace.set(next);
        if drag.which == Splitter::Vertical {
            layout::refresh();
        }
    }
    Some(LRESULT(0))
}

fn release(shared: &Shared) -> Option<LRESULT> {
    // キャプチャーを放すと続けて WM_CAPTURECHANGED が届くので、先に外して取り消しとして扱わせない。
    let was = DRAG.lock().expect("splitter drag").take()?;
    unsafe {
        let _ = ReleaseCapture();
    }
    hotpatch::set_input(false);
    set_hot(None);
    if shared.workspace.get() != was.before {
        shared.workspace.save();
    }
    Some(LRESULT(0))
}

/// つかんだままキャプチャーを失った（Esc、別のウィンドウが取った）ときは、押す前に戻す。
fn cancel(shared: &Shared) -> Option<LRESULT> {
    let was = DRAG.lock().expect("splitter drag").take()?;
    hotpatch::set_input(false);
    set_hot(None);
    shared.workspace.set(was.before);
    layout::refresh();
    Some(LRESULT(0))
}

fn leave() -> Option<LRESULT> {
    if drag().is_none() {
        set_hot(None);
    }
    None
}

fn cursor(hwnd: HWND, shared: &Shared, lparam: LPARAM) -> Option<LRESULT> {
    if lparam.0 & 0xFFFF != HTCLIENT {
        return None;
    }
    let which = match drag() {
        Some(d) => d.which,
        None => {
            let (geo, scale) = geometry(hwnd)?;
            let mut p = POINT::default();
            unsafe {
                GetCursorPos(&mut p).ok()?;
                let _ = ScreenToClient(hwnd, &mut p);
            }
            hit(
                &shared.workspace.get(),
                geo,
                (p.x as f32 / scale, p.y as f32 / scale),
            )?
        }
    };
    let id = match which {
        Splitter::Vertical => IDC_SIZEWE,
        Splitter::Horizontal => IDC_SIZENS,
    };
    unsafe {
        if let Ok(c) = LoadCursorW(None, id) {
            SetCursor(Some(c));
        }
    }
    Some(LRESULT(1))
}

#[cfg(test)]
mod tests;
