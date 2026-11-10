//! キー配列（Windows のキーボード レイアウト）で、サーフェスの `WM_KEYDOWN` と `WM_KEYUP` の仮想キーをキーの名前にする。
//! code の名前のキーは仮想キーの名前、記号のキーは今のキー配列で押された文字にする（名前の決まりは `keys.rs`）。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, ToUnicodeEx, HKL, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

use crate::win::keys;

/// 修飾キー（Ctrl、Shift、Alt）をどれも押していない状態。離したキーは、修飾キーを先に離しても戻るようこの状態の名前にする。
pub const NO_MODS: (bool, bool, bool) = (false, false, false);
/// キーの状態の配列で、押しているキーを示す上位ビット。
const KEY_PRESSED: u8 = 0x80;
/// `ToUnicodeEx` の flags の、キーボードの状態（デッドキーの途中など）を変えずに文字にする指定（Windows 10 1607 から）。
const KEEP_KEYBOARD_STATE: u32 = 0x4;

/// このスレッドの今のキー配列で、修飾キー `mods`（Ctrl、Shift、Alt）を前に付けたキーの名前。
pub fn key_name(vk: u32, mods: (bool, bool, bool)) -> String {
    key_name_in(vk, mods, unsafe { GetKeyboardLayout(0) })
}

/// 修飾キー（Ctrl、Shift、Alt）を前に付けたキーの名前。code の名前のキーは仮想キーの名前、記号のキーはキー配列
/// `keyboard` で押された文字。
pub fn key_name_in(vk: u32, (ctrl, shift, alt): (bool, bool, bool), keyboard: HKL) -> String {
    let name =
        keys::vk_name(vk).unwrap_or_else(|| keys::symbol_name(vk, typed_char(vk, shift, keyboard)));
    keys::with_modifiers(&name, ctrl, shift, alt)
}

/// HWND の UI スレッドの、今のキー配列。
pub fn keyboard_layout(hwnd: isize) -> HKL {
    unsafe {
        let thread = GetWindowThreadProcessId(HWND(hwnd as *mut _), None);
        GetKeyboardLayout(thread)
    }
}

/// 記号のキーをキー配列 `keyboard` で押したときの文字。Shift だけを押した状態で文字にする（Ctrl を含めると制御文字になる）。
/// デッドキーと、文字にならないキーは `None`。
fn typed_char(vk: u32, shift: bool, keyboard: HKL) -> Option<char> {
    let mut state = [0u8; 256];
    if shift {
        state[usize::from(VK_SHIFT.0)] = KEY_PRESSED;
    }
    let mut buf = [0u16; 4];
    let n = unsafe { ToUnicodeEx(vk, 0, &state, &mut buf, KEEP_KEYBOARD_STATE, Some(keyboard)) };
    let c = (n == 1)
        .then(|| char::from_u32(u32::from(buf[0])))
        .flatten()?;
    (!c.is_control()).then_some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(vk: u32, ctrl: bool, shift: bool) -> String {
        key_name(vk, (ctrl, shift, false))
    }

    #[test]
    fn 英字と矢印はキー配列に依らず_shift_付きの名前_記号は今のキー配列で押された文字() {
        assert_eq!(name(0x5A, false, true), "Shift+KeyZ");
        assert_eq!(name(0x25, false, true), "Shift+ArrowLeft");
        assert_eq!(
            name(0xBD, false, false),
            "-",
            "VK_OEM_MINUS は JIS でも US でも -"
        );
        assert_eq!(name(0xBD, true, false), "Ctrl+-");
        assert_eq!(name(0xBC, false, false), ",");
        assert_eq!(name(0xBE, false, false), ".");
        assert_eq!(
            key_name(0x4E, NO_MODS),
            "KeyN",
            "離した N は修飾キーを付けない"
        );
        // JIS のキー配列（VK_OEM_7 が ^ のキー）の Shift+- は = で、Ctrl+Shift+- も Ctrl+= になる。
        if name(0xDE, false, false) == "^" {
            assert_eq!(name(0xBD, false, true), "=");
            assert_eq!(name(0xBD, true, true), "Ctrl+=");
            assert_eq!(name(0xBB, false, true), "+", "Shift+; は +");
        }
    }
}
