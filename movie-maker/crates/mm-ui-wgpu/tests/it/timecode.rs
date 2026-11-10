//! 再生中の時刻の字（Timeline の角、再生の操作のバー）は、字ごとに覚えたグリフを並べるので、時刻が進んでもシェーピングし直さない。

use mm_types::{Rgba, RgbaImage, Size};
use mm_ui_layout::{Item, ItemKind, TimelineModel, ViewerState};
use mm_ui_wgpu::Ui;

use crate::support::{gpu, render_sized, FORMAT, H, W};

const FPS: f64 = 30.0;
/// 進めるフレームの数。
const FRAMES: usize = 150;

fn model() -> TimelineModel {
    TimelineModel {
        duration: 600.0,
        fps: FPS,
        items: vec![Item::layer(0.0, 600.0, ItemKind::Footage, true)],
        ..TimelineModel::default()
    }
}

fn viewer(t: f64) -> ViewerState {
    ViewerState {
        t,
        duration: 600.0,
        fps: FPS,
        playing: true,
        mark_in: Some(t / 2.0),
        mark_out: None,
        ..ViewerState::default()
    }
}

#[test]
fn 時刻を毎フレーム進めても_最初のフレームのあとはシェーピングしない() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let image = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.5, 0.5, 0.5, 1.0));
    ui.set_frame(&device, &queue, &image);
    ui.set_source_frame(&device, &queue, &image);
    let step = |ui: &mut Ui, i: usize| {
        // 37 フレームずつ進め、数字の桁がいろいろに変わるようにする。
        let t = (i * 37) as f64 / FPS;
        ui.set_playhead(t);
        ui.set_transport(viewer(t), Some(viewer(t)));
    };
    step(&mut ui, 0);
    render_sized(&device, &queue, &mut ui, (W, H));
    let first = ui.text_stats();
    assert!(first.shaped > 0);
    for i in 1..=FRAMES {
        step(&mut ui, i);
        render_sized(&device, &queue, &mut ui, (W, H));
    }
    let after = ui.text_stats();
    assert!(
        after.uploads > first.uploads,
        "時刻が変わるので頂点は作り直す"
    );
    assert_eq!(
        after.shaped, first.shaped,
        "時刻の字は覚えたグリフを並べるだけ"
    );
}
