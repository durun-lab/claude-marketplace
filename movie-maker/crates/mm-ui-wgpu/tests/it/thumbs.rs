//! 帯の先頭のサムネイルを、オフスクリーンに描いて確かめる。サムネイルは 1 色のスロットなので、画素の色で置き場所と重なりが分かる。

use mm_ui_layout::{Item, ItemKind, RowHeight, RowHeights, TimelineModel, ViewState};
use mm_ui_wgpu::{cell_from_rgb, cell_len, ThumbStats, Ui, CELL_H, CELL_W};

use crate::support::{bar_rect, close, gpu, px_in, render_sized, FORMAT, H, W};

const RED: [u8; 3] = [255, 0, 0];

/// 長さ 10 秒の撮影素材のレイヤーが 1 本。素材の鍵は `k`。
fn model(thumb: bool) -> TimelineModel {
    let item = Item::layer(0.0, 10.0, ItemKind::Footage, true).with_label("clip.mp4");
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![if thumb { item.with_thumb("k") } else { item }],
        ..TimelineModel::default()
    }
}

fn ui(device: &wgpu::Device, m: &TimelineModel, height: Option<RowHeight>) -> Ui {
    let mut ui = Ui::new(device, FORMAT);
    ui.set_model(m.clone());
    ui.resize(W, H);
    if let Some(height) = height {
        let mut state: ViewState = ui.view_state().expect("最初の表示");
        state.heights = RowHeights {
            all: Some(height),
            ..RowHeights::default()
        };
        ui.restore_view(state);
    }
    ui
}

fn red_cell() -> Vec<u8> {
    [255, 0, 0, 255].repeat(cell_len() / 4)
}

#[test]
fn サムネイルは読み込みを頼み_届いたら帯の左に描き_帯の残りは帯の色のまま() {
    let Some((device, queue)) = gpu() else { return };
    let m = model(true);
    let mut ui = ui(&device, &m, None);
    let before = render_sized(&device, &queue, &mut ui, (W, H));
    assert_eq!(
        ui.take_thumb_requests().len(),
        1,
        "スロットに無いサムネイルを頼む"
    );
    assert!(
        ui.take_thumb_requests().is_empty(),
        "頼み済みは頼み直さない"
    );
    assert_eq!(ui.thumb_stats().drawn, 0, "届くまではサムネイルなしで描く");
    ui.set_thumb(&device, &queue, "k", Some(&red_cell()));
    let after = render_sized(&device, &queue, &mut ui, (W, H));
    assert_eq!(ui.thumb_stats().drawn, 1);
    let bar = bar_rect(&ui, &m, 1.0, 0, (0.0, 10.0));
    let (y, left) = ((bar.y + bar.h / 2.0) as u32, (bar.x + 10.0) as u32);
    assert!(close(px_in(&after, W, left, y), RED, 2), "サムネイルの中");
    let body = (bar.x + 200.0) as u32;
    assert_eq!(
        px_in(&after, W, body, y),
        px_in(&before, W, body, y),
        "サムネイルの右は帯の色のまま"
    );
}

#[test]
fn 縮めて描くサムネイルは縮小の段から引き_細い縞は灰色になる() {
    let Some((device, queue)) = gpu() else { return };
    let m = model(true);
    let mut ui = ui(&device, &m, None);
    render_sized(&device, &queue, &mut ui, (W, H));
    ui.take_thumb_requests();
    // スロットと同じ大きさの、1 画素ごとの白黒の縦縞。倍率 1 の Medium の帯（44x25）ではスロットを 4 分の 1 ほどに縮める。
    let rgb: Vec<u8> = (0..CELL_W * CELL_H)
        .flat_map(|i| {
            [if (i % CELL_W).is_multiple_of(2) {
                255
            } else {
                0
            }; 3]
        })
        .collect();
    let stripes = cell_from_rgb(CELL_W, CELL_H, &rgb).unwrap();
    ui.set_thumb(&device, &queue, "k", Some(&stripes));
    let after = render_sized(&device, &queue, &mut ui, (W, H));
    let bar = bar_rect(&ui, &m, 1.0, 0, (0.0, 10.0));
    let y = (bar.y + bar.h / 2.0) as u32;
    for x in (bar.x + 4.0) as u32..(bar.x + 40.0) as u32 {
        let [r, ..] = px_in(&after, W, x, y);
        assert!((96..=160).contains(&r), "x={x}: {r}");
    }
}

#[test]
fn small_の行と鍵の無い帯には出さない() {
    let Some((device, queue)) = gpu() else { return };
    let mut small = ui(&device, &model(true), Some(RowHeight::Small));
    render_sized(&device, &queue, &mut small, (W, H));
    assert!(small.take_thumb_requests().is_empty());
    let mut none = ui(&device, &model(false), None);
    render_sized(&device, &queue, &mut none, (W, H));
    assert!(none.take_thumb_requests().is_empty());
}

#[test]
fn 読めなかったサムネイルは帯から外して名前を左に戻し_編集の後も頼み直さない() {
    let Some((device, queue)) = gpu() else { return };
    let mut plain = ui(&device, &model(false), None);
    let without = render_sized(&device, &queue, &mut plain, (W, H));
    let m = model(true);
    let mut ui = ui(&device, &m, None);
    let pending = render_sized(&device, &queue, &mut ui, (W, H));
    ui.take_thumb_requests();
    ui.set_thumb(&device, &queue, "k", None);
    let missing = render_sized(&device, &queue, &mut ui, (W, H));
    assert!(missing == without, "名前を右へずらさない");
    assert!(pending != without, "読み込みを待つ間は名前の左を空けておく");
    assert_eq!(ui.thumb_stats().dropped, 1);
    ui.update_model(m.clone());
    render_sized(&device, &queue, &mut ui, (W, H));
    assert!(
        ui.take_thumb_requests().is_empty(),
        "読めなかったサムネイルは編集の後も頼み直さない"
    );
    assert_eq!(ui.thumb_stats().dropped, 1, "置き換えたモデルからも外す");
}

#[test]
fn 取り込みを終えたら読めなかったサムネイルを帯に戻して頼み直し_届いたら描く() {
    let Some((device, queue)) = gpu() else { return };
    let m = model(true);
    let mut ui = ui(&device, &m, None);
    render_sized(&device, &queue, &mut ui, (W, H));
    ui.take_thumb_requests();
    ui.set_thumb(&device, &queue, "k", None);
    render_sized(&device, &queue, &mut ui, (W, H));
    ui.retry_thumbs();
    render_sized(&device, &queue, &mut ui, (W, H));
    assert_eq!(ui.take_thumb_requests().len(), 1);
    assert_eq!(ui.thumb_stats(), ThumbStats::default());
    ui.set_thumb(&device, &queue, "k", Some(&red_cell()));
    render_sized(&device, &queue, &mut ui, (W, H));
    assert_eq!(ui.thumb_stats().drawn, 1);
}
