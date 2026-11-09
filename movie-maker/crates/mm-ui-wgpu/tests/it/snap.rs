//! スナップの表示（太いガイド線と上下の三角）とドラッグ中のツールチップを、つまみのドラッグの途中で描いて確かめる。

use mm_ui_layout::palette::{self, Palette};
use mm_ui_layout::{
    Item, ItemKind, Mods, PointerEvent, Rect, RowHeights, Rows, TimelineLayout, TimelineModel,
};
use mm_ui_wgpu::Ui;

use crate::support::{bar_rect, close, gpu, hex, px_in, render_sized, FORMAT, H, W};

/// 再生位置（秒）。撮影素材のレイヤー（0-4 秒）の出つまみをここへ引くと吸い付く。
const PLAYHEAD: f64 = 6.0;

/// 引いたまま描いた結果。
struct Dragged {
    image: Vec<u8>,
    /// 引いた先の点（物理ピクセル）。
    at: (f32, f32),
    /// 引いた先の時刻の x。
    x: f32,
    /// Timeline の矩形。
    timeline: Rect,
    /// 目盛りの帯の下端と、行の領域の下端の y。
    ruler_bottom: f32,
    rows_bottom: f32,
}

/// 引いている途中の `Ui`。
struct Held {
    device: wgpu::Device,
    queue: wgpu::Queue,
    ui: Ui,
    model: TimelineModel,
    /// 引いた先の点（物理ピクセル）。
    at: (f32, f32),
    /// 引いた先の時刻の x。
    x: f32,
}

/// 撮影素材のレイヤーの出つまみを掴んで、時刻 `to`（秒）の右 `dx` まで引く（離さない）。
fn hold(to: f64, dx: f32) -> Option<Held> {
    let (device, queue) = gpu()?;
    let model = TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![Item::layer(0.0, 4.0, ItemKind::Footage, true)],
        ..TimelineModel::default()
    };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model.clone());
    ui.resize(W, H);
    ui.set_playhead(PLAYHEAD);
    let bar = bar_rect(&ui, &model, 1.0, 0, (0.0, 4.0));
    let y = bar.y + bar.h / 2.0;
    let x = bar_rect(&ui, &model, 1.0, 0, (to, to)).x;
    let mods = Mods::default();
    ui.pointer(PointerEvent::Down {
        x: bar.right() - 2.0,
        y,
        mods,
    });
    ui.pointer(PointerEvent::Move { x: x + dx, y });
    Some(Held {
        device,
        queue,
        ui,
        model,
        at: (x + dx, y),
        x,
    })
}

/// 撮影素材のレイヤーの出つまみを掴んで、時刻 `to`（秒）の右 `dx` まで引いたまま描く。
fn drag_to(to: f64, dx: f32) -> Option<Dragged> {
    let mut h = hold(to, dx)?;
    let image = render_sized(&h.device, &h.queue, &mut h.ui, (W, H));
    let timeline = h.ui.layout().timeline;
    let content = Rows::of(&h.model.items, &RowHeights::default()).content_height();
    let layout = TimelineLayout::new((timeline.w, timeline.h), content);
    Some(Dragged {
        image,
        at: h.at,
        x: h.x,
        timeline,
        ruler_bottom: timeline.y + layout.ruler.bottom(),
        rows_bottom: timeline.y + layout.row_area.bottom(),
    })
}

/// 画像の矩形 `r` の中で `want` に合う画素の数。
fn count(img: &[u8], r: Rect, want: impl Fn([u8; 3]) -> bool) -> usize {
    let (x0, y0) = (r.x.max(0.0) as u32, r.y.max(0.0) as u32);
    let (x1, y1) = (
        r.right().min(W as f32) as u32,
        r.bottom().min(H as f32) as u32,
    );
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|(x, y)| want(px_in(img, W, *x, *y)))
        .count()
}

fn guide(p: [u8; 3]) -> bool {
    close(p, hex(palette::GUIDE), 8)
}

/// ツールチップの矩形の左上の部分（ポインターの右 16 から始まる矩形の、縁の内側）。
fn tip_area(at: (f32, f32)) -> Rect {
    Rect::new(at.0 + 18.0, at.1 - 7.0, 24.0, 14.0)
}

fn tip_fill(p: [u8; 3]) -> bool {
    close(p, hex(Palette::DEFAULT.ruler), 4)
}

#[test]
fn 吸い付いたドラッグは太いガイド線と上下の三角とツールチップを描く() {
    let Some(d) = drag_to(PLAYHEAD, 3.0) else {
        return;
    };
    // 帯の行より下の空いた所で、ガイド線（太さ 2）が再生位置の線の上に重なる。
    let below = Rect::new(d.x - 4.0, d.at.1 + 30.0, 8.0, 1.0);
    assert_eq!(count(&d.image, below, guide), 2, "太さ 2 のガイド線");
    // 下向きの三角は底辺が上。目盛りの帯の下端から 6 上の行では、中心から 2〜3 右も三角の中。
    let top_mark = Rect::new(d.x + 2.0, d.ruler_bottom - 6.0, 2.0, 1.0);
    assert_eq!(count(&d.image, top_mark, guide), 2, "目盛りの帯の三角");
    let bottom_mark = Rect::new(d.x + 2.0, d.rows_bottom - 2.0, 2.0, 1.0);
    assert_eq!(count(&d.image, bottom_mark, guide), 2, "下端の三角");
    let tip = tip_area(d.at);
    assert!(count(&d.image, tip, tip_fill) > 20, "ツールチップの矩形");
    let ink = |p: [u8; 3]| p.iter().all(|c| *c > 180);
    assert!(count(&d.image, tip, ink) > 5, "量の文字");
}

#[test]
fn 吸い付かないドラッグにはガイド線を出さない() {
    let Some(d) = drag_to(7.8, 0.0) else {
        return;
    };
    assert_eq!(
        count(&d.image, d.timeline, guide),
        0,
        "スナップの表示が無い"
    );
    assert!(
        count(&d.image, tip_area(d.at), tip_fill) > 20,
        "ツールチップは出す"
    );
}

#[test]
fn 再生位置の線はツールチップの矩形の上に描かない() {
    let Some(mut h) = hold(7.8, 0.0) else {
        return;
    };
    // ツールチップの中（ポインターの右 30）を通る時刻に再生位置を置く。
    let per_sec = bar_rect(&h.ui, &h.model, 1.0, 0, (0.0, 1.0)).w;
    h.ui.set_playhead(7.8 + f64::from(30.0 / per_sec));
    let image = render_sized(&h.device, &h.queue, &mut h.ui, (W, H));
    let playhead = |p: [u8; 3]| close(p, hex(palette::PLAYHEAD), 8);
    let tip = tip_area(h.at);
    assert!(count(&image, tip, tip_fill) > 20, "ツールチップの矩形");
    assert_eq!(count(&image, tip, playhead), 0, "ツールチップの中");
    let below = Rect::new(tip.x, h.at.1 + 30.0, tip.w, 1.0);
    assert_eq!(count(&image, below, playhead), 2, "ツールチップの外は描く");
}
