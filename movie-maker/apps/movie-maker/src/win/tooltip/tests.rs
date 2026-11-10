use super::*;
use windows::Win32::UI::WindowsAndMessaging::{
    IsWindowVisible, WINDOW_EX_STYLE, WINDOW_STYLE, WS_CHILD, WS_VISIBLE,
};

const WORK: Bounds = (0, 0, 1920, 1080);

#[test]
fn 位置はポインターの下で_右に掛かれば左へ寄せ_下に入らなければ上に出す() {
    let size = (200, 40);
    assert_eq!(place((100, 100), size, WORK, 1.0), (100, 122), "下");
    assert_eq!(
        place((1900, 100), size, WORK, 1.0),
        (1720, 122),
        "右の端に掛からない"
    );
    let (x, y) = place((500, 1070), size, WORK, 1.0);
    assert_eq!(x, 500);
    assert!(y + size.1 <= 1070, "ポインターの上: {y}");
    assert_eq!(
        place((10, 2), (50, 20), (0, 0, 100, 30), 1.0).1,
        0,
        "上へ逃がしても画面の外に出ない"
    );
}

#[test]
fn 表示の倍率に合わせて_ポインターとの距離を広げる() {
    let at = |scale| place((100, 100), (50, 20), WORK, scale).1 - 100;
    assert_eq!(at(1.0), GAP_BELOW);
    assert_eq!(at(1.5), (GAP_BELOW as f32 * 1.5).round() as i32);
}

#[test]
fn 複数の画面の左にある負の座標の作業領域でも_左の端に収める() {
    let work = (-1920, 0, 0, 1080);
    let (x, _) = place((-10, 100), (200, 40), work, 1.0);
    assert_eq!(x, -200);
}

#[test]
fn 最前面のツールチップを作り_出した文と位置を記録し_隠せる() {
    let parent = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            windows::core::w!("STATIC"),
            windows::core::w!("tooltip parent"),
            WINDOW_STYLE::default() | WS_VISIBLE,
            100,
            100,
            400,
            300,
            None,
            None,
            None,
            None,
        )
    }
    .expect("試験の親");
    let surface = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            windows::core::w!("STATIC"),
            windows::core::w!("tooltip surface"),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            400,
            300,
            Some(parent),
            None,
            None,
            None,
        )
    }
    .expect("試験の子");
    let hint = HintSlot::default();
    hint.set(mm_ui_wgpu::Hint {
        tooltip: Some(TooltipContent::Command("timeline.blade_tool")),
        at: Some((50.0, 60.0)),
        ..Default::default()
    });
    let window = create(surface).expect("ツールチップを作れる");
    let tip = TooltipContent::Command("timeline.blade_tool");
    show(window, surface, &tip, (50.0, 60.0), &hint);
    let shown = hint.shown();
    assert_eq!(shown.len(), 1);
    assert!(
        shown[0].text.starts_with("Blade Tool (B)\n"),
        "{:?}",
        shown[0]
    );
    // 出した位置は、サーフェスの左上（画面の座標）にポインターの位置を足した点の下。
    let mut origin = POINT::default();
    let _ = unsafe { ClientToScreen(surface, &mut origin) };
    assert_eq!(shown[0].at.0, origin.x + 50, "ポインターの x に合わせる");
    assert!(shown[0].at.1 >= origin.y + 60, "ポインターの下");
    assert!(unsafe { IsWindowVisible(window) }.as_bool(), "出ている");
    hide(window, surface);
    assert!(!unsafe { IsWindowVisible(window) }.as_bool(), "隠れた");
    let ms = delays(window);
    assert!(ms.initial > 0 && ms.autopop >= ms.initial, "{ms:?}");
    let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(parent) };
}
