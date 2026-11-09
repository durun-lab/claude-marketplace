//! Timeline のレイヤーの行（行の高さ）と縦のスクロールを、表示の倍率 1、1.5、2 で描いて確かめる。
//! 行と帯とスクロールボックスの位置は `Ui::view_state` と `mm-ui-layout` の関数から求める（`Ui` と同じ計算）。

use mm_ui_layout::{
    palette, scroll, CutBar, Item, ItemKind, PointerEvent, Rect, RowHeight, RowHeights, Rows,
    TimeView, TimelineLayout, TimelineModel, ViewState,
};
use mm_ui_wgpu::Ui;

use crate::support::{bar_rect, close, gpu, hex, px_in, render_sized, FORMAT, H, W};

/// 上に文字のレイヤーが `texts` 本、いちばん下に sequence のレイヤー（カット 2 つ）。長さ 10 秒。
fn model(texts: usize) -> TimelineModel {
    let mut items: Vec<Item> = (0..texts)
        .map(|i| {
            let start = (i % 9) as f64;
            Item::layer(start, start + 1.0, ItemKind::Text, true).with_id(&format!("text-{i}"))
        })
        .collect();
    let mut seq = Item::layer(0.0, 10.0, ItemKind::Sequence, true).with_id("main");
    let cut = |start, end| CutBar {
        start,
        end,
        ..CutBar::default()
    };
    seq.cuts = vec![cut(0.0, 6.0), cut(6.0, 10.0)];
    items.push(seq);
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items,
    }
}

/// 表示の倍率 `scale` の、論理ピクセルで `W`x`H` のサーフェス。
fn ui_at(device: &wgpu::Device, scale: f32, m: &TimelineModel) -> (Ui, (u32, u32)) {
    let size = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
    let mut ui = Ui::new(device, FORMAT);
    ui.set_scale_factor(scale);
    ui.set_model(m.clone());
    ui.resize(size.0, size.1);
    (ui, size)
}

/// Timeline の割り付けと行の並び（`Ui` と同じ計算。論理ピクセル）。
fn geometry(ui: &Ui, m: &TimelineModel, scale: f32) -> (TimelineLayout, Rows) {
    let t = ui.layout().timeline;
    let state = ui.view_state().expect("最初の表示");
    let rows = Rows::of(&m.items, &state.heights);
    let layout = TimelineLayout::new((t.w / scale, t.h / scale), rows.content_height());
    (layout, rows)
}

/// 論理ピクセルの矩形の中央の画素（物理ピクセル）。
fn centre(ui: &Ui, scale: f32, r: Rect) -> (u32, u32) {
    let t = ui.layout().timeline;
    let x = t.x + (r.x + r.w / 2.0) * scale;
    let y = t.y + (r.y + r.h / 2.0) * scale;
    (x as u32, y as u32)
}

/// 列 `x` の上で、`color` に近い画素が続く最も長い数。
fn run_in_column(img: &[u8], size: (u32, u32), x: u32, color: [u8; 3]) -> u32 {
    let (mut best, mut run) = (0, 0);
    for y in 0..size.1 {
        run = if close(px_in(img, size.0, x, y), color, 3) {
            run + 1
        } else {
            0
        };
        best = best.max(run);
    }
    best
}

#[test]
fn 開くと縦のスクロールバーを出し_下の高い_sequence_の行の上に文字の行が続く() {
    let Some((device, queue)) = gpu() else { return };
    let m = model(30);
    for scale in [1.0_f32, 1.5, 2.0] {
        let (mut ui, size) = ui_at(&device, scale, &m);
        let out = render_sized(&device, &queue, &mut ui, size);
        let (layout, rows) = geometry(&ui, &m, scale);
        let state = ui.view_state().expect("最初の表示");
        assert!(layout.scrollbar.w > 0.0, "表示の倍率 {scale}: 行が入らない");
        assert_eq!(
            state.scroll_y,
            rows.content_height() - layout.row_area.h,
            "表示の倍率 {scale}: いちばん下の sequence の行が見える所まで"
        );
        let thumb = scroll::thumb(layout.scrollbar, rows.content_height(), state.scroll_y);
        let (kx, ky) = centre(&ui, scale, thumb);
        let at = |x, y| px_in(&out, size.0, x, y);
        assert!(
            close(at(kx, ky), hex(palette::LABEL), 3),
            "表示の倍率 {scale}: スクロールボックス"
        );
        let seq = bar_rect(&ui, &m, scale, 30, (1.0, 5.0));
        let seq_x = (seq.x + seq.w / 2.0) as u32;
        let seq_color = hex(palette::item_color(ItemKind::Sequence));
        let height = run_in_column(&out, size, seq_x, seq_color);
        let want = rows.bar(30).1 * scale;
        assert!(
            (height as f32 - want).abs() <= 1.0,
            "表示の倍率 {scale}: sequence の帯の高さ {height}（期待 {want}）"
        );
        let text = bar_rect(&ui, &m, scale, 29, (2.0, 3.0));
        let text_at = at(
            (text.x + text.w / 2.0) as u32,
            (text.y + text.h / 2.0) as u32,
        );
        let text_color = hex(palette::item_color(ItemKind::Text));
        assert!(
            close(text_at, text_color, 3),
            "表示の倍率 {scale}: 上の文字の行 {text_at:?}"
        );
        assert!(text.bottom() <= seq.y, "表示の倍率 {scale}: 文字の行は上");
    }
}

