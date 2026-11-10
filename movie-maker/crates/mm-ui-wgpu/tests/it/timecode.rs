//! 再生位置のタイムコード（見出しの列の上の角、再生の操作のバー）は、1 字ずつキャッシュしたシェーピングの結果を並べ、
//! ほかの文字と頂点を分けて描くので、再生中はシェーピングせず、タイムコードのほかの文字の頂点も作り直さない。

use mm_types::{Rgba, RgbaImage, Size};
use mm_ui_layout::{Item, ItemKind, TimelineModel, ViewerState};
use mm_ui_wgpu::Ui;

use crate::support::{gpu, render_sized, FORMAT, H, W};

const FPS: f64 = 30.0;
/// 進めるフレームの数。
const FRAMES: u64 = 150;

fn model() -> TimelineModel {
    TimelineModel {
        duration: 600.0,
        fps: FPS,
        items: vec![Item::layer(0.0, 600.0, ItemKind::Footage, true)],
        ..TimelineModel::default()
    }
}

/// 再生中のビューアー。イン点とアウト点は再生中に動かない。
fn viewer(t: f64) -> ViewerState {
    ViewerState {
        t,
        duration: 600.0,
        fps: FPS,
        playing: true,
        mark_in: Some(1.0),
        mark_out: None,
        ..ViewerState::default()
    }
}

#[test]
fn 再生位置を毎フレーム進めても_シェーピングせず_タイムコードの頂点だけを作り直す() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let image = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.5, 0.5, 0.5, 1.0));
    ui.set_frame(&device, &queue, &image);
    ui.set_source_frame(&device, &queue, &image);
    let step = |ui: &mut Ui, i: u64| {
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
    assert_eq!(
        after.shaped, first.shaped,
        "タイムコードは 1 字ずつキャッシュした字を並べるだけ"
    );
    assert_eq!(
        after.uploads, first.uploads,
        "タイムコードのほかの文字の頂点は作り直さない"
    );
    assert_eq!(
        after.timecode_uploads,
        first.timecode_uploads + FRAMES,
        "タイムコードの頂点は毎フレーム作り直す"
    );
}
