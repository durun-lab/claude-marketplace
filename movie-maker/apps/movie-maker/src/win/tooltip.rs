//! サーフェスの部品（ツールバー、再生の操作のバー、見出しのボタン、帯）のツールチップ。Win32 のトラッキングのツールチップ 1 つを
//! UI のスレッドに持ち、ポインターの下の部品が変わったら文と位置を差し替える（帯は拡大とスクロールで動くので、部品ごとに
//! 対象の矩形を登録しない）。最前面のポップアップで作るので、子 WebView の上にも出る。出す時機は `dwell.rs`、文は `text.rs`。
//! 先例は Windows の標準のツールチップ（待ち時間は `TTM_GETDELAYTIME` の既定）。

mod dwell;
mod text;

use std::cell::RefCell;

use mm_ui_layout::TooltipContent;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, TOOLTIPS_CLASSW, TTDT_AUTOPOP,
    TTDT_INITIAL, TTDT_RESHOW, TTF_ABSOLUTE, TTF_TRACK, TTM_ADDTOOLW, TTM_GETBUBBLESIZE,
    TTM_GETDELAYTIME, TTM_SETMAXTIPWIDTH, TTM_TRACKACTIVATE, TTM_TRACKPOSITION, TTM_UPDATETIPTEXTW,
    TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, KillTimer, SendMessageW, SetTimer, SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, WINDOW_STYLE, WS_EX_TOPMOST, WS_POPUP,
};

use self::dwell::{Delays, Dwell, TooltipAction};
pub use self::text::{hint_text, text_of};
use crate::win::pointer_hint::HintSlot;

/// 出すまでのタイマーと、消すまでのタイマーの ID。
const TIMER_SHOW: usize = 0x4D31;
const TIMER_POP: usize = 0x4D32;
/// ツールチップの最大の幅（論理ピクセル）。説明が長ければ折り返す。
const MAX_WIDTH: i32 = 360;
/// ポインターの下からツールチップの上端までの距離と、上に出したときのポインターとの間隔（論理ピクセル）。
const GAP_BELOW: i32 = 22;
const GAP_ABOVE: i32 = 6;
/// 基準の DPI。
const BASE_DPI: i32 = 96;

/// 出したツールチップの記録。位置は画面の座標。`--play-test` が確かめる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub text: String,
    pub at: (i32, i32),
}

/// 画面の矩形（左、上、右、下）。
pub type Bounds = (i32, i32, i32, i32);

/// ツールチップの左上。ポインターの下に出し、画面の右に掛かれば左へ寄せ、下に入らなければポインターの上に出す。
/// 画面の外へは出さない。`scale` は表示の倍率。
pub fn place(anchor: (i32, i32), size: (i32, i32), work: Bounds, scale: f32) -> (i32, i32) {
    let px = |v: i32| (v as f32 * scale).round() as i32;
    let (left, top, right, bottom) = work;
    let x = anchor.0.min(right - size.0).max(left);
    let below = anchor.1 + px(GAP_BELOW);
    let y = if below + size.1 <= bottom {
        below
    } else {
        anchor.1 - px(GAP_ABOVE) - size.1
    };
    (x, y.max(top))
}

