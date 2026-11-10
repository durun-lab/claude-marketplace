//! サーフェスのカーソル。仕切りの上では仕切りの形（`mm_ui_layout::splitter_cursor`）、ほかではポインターの下のもの
//! （`Pointee`）と今の修飾キーから決め（`mm_ui_layout::cursor_for`）、ここで Windows のカーソルにする。左右と上下の矢印と禁止は
//! 標準のカーソル（`IDC_SIZEWE`、`IDC_SIZENS`、`IDC_NO`）、roll、slip、slide、刃は `art.rs` で描いた絵から表示の倍率ごとに作る
//! （カーソルの大きさは `GetSystemMetricsForDpi` で、倍率 100% で 32、125% で 40、150% で 48）。

mod art;

use std::collections::HashMap;
use std::sync::Mutex;

use mm_ui_layout::{cursor_for, splitter_cursor, CursorKind, Mods};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, LoadCursorW, SetCursor, HCURSOR, ICONINFO, IDC_ARROW, IDC_NO, IDC_SIZENS,
    IDC_SIZEWE, SM_CXCURSOR,
};

use crate::win::pointer_hint::HintSlot;
use crate::win::splitter;

/// 作ったカーソル（形と大きさごと）。プロセスの終わりまで持つ。
static CACHE: Mutex<Option<HashMap<(u8, u32), isize>>> = Mutex::new(None);

/// 表示の倍率 `dpi` でのカーソルの一辺（画素）。
fn cursor_size(dpi: u32) -> u32 {
    u32::try_from(unsafe { GetSystemMetricsForDpi(SM_CXCURSOR, dpi) })
        .ok()
        .filter(|s| *s > 0)
        .unwrap_or(32)
}

fn stock(id: PCWSTR) -> Option<HCURSOR> {
    unsafe { LoadCursorW(None, id) }.ok()
}

/// 絵から Windows のカーソルを作る。色は上から並べた BGRA の 32 ビット、マスクは使わない（全部 0）。
fn create(kind: CursorKind, size: u32) -> Option<HCURSOR> {
    let art = art::render(kind, size);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            // 負の高さは上の行から並ぶ形。
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(color.0));
            return None;
        }
        std::ptr::copy_nonoverlapping(art.bgra.as_ptr(), bits.cast::<u8>(), art.bgra.len());
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, None);
        let icon = ICONINFO {
            fIcon: false.into(),
            xHotspot: art.hot.0,
            yHotspot: art.hot.1,
            hbmMask: mask,
            hbmColor: color,
        };
        let made = CreateIconIndirect(&icon);
        // アイコンは画像を複製して持つので、元のビットマップは捨てる。
        let _ = DeleteObject(HGDIOBJ(color.0));
        let _ = DeleteObject(HGDIOBJ(mask.0));
        made.ok().map(|i| HCURSOR(i.0))
    }
}

/// 形 `kind` のカーソル。自前の絵は倍率ごとに 1 度だけ作る。
fn handle(kind: CursorKind, dpi: u32) -> Option<HCURSOR> {
    match kind {
        CursorKind::Arrow => stock(IDC_ARROW),
        CursorKind::SizeWE => stock(IDC_SIZEWE),
        CursorKind::SizeNS => stock(IDC_SIZENS),
        CursorKind::No => stock(IDC_NO),
        CursorKind::Roll | CursorKind::Slip | CursorKind::Slide | CursorKind::Blade => {
            let size = cursor_size(dpi);
            let mut cache = CACHE.lock().expect("cursor cache");
            let cache = cache.get_or_insert_with(HashMap::new);
            if let Some(h) = cache.get(&(kind as u8, size)) {
                return Some(HCURSOR(*h as *mut _));
            }
            let made = create(kind, size)?;
            cache.insert((kind as u8, size), made.0 as isize);
            Some(made)
        }
    }
}

/// いまのポインターの下と修飾キー `mods` のカーソルを `hwnd` の上に出す。仕切りの上か、仕切りをつかんでいる間は仕切りの形。
/// 標準でも自前でも作れなければ false（既定に任せる）。
pub fn apply(hint: &HintSlot, hwnd: HWND, mods: Mods) -> bool {
    let kind = splitter::cursor_target(hwnd).map_or_else(
        || cursor_for(&hint.current().pointee, mods),
        splitter_cursor,
    );
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    match handle(kind, dpi) {
        Some(h) => {
            unsafe { SetCursor(Some(h)) };
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 自前のカーソルは倍率ごとの大きさで作れ_同じ形は同じカーソルを返す() {
        for dpi in [96, 120, 144] {
            for kind in [
                CursorKind::Roll,
                CursorKind::Slip,
                CursorKind::Slide,
                CursorKind::Blade,
            ] {
                let a = handle(kind, dpi).expect("カーソルを作れる");
                let b = handle(kind, dpi).expect("カーソルを作れる");
                assert_eq!(a.0, b.0, "{kind:?} {dpi}");
            }
        }
        assert!(cursor_size(96) <= cursor_size(144));
    }

    #[test]
    fn 標準のカーソルは_windows_のものを返す() {
        for kind in [
            CursorKind::Arrow,
            CursorKind::SizeWE,
            CursorKind::SizeNS,
            CursorKind::No,
        ] {
            assert!(handle(kind, 96).is_some(), "{kind:?}");
        }
    }

    /// Windows のカーソルの色の画像の大きさと、ホットスポット。
    fn bitmap_and_hotspot(h: HCURSOR) -> ((i32, i32), (u32, u32)) {
        use windows::Win32::Graphics::Gdi::{GetObjectW, BITMAP};
        use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, HICON};
        let mut info = ICONINFO::default();
        unsafe { GetIconInfo(HICON(h.0), &mut info) }.expect("カーソルの情報");
        let mut bm = BITMAP::default();
        let len = std::mem::size_of::<BITMAP>() as i32;
        let ptr = (&mut bm as *mut BITMAP).cast();
        let got = unsafe { GetObjectW(HGDIOBJ(info.hbmColor.0), len, Some(ptr)) };
        // GetIconInfo が作った画像は呼んだ側が捨てる。
        unsafe {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        assert!(got > 0, "色の画像");
        ((bm.bmWidth, bm.bmHeight), (info.xHotspot, info.yHotspot))
    }

    #[test]
    fn 自前のカーソルは倍率のカーソルの大きさで_ホットスポットは中央() {
        for dpi in [96, 120, 144] {
            let size = cursor_size(dpi);
            for kind in [CursorKind::Roll, CursorKind::Blade] {
                let h = handle(kind, dpi).expect("カーソルを作れる");
                let (wh, hot) = bitmap_and_hotspot(h);
                let side = size as i32;
                assert_eq!(wh, (side, side), "{kind:?} {dpi}");
                assert_eq!(hot, (size / 2, size / 2), "{kind:?} {dpi}");
            }
        }
    }
}
