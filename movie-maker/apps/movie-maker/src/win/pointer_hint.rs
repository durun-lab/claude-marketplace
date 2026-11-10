//! ポインターの下にあるものを、描画のスレッドから UI のスレッドへ渡す 1 つの入口。描画のスレッドの `Ui` が作った
//! [`Hint`]（`Pointee`、ツールチップの中身、ポインターの位置）を [`HintSlot`] に置き、UI のスレッドの `WM_SETCURSOR`、
//! ツールチップ、右クリックのメニューは [`HintSlot::current`] で引く。置いたら `WM_MM_HINT` を投げ、UI のスレッドが
//! ツールチップとカーソルを差し替える。サーフェスのカーソルはここの振り分け 1 つで決め、仕切りの上と仕切りをつかんでいる間は
//! 仕切りの形にする（`cursor::apply`）。

use std::sync::Mutex;

use mm_ui_wgpu::{Hint, StatusText, Ui};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_MENU, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, PostMessageW, WM_APP, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_MBUTTONDOWN, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_SETCURSOR, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_TIMER,
};

use crate::win::state::Shared;
use crate::win::tooltip::{self, Shown};
use crate::win::{cursor, surface};

/// ポインターの下が変わったことを UI のスレッドに知らせるメッセージ。
pub const WM_MM_HINT: u32 = WM_APP + 0x31;
/// `WM_SETCURSOR` の下位語で、クライアント領域の上を示す値（`HTCLIENT`）。
const HTCLIENT: isize = 1;
/// 記録するツールチップの数の上限。
const SHOWN_LIMIT: usize = 64;

/// 描画のスレッドが置いた、ポインターの下の手がかり。
#[derive(Default)]
pub struct HintSlot {
    hint: Mutex<Hint>,
    status: Mutex<StatusText>,
    shown: Mutex<ShownLog>,
}

/// 出したツールチップの記録。上限を超えたら古いものから捨て、捨てた数を数える（後から出したものだけを引けるように）。
#[derive(Default)]
struct ShownLog {
    items: Vec<Shown>,
    dropped: usize,
}

impl HintSlot {
    /// ポインターの下の手がかり。UI のスレッドがカーソル、ツールチップ、右クリックのメニューに使う入口。
    pub fn current(&self) -> Hint {
        self.hint.lock().expect("hint").clone()
    }

    /// 手がかりを置く。前と違えば真。
    pub fn set(&self, hint: Hint) -> bool {
        let mut slot = self.hint.lock().expect("hint");
        let changed = *slot != hint;
        *slot = hint;
        changed
    }

    /// 最後に描いたステータスバーの文（左、右）。
    pub fn status(&self) -> StatusText {
        self.status.lock().expect("status").clone()
    }

    fn set_status(&self, status: StatusText) {
        *self.status.lock().expect("status") = status;
    }

    /// 出したツールチップを記録する（古いものから捨てる）。
    pub fn record_shown(&self, shown: Shown) {
        let mut log = self.shown.lock().expect("shown");
        if log.items.len() >= SHOWN_LIMIT {
            log.items.remove(0);
            log.dropped += 1;
        }
        log.items.push(shown);
    }

    /// これまでに出したツールチップの数（捨てた分を含む）。[`Self::shown_since`] に渡すと、後から出したものだけを引ける。
    pub fn shown_count(&self) -> usize {
        let log = self.shown.lock().expect("shown");
        log.dropped + log.items.len()
    }

    /// `count`（[`Self::shown_count`] の値）の後に出したツールチップの記録。
    pub fn shown_since(&self, count: usize) -> Vec<Shown> {
        let log = self.shown.lock().expect("shown");
        let skip = count.saturating_sub(log.dropped);
        log.items.iter().skip(skip).cloned().collect()
    }
}

/// 描画のスレッドが、今のポインターの下の手がかりとステータスバーの文を置く。手がかりが変わっていれば UI のスレッドへ知らせる。
pub fn publish(shared: &Shared, hwnd: isize, ui: &Ui) {
    shared.hint.set_status(ui.status_text().clone());
    if shared.hint.set(ui.hint()) {
        // ウィンドウが無ければ失敗するだけ。
        let _ =
            unsafe { PostMessageW(Some(HWND(hwnd as *mut _)), WM_MM_HINT, WPARAM(0), LPARAM(0)) };
    }
}