#[test]
fn shift_ホイールで上の行に戻しても目盛りの帯は動かない() {
    let Some((device, queue)) = gpu() else { return };
    let m = model(30);
    let scale = 1.5;
    let (mut ui, size) = ui_at(&device, scale, &m);
    let t = ui.layout().timeline;
    let ruler_at = (t.x as u32 + 2, t.y as u32 + 2);
    let before = px_in(
        &render_sized(&device, &queue, &mut ui, size),
        size.0,
        ruler_at.0,
        ruler_at.1,
    );
    ui.pointer(PointerEvent::Wheel {
        x: t.x + t.w / 2.0,
        y: t.y + t.h / 2.0,
        dy: 1000.0,
        ctrl: false,
        shift: true,
    });
    let out = render_sized(&device, &queue, &mut ui, size);
    assert_eq!(ui.view_state().expect("表示").scroll_y, 0.0);
    assert_eq!(
        px_in(&out, size.0, ruler_at.0, ruler_at.1),
        before,
        "目盛りの帯"
    );
    let top = bar_rect(&ui, &m, scale, 0, (0.0, 1.0));
    let (layout, _) = geometry(&ui, &m, scale);
    assert_eq!(
        top.y,
        t.y + (layout.row_area.y + 2.0) * scale,
        "行 0 が行の領域の上端"
    );
    let p = px_in(
        &out,
        size.0,
        (top.x + top.w / 2.0) as u32,
        (top.y + top.h / 2.0) as u32,
    );
    assert!(
        close(p, hex(palette::item_color(ItemKind::Text)), 3),
        "{p:?}"
    );
}

#[test]
fn 戻した表示の状態はズームとスクロールと行の高さを当て_それまでは表示の状態を出さない() {
    let Some((device, queue)) = gpu() else { return };
    let m = model(30);
    let scale = 2.0;
    let size = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_scale_factor(scale);
    ui.set_model(m.clone());
    let saved = ViewState {
        time: TimeView {
            origin: 1.0,
            px_per_sec: 100.0,
        },
        scroll_y: 54.0,
        heights: RowHeights {
            all: Some(RowHeight::Small),
            ..RowHeights::default()
        },
    };
    ui.restore_view(saved.clone());
    assert_eq!(ui.view_state(), None, "大きさが決まる前");
    ui.resize(size.0, size.1);
    assert_eq!(ui.view_state(), Some(saved), "戻した値のまま");
    let out = render_sized(&device, &queue, &mut ui, size);
    // 54 px 送った Small の行は行 3（3 から 4 秒）から見える。
    let bar = bar_rect(&ui, &m, scale, 3, (3.0, 4.0));
    let color = hex(palette::item_color(ItemKind::Text));
    let height = run_in_column(&out, size, (bar.x + bar.w / 2.0) as u32, color);
    let want = Rows::new([RowHeight::Small]).bar(0).1 * scale;
    assert!(
        (height as f32 - want).abs() <= 1.0,
        "Small の帯の高さ {height}（期待 {want}）"
    );
}

#[test]
fn 行の高さを置き直すと帯の高さが変わり_スクロールは範囲に収まる() {
    let Some((device, queue)) = gpu() else { return };
    let m = model(30);
    let (mut ui, size) = ui_at(&device, 1.0, &m);
    ui.set_row_heights(RowHeights {
        all: Some(RowHeight::Large),
        ..RowHeights::default()
    });
    let out = render_sized(&device, &queue, &mut ui, size);
    let (layout, rows) = geometry(&ui, &m, 1.0);
    let state = ui.view_state().expect("表示");
    assert!(state.scroll_y <= rows.content_height() - layout.row_area.h);
    let first = rows.at(state.scroll_y).expect("見えている行");
    let row = first + 1;
    let start = (row % 9) as f64;
    let bar = bar_rect(&ui, &m, 1.0, row, (start, start + 1.0));
    let color = hex(palette::item_color(ItemKind::Text));
    let height = run_in_column(&out, size, (bar.x + bar.w / 2.0) as u32, color);
    assert_eq!(height as f32, Rows::new([RowHeight::Large]).bar(0).1);
}
