//! Timeline のズームバーと、キーとメニューの全体表示のコマンド、再生位置のページ送りを、表示の倍率 1 と 1.5 で描いて確かめる。
//! スクロールボックスの位置は `Ui::view_state` と `mm-ui-layout` の関数から求める（`Ui` と同じ計算）。

use mm_ui_layout::{
    palette, zoombar, Item, ItemKind, Rect, RowHeights, Rows, TimeView, TimelineCommand,
    TimelineLayout, TimelineModel, ViewState,
};
use mm_ui_wgpu::Ui;

use crate::support::{close, gpu, hex, px_in, render_sized, FORMAT, H, W};

/// 10 秒の、文字のレイヤー 2 本。
fn model() -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![
            Item::layer(0.0, 10.0, ItemKind::Text, true),
            Item::layer(2.0, 6.0, ItemKind::Text, true),
        ],
    }
}

/// 2 秒から 4.x 秒を拡大して見せる表示。
const ZOOMED: TimeView = TimeView {
    origin: 2.0,
    px_per_sec: 240.0,
};

/// 表示の倍率 `scale` の、論理ピクセルで `W`x`H` のサーフェスに、`ZOOMED` の表示を戻した UI。
fn zoomed_ui(device: &wgpu::Device, scale: f32, m: &TimelineModel) -> (Ui, (u32, u32)) {
    let size = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
    let mut ui = Ui::new(device, FORMAT);
    ui.set_scale_factor(scale);
    ui.set_model(m.clone());
    ui.restore_view(ViewState {
        time: ZOOMED,
        scroll_y: 0.0,
        heights: RowHeights::default(),
    });
    ui.resize(size.0, size.1);
    (ui, size)
}

/// Timeline の割り付け（`Ui` と同じ計算。論理ピクセル）。
fn layout(ui: &Ui, m: &TimelineModel, scale: f32) -> TimelineLayout {
    let t = ui.layout().timeline;
    let rows = Rows::of(&m.items, &RowHeights::default());
    TimelineLayout::new((t.w / scale, t.h / scale), rows.content_height())
}

/// ズームバーの縦の中央の、論理ピクセルの x の画素（物理ピクセル）。`timeline` は Timeline の矩形（物理ピクセル）。
fn bar_px(timeline: Rect, l: &TimelineLayout, scale: f32, x: f32) -> (u32, u32) {
    let y = l.zoombar.y + l.zoombar.h / 2.0;
    (
        (timeline.x + x * scale) as u32,
        (timeline.y + y * scale) as u32,
    )
}

#[test]
fn timeline_の下端にズームバーを描き_全体表示でスクロールボックスが溝の全部になり_もう一度で戻る() {
    let Some((device, queue)) = gpu() else { return };
    let m = model();
    for scale in [1.0_f32, 1.5] {
        let (mut ui, size) = zoomed_ui(&device, scale, &m);
        let l = layout(&ui, &m, scale);
        assert!(l.zoombar.h > 0.0 && l.zoombar.bottom() == l.bounds.bottom());
        let thumb = zoombar::thumb(l.zoombar, m.duration, &ZOOMED, l.row_area.w);
        let out = render_sized(&device, &queue, &mut ui, size);
        let timeline = ui.layout().timeline;
        let at = |out: &[u8], x: f32| {
            let (px, py) = bar_px(timeline, &l, scale, x);
            px_in(out, size.0, px, py)
        };
        let label = hex(palette::LABEL);
        let mid = thumb.x + thumb.w / 2.0;
        assert!(
            close(at(&out, mid), label, 3),
            "表示の倍率 {scale}: スクロールボックス"
        );
        let track = at(&out, thumb.right() + 20.0);
        assert!(
            close(track, hex(palette::RULER), 3),
            "表示の倍率 {scale}: 溝 {track:?}"
        );

        ui.command(TimelineCommand::FullExtentZoom);
        let whole = render_sized(&device, &queue, &mut ui, size);
        let near_end = l.zoombar.right() - 10.0;
        assert!(
            close(at(&whole, near_end), label, 3),
            "表示の倍率 {scale}: 全体表示"
        );
        ui.command(TimelineCommand::FullExtentZoom);
        let back = ui.view_state().expect("表示の状態").time;
        assert_eq!(back, ZOOMED, "表示の倍率 {scale}: もう一度で元の倍率と位置");
    }
}

#[test]
fn 再生位置が右端を越えると_1_画面ぶん送り_左端を越えると戻す() {
    let Some((device, _queue)) = gpu() else {
        return;
    };
    let m = model();
    let (mut ui, _) = zoomed_ui(&device, 1.0, &m);
    let width = layout(&ui, &m, 1.0).row_area.w;
    let (_, end) = ZOOMED.visible(width);
    let time = |ui: &Ui| ui.view_state().expect("表示の状態").time;
    ui.set_playhead(end - 0.01);
    assert_eq!(time(&ui), ZOOMED, "見えている所へ動いても送らない");
    ui.set_playhead(end + 0.01);
    let paged = time(&ui);
    assert!(
        (paged.origin - end).abs() < 1e-9,
        "右端が左端になる: {paged:?}"
    );
    assert_eq!(paged.px_per_sec, ZOOMED.px_per_sec);
    ui.set_playhead(end - 0.01);
    assert!(
        (time(&ui).origin - ZOOMED.origin).abs() < 1e-9,
        "左端を越えると戻す"
    );
}