/// ポインターがサーフェスの上にあれば、今の修飾キーのカーソルに替える。Alt と Shift を押した・離したときと、
/// ポインターの下が変わったとき（動かさずにツールを替えたとき）に呼ぶ。
fn refresh_cursor(hint: &HintSlot, hwnd: HWND) {
    let mut p = POINT::default();
    let mut client = RECT::default();
    let inside = unsafe {
        GetCursorPos(&mut p).is_ok()
            && ScreenToClient(hwnd, &mut p).as_bool()
            && GetClientRect(hwnd, &mut client).is_ok()
    } && (client.left..client.right).contains(&p.x)
        && (client.top..client.bottom).contains(&p.y);
    if inside {
        cursor::apply(hint, hwnd, surface::pointer_mods());
    }
}

fn is_modifier(wparam: WPARAM) -> bool {
    wparam.0 == usize::from(VK_SHIFT.0) || wparam.0 == usize::from(VK_MENU.0)
}

/// サーフェスのウィンドウプロシージャが最初に呼ぶ。カーソル、ツールチップのタイマーと差し替えは受けて結果を返し、
/// 押下、ホイール、キーでツールチップを隠すものは通常の処理を続けさせる（`None`）。
pub fn on_message(
    hint: &HintSlot,
    hwnd: HWND,
    (msg, wparam, lparam): (u32, WPARAM, LPARAM),
) -> Option<LRESULT> {
    match msg {
        WM_SETCURSOR if lparam.0 & 0xFFFF == HTCLIENT => {
            cursor::apply(hint, hwnd, surface::pointer_mods()).then_some(LRESULT(1))
        }
        WM_MM_HINT => {
            tooltip::target(hwnd, hint);
            refresh_cursor(hint, hwnd);
            Some(LRESULT(0))
        }
        WM_TIMER => tooltip::timer(hwnd, wparam.0, hint).then_some(LRESULT(0)),
        WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP if is_modifier(wparam) => {
            refresh_cursor(hint, hwnd);
            None
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_MOUSEWHEEL | WM_KEYDOWN
        | WM_SYSKEYDOWN => {
            tooltip::dismiss(hwnd, hint);
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_ui_layout::{TooltipContent, Under};

    #[test]
    fn 置いた手がかりが前と違うときだけ変わったと返し_現在の値を入口で引く() {
        let slot = HintSlot::default();
        let a = Hint {
            tooltip: Some(TooltipContent::Command("edit.undo")),
            ..Hint::default()
        };
        assert!(slot.set(a.clone()), "最初は変わった");
        assert!(!slot.set(a.clone()), "同じなら変わっていない");
        assert_eq!(slot.current(), a);
        let mut b = a;
        b.pointee.under = Under::Outside;
        b.at = Some((1.0, 2.0));
        assert!(slot.set(b));
    }

    #[test]
    fn 出したツールチップの記録は上限を超えたら古いものから捨てる() {
        let slot = HintSlot::default();
        for i in 0..(SHOWN_LIMIT + 3) {
            slot.record_shown(Shown {
                text: i.to_string(),
                at: (0, 0),
            });
        }
        let shown = slot.shown_since(0);
        assert_eq!(shown.len(), SHOWN_LIMIT);
        assert_eq!(shown[0].text, "3");
        let count = slot.shown_count();
        assert_eq!(count, SHOWN_LIMIT + 3, "捨てた分も数える");
        slot.record_shown(Shown {
            text: "new".into(),
            at: (0, 0),
        });
        let later = slot.shown_since(count);
        assert_eq!(
            later.len(),
            1,
            "上限で捨てた後も、後から出したものだけを引ける"
        );
        assert_eq!(later[0].text, "new");
    }

    #[test]
    fn 修飾キーは_shift_と_alt_だけ() {
        assert!(is_modifier(WPARAM(usize::from(VK_SHIFT.0))));
        assert!(is_modifier(WPARAM(usize::from(VK_MENU.0))));
        assert!(!is_modifier(WPARAM(0x41)));
    }
}