/// UI のスレッドが持つツールチップ。
#[derive(Default)]
struct State {
    window: Option<HWND>,
    dwell: Dwell,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn tool_info(owner: HWND, text: PWSTR) -> TTTOOLINFOW {
    TTTOOLINFOW {
        // v5 の構造体の大きさ（lParam の手前まで）。古い comctl32 も受ける。
        cbSize: std::mem::offset_of!(TTTOOLINFOW, lParam) as u32,
        uFlags: TTF_TRACK | TTF_ABSOLUTE,
        hwnd: owner,
        uId: 1,
        lpszText: text,
        ..Default::default()
    }
}

fn send(window: HWND, msg: u32, wparam: usize, info: &mut TTTOOLINFOW) -> isize {
    unsafe {
        SendMessageW(
            window,
            msg,
            Some(WPARAM(wparam)),
            Some(LPARAM(info as *mut _ as isize)),
        )
        .0
    }
}

/// 最前面のツールチップを作る。作れなければ `None`（ツールチップだけが出ない）。
fn create(owner: HWND) -> Option<HWND> {
    unsafe {
        let init = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        };
        // 登録済みでも成功する。失敗すればウィンドウを作れないので、その結果で分かる。
        let _ = InitCommonControlsEx(&init);
        let hinstance = GetModuleHandleW(PCWSTR::null()).ok()?;
        let window = CreateWindowExW(
            WS_EX_TOPMOST,
            TOOLTIPS_CLASSW,
            None,
            WS_POPUP | WINDOW_STYLE(TTS_NOPREFIX | TTS_ALWAYSTIP),
            0,
            0,
            0,
            0,
            Some(owner),
            None,
            Some(hinstance.into()),
            None,
        )
        .ok()?;
        // 作ったあとにも最前面を指定する（親のウィンドウの背面に回らないよう）。
        let _ = SetWindowPos(
            window,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        let mut empty = [0u16];
        let mut info = tool_info(owner, PWSTR(empty.as_mut_ptr()));
        send(window, TTM_ADDTOOLW, 0, &mut info);
        let max = max_width(owner);
        SendMessageW(window, TTM_SETMAXTIPWIDTH, None, Some(LPARAM(max as isize)));
        Some(window)
    }
}

fn scale_of(owner: HWND) -> f32 {
    let dpi = unsafe { GetDpiForWindow(owner) } as i32;
    if dpi <= 0 {
        1.0
    } else {
        dpi as f32 / BASE_DPI as f32
    }
}

fn max_width(owner: HWND) -> i32 {
    (MAX_WIDTH as f32 * scale_of(owner)).round() as i32
}

/// Windows の既定の待ち時間。
fn delays(window: HWND) -> Delays {
    let get = |which| unsafe {
        let ms = SendMessageW(window, TTM_GETDELAYTIME, Some(WPARAM(which as usize)), None).0;
        u32::try_from(ms).unwrap_or(0)
    };
    Delays {
        initial: get(TTDT_INITIAL),
        reshow: get(TTDT_RESHOW),
        autopop: get(TTDT_AUTOPOP),
    }
}

/// 画面の座標 `p` のあるモニターの作業領域。
fn work_area(p: POINT) -> Bounds {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let ok = unsafe {
        let monitor = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        GetMonitorInfoW(monitor, &mut info).as_bool()
    };
    let r: RECT = if ok { info.rcWork } else { RECT::default() };
    (r.left, r.top, r.right, r.bottom)
}

/// ツールチップを `tip` の文で、ポインターの位置 `anchor`（サーフェスのクライアント座標）の下に出す。
fn show(window: HWND, owner: HWND, tip: &TooltipContent, anchor: (f32, f32), hint: &HintSlot) {
    let mut text: Vec<u16> = text_of(tip).encode_utf16().chain(Some(0)).collect();
    let mut at = POINT {
        x: anchor.0.round() as i32,
        y: anchor.1.round() as i32,
    };
    // クライアントの座標に出すのは物理ピクセルで、画面の座標への変換が失敗しても出さないだけにする。
    if !unsafe { ClientToScreen(owner, &mut at) }.as_bool() {
        return;
    }
    let mut info = tool_info(owner, PWSTR(text.as_mut_ptr()));
    send(window, TTM_UPDATETIPTEXTW, 0, &mut info);
    let bubble = send(window, TTM_GETBUBBLESIZE, 0, &mut info) as u32;
    let size = ((bubble & 0xFFFF) as i32, (bubble >> 16) as i32);
    let (x, y) = place((at.x, at.y), size, work_area(at), scale_of(owner));
    unsafe {
        SendMessageW(
            window,
            TTM_TRACKPOSITION,
            None,
            Some(LPARAM(make_long(x, y))),
        );
    }
    send(window, TTM_TRACKACTIVATE, 1, &mut info);
    hint.record_shown(Shown {
        text: String::from_utf16_lossy(&text[..text.len() - 1]),
        at: (x, y),
    });
}

/// `MAKELONG(x, y)`。負の座標も下位 16 ビットで渡す。
fn make_long(x: i32, y: i32) -> isize {
    ((y as i16 as u16 as isize) << 16) | (x as i16 as u16 as isize)
}

fn hide(window: HWND, owner: HWND) {
    let mut empty = [0u16];
    let mut info = tool_info(owner, PWSTR(empty.as_mut_ptr()));
    send(window, TTM_TRACKACTIVATE, 0, &mut info);
}

/// タイマーを掛ける・止める・ツールチップを出す・隠す。
fn run(actions: Vec<TooltipAction>, window: HWND, owner: HWND, hint: &HintSlot) {
    for action in actions {
        match action {
            TooltipAction::Show(tip) => {
                let anchor = hint.current().at.unwrap_or_default();
                show(window, owner, &tip, anchor, hint);
            }
            TooltipAction::Hide => hide(window, owner),
            TooltipAction::ArmShow(ms) => arm(owner, TIMER_SHOW, ms),
            TooltipAction::ArmPop(ms) => arm(owner, TIMER_POP, ms),
            TooltipAction::Disarm => unsafe {
                // タイマーが無ければ失敗するだけなので、結果は破棄する。
                let _ = KillTimer(Some(owner), TIMER_SHOW);
                let _ = KillTimer(Some(owner), TIMER_POP);
            },
        }
    }
}

fn arm(owner: HWND, id: usize, ms: u32) {
    // 0 のタイマーは Windows が 1 ms に直す。失敗すれば出ないだけ。
    let _ = unsafe { SetTimer(Some(owner), id, ms.max(1), None) };
}

fn with_window<R>(owner: HWND, f: impl FnOnce(&mut Dwell, HWND) -> R) -> Option<R> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.window.is_none() {
            s.window = create(owner);
        }
        let window = s.window?;
        Some(f(&mut s.dwell, window))
    })
}

/// ポインターの下の部品の、ツールチップの中身が変わった（`WM_MM_HINT`）。
pub fn target(owner: HWND, hint: &HintSlot) {
    let tip = hint.current().tooltip;
    let actions = with_window(owner, |dwell, window| {
        (dwell.target(tip, delays(window)), window)
    });
    if let Some((actions, window)) = actions {
        run(actions, window, owner, hint);
    }
}

/// タイマーの時間が来た。`id` が出すタイマーか消すタイマーかで進める。
pub fn timer(owner: HWND, id: usize, hint: &HintSlot) -> bool {
    if id != TIMER_SHOW && id != TIMER_POP {
        return false;
    }
    let actions = with_window(owner, |dwell, window| {
        let actions = if id == TIMER_SHOW {
            dwell.show_timer(delays(window))
        } else {
            dwell.pop_timer()
        };
        (actions, window)
    });
    if let Some((actions, window)) = actions {
        run(actions, window, owner, hint);
    }
    true
}

/// 押した、ホイールを回した、キーを押した、ポインターが出た。出ていれば隠す。
pub fn dismiss(owner: HWND, hint: &HintSlot) {
    let actions = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let window = s.window?;
        Some((s.dwell.dismiss(), window))
    });
    if let Some((actions, window)) = actions {
        run(actions, window, owner, hint);
    }
}

#[cfg(test)]
mod tests;
