//! 再生の操作のバー。映像の領域の真下にタイムコードとボタンを描き、バーの上のクリックをコマンドとシークにする。

use mm_types::{Rgba, RgbaImage, Size};
use mm_ui_layout::{
    jog_track, transport_button, Item, ItemKind, Mods, PointerEvent, Rect, TimelineModel,
    TransportAction, Viewer, ViewerState,
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
fn 映像の領域の下にバーが付き_タイムコードとボタンが描かれ_source_を開くと_source_の下にも付く() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    ui.set_frame(&device, &queue, &frame());
    ui.set_playhead(2.5);
    let l = ui.layout();
    assert_eq!(l.source_transport, None);
    assert_eq!(l.program_transport.y, l.program.bottom());
    let out = render_sized(&device, &queue, &mut ui, (W, H));
    let bar = l.program_transport;
    let back = px_in(&out, W, bar.x as u32 + 2, bar.bottom() as u32 - 2);
    assert!(lit(&out, bar, back) > 300, "タイムコードとボタンの画素");

    ui.set_source_frame(&device, &queue, &frame());
    ui.set_transport(ViewerState::default(), Some(ViewerState::default()));
    let bar = ui.layout().source_transport.expect("Source のバー");
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
    // 表示の倍率 1 なので、論理ピクセルの位置がそのままサーフェスの座標になる。
    let l = ui.layout();
    let (pb, sb) = (
        l.program_transport,
        l.source_transport.expect("Source の再生の操作のバー"),
    );
    assert_eq!(ui.region_at(pb.x + 2.0, pb.y + 20.0), Region::Program);
    assert_eq!(ui.region_at(sb.x + 2.0, sb.y + 20.0), Region::Source);

    // ジョグバーの溝の中ほどを押すと Program の長さの半分へシークする。
    let track = jog_track(&l, Viewer::Program).expect("ジョグバーの溝");
    let mid = (track.x + track.w / 2.0, track.y + track.h / 2.0);
    let seek = ui.transport_pointer(down(mid.0, mid.1));
    match seek[..] {
        [TransportAction::Seek {
            viewer: Viewer::Program,
            t,
        }] => assert!((t - 5.0).abs() < 0.2, "{t}"),
        _ => panic!("{seek:?}"),
    }
    ui.transport_pointer(PointerEvent::Up);
    // ボタンは押して離したときに実行する。
    let stop = transport_button(&l, Viewer::Program, "playback.stop").expect("Stop");
    let (x, y) = (stop.x + stop.w / 2.0, stop.y + stop.h / 2.0);
    assert!(ui.transport_pointer(down(x, y)).is_empty());
    let actions = ui.transport_pointer(PointerEvent::Up);
    assert_eq!(actions, [TransportAction::Command("playback.stop")]);
}
