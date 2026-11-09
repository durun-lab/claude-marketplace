//! 再生の操作のバー。映像の真下に時刻とボタンを描き、バーの上のクリックをコマンドとシークにする。

use mm_types::{Rgba, RgbaImage, Size};
use mm_ui_layout::{
    Item, ItemKind, Mods, PointerEvent, Rect, TimelineModel, TransportAction, Viewer, ViewerState,
};
use mm_ui_wgpu::{Region, Ui};

use crate::support::{gpu, px_in, render_sized, FORMAT, H, W};

fn model() -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![Item::layer(0.0, 10.0, ItemKind::Footage, true)],
    }
}

fn down(x: f32, y: f32) -> PointerEvent {
    PointerEvent::Down {
        x,
        y,
        mods: Mods::default(),
    }
}

/// `rect` に、背景の色 `back` と違う画素がいくつあるか。
fn lit(out: &[u8], rect: Rect, back: [u8; 3]) -> usize {
    let mut n = 0;
    for y in rect.y as u32..rect.bottom() as u32 {
        for x in rect.x as u32..rect.right() as u32 {
            if px_in(out, W, x, y) != back {
                n += 1;
            }
        }
    }
    n
}

fn frame() -> RgbaImage {
    RgbaImage::filled(Size::new(16, 9), Rgba::new(0.5, 0.5, 0.5, 1.0))
}

#[test]
fn 映像の下にバーが付き_時刻とボタンが描かれ_source_を開くと_source_の下にも付く() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    ui.set_frame(&device, &queue, &frame());
    ui.set_playhead(2.5);
    let l = ui.layout();
    assert_eq!(l.source_bar, None);
    assert_eq!(l.program_bar.y, l.program.bottom());
    let out = render_sized(&device, &queue, &mut ui, (W, H));
    let bar = l.program_bar;
    let back = px_in(&out, W, bar.x as u32 + 2, bar.bottom() as u32 - 2);
    assert!(lit(&out, bar, back) > 300, "時刻とボタンの画素");

    ui.set_source_frame(&device, &queue, &frame());
    ui.set_transport(ViewerState::default(), Some(ViewerState::default()));
    let bar = ui.layout().source_bar.expect("Source のバー");
    let out = render_sized(&device, &queue, &mut ui, (W, H));
    let back = px_in(&out, W, bar.x as u32 + 2, bar.bottom() as u32 - 2);
    assert!(lit(&out, bar, back) > 300, "Source のバーにも描かれる");
}

#[test]
fn バーの上は_program_と_source_の領域で_クリックはボタンのコマンドとジョグバーのシークになる() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    ui.set_frame(&device, &queue, &frame());
    ui.set_source_frame(&device, &queue, &frame());
    let source = ViewerState {
        duration: 4.0,
        fps: 24.0,
        ..ViewerState::default()
    };
    ui.set_transport(ViewerState::default(), Some(source));
    let l = ui.layout();
    let (pb, sb) = (l.program_bar, l.source_bar.expect("バー"));
    assert_eq!(ui.region_at(pb.x + 2.0, pb.y + 20.0), Region::Program);
    assert_eq!(ui.region_at(sb.x + 2.0, sb.y + 20.0), Region::Source);

    // ジョグバーの中ほどを押すと Program の長さの半分へシークする。
    let track_mid = pb.x + pb.w / 2.0;
    let seek = ui.transport_pointer(down(track_mid, pb.y + 4.0));
    match seek[..] {
        [TransportAction::Seek {
            viewer: Viewer::Program,
            t,
        }] => assert!((t - 5.0).abs() < 0.2, "{t}"),
        _ => panic!("{seek:?}"),
    }
    ui.transport_pointer(PointerEvent::Up);
    // ボタンの行の中央は Play / Stop。
    let actions = ui.transport_pointer(down(track_mid, pb.y + 30.0));
    assert_eq!(actions, [TransportAction::Command("playback.toggle")]);
}
