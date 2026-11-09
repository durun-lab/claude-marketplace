//! Timeline の文字（目盛りの時刻、帯の名前）を描いて、字の画素の位置と大きさを確かめる。
//! 字の画素は、背景の色から離れて字の色に寄った画素で見分ける。

use mm_ui_layout::{
    Item, ItemKind, PointerEvent, TimelineModel, LABEL_PAD, MARK_PX, RULER_H, VIEW_MARGIN,
};
use mm_ui_wgpu::Ui;

use crate::support::{bar_rect, gpu, px_in, render_sized, FORMAT, H, W};

/// 字の画素の範囲（左、上、右、下。両端を含む）。
type InkBox = (u32, u32, u32, u32);

/// 試験のモデルの長さ（秒）。
const DURATION: f64 = 10.0;

fn model(items: Vec<Item>) -> TimelineModel {
    TimelineModel {
        duration: DURATION,
        fps: 30.0,
        items,
    }
}

/// 表示の倍率 `scale` の、論理ピクセルで `W`x`H` のサーフェス。
fn ui_at(device: &wgpu::Device, scale: f32, m: TimelineModel) -> (Ui, (u32, u32)) {
    let size = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
    let mut ui = Ui::new(device, FORMAT);
    ui.set_scale_factor(scale);
    ui.set_model(m);
    ui.resize(size.0, size.1);
    (ui, size)
}

/// 行 `row` の時刻 `[start, end)` の帯の、物理ピクセルの矩形（左、上、右、下。右と下は含まない）。
fn bar_box(ui: &Ui, m: &TimelineModel, scale: f32, row: usize, (start, end): (f32, f32)) -> InkBox {
    let r = bar_rect(ui, m, scale, row, (f64::from(start), f64::from(end)));
    (
        r.x.ceil() as u32,
        r.y.ceil() as u32,
        r.right().floor() as u32,
        r.bottom().floor() as u32,
    )
}

/// 画像 `img`（幅 `w`）の矩形 `[x0, x1) x [y0, y1)` の中で `ink` に当たる画素の範囲。
fn ink_box(
    img: &[u8],
    w: u32,
    (x0, y0, x1, y1): InkBox,
    ink: impl Fn([u8; 3]) -> bool,
) -> Option<InkBox> {
    let mut found: Option<InkBox> = None;
    for y in y0..y1 {
        for x in x0..x1 {
            if ink(px_in(img, w, x, y)) {
                let b = found.get_or_insert((x, y, x, y));
                *b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            }
        }
    }
    found
}

/// 青い帯（撮影素材）の上の明るい字。つまみの薄い明るさ（赤が 90 前後）と再生位置の線（緑が 90）は字にしない。
fn light_ink(p: [u8; 3]) -> bool {
    p.iter().all(|c| *c > 150)
}

/// 橙の帯（文字）の上の暗い字。
fn dark_ink(p: [u8; 3]) -> bool {
    p[0] < 110
}

#[test]
fn 帯の名前は表示の倍率_125_から_200_でも論理ピクセルの寸法で帯の中に描く() {
    let Some((device, queue)) = gpu() else { return };
    let mut base: Option<InkBox> = None;
    for scale in [1.0_f32, 1.25, 1.5, 2.0] {
        let m = model(vec![
            Item::layer(0.0, DURATION, ItemKind::Footage, true).with_label("clip-0123.mp4")
        ]);
        let (mut ui, size) = ui_at(&device, scale, m.clone());
        let out = render_sized(&device, &queue, &mut ui, size);
        let bar = bar_box(&ui, &m, scale, 0, (0.0, DURATION as f32));
        let b = ink_box(&out, size.0, bar, light_ink).expect("帯の名前の字");
        // 字は帯の左端から余白を空けた所から始まる（最初の字の左の空きは 2 論理ピクセルまで）。
        let left = bar.0 as f32 + LABEL_PAD * scale;
        assert!(
            b.0 as f32 >= left - 1.0 && (b.0 as f32) <= left + 2.0 * scale,
            "表示の倍率 {scale}: 字の左端 {} （名前の左端 {left}）",
            b.0
        );
        // 字は帯の上の部分（下端の帯のマーカーの所を除く）の縦の中央に置き、マーカーの所に掛けない。
        let marks_top = bar.3 as f32 - MARK_PX * scale;
        let centre = (b.1 + b.3) as f32 / 2.0;
        let name_centre = (bar.1 as f32 + marks_top) / 2.0;
        assert!(
            (centre - name_centre).abs() <= 3.0 * scale,
            "表示の倍率 {scale}: 字の縦の中央 {centre} と名前を置く所の中央 {name_centre}"
        );
        assert!(
            (b.3 as f32) < marks_top + 1.0,
            "表示の倍率 {scale}: 字の下端 {} とマーカーの上端 {marks_top}",
            b.3
        );
        let Some(one) = base else {
            base = Some(b);
            continue;
        };
        let pairs = [
            ("幅", b.2 - b.0, one.2 - one.0),
            ("高さ", b.3 - b.1, one.3 - one.1),
        ];
        for (name, got, at_one) in pairs {
            let want = at_one as f32 * scale;
            assert!(
                (got as f32 - want).abs() <= 2.0 + scale,
                "表示の倍率 {scale}: 字の{name} {got}（等倍の {at_one} の {scale} 倍は {want}）"
            );
        }
    }
}

#[test]
fn 入らない名前は帯の中で切り詰めて省略記号を付ける() {
    let Some((device, queue)) = gpu() else { return };
    for scale in [1.0_f32, 1.25, 1.5] {
        let long = "A long caption that does not fit in the bar";
        let m = model(vec![
            Item::layer(0.0, 1.0, ItemKind::Text, true).with_label(long)
        ]);
        let (mut ui, size) = ui_at(&device, scale, m.clone());
        let out = render_sized(&device, &queue, &mut ui, size);
        let bar = bar_box(&ui, &m, scale, 0, (0.0, 1.0));
        let b = ink_box(&out, size.0, bar, dark_ink).expect("帯の名前の字");
        let limit = bar.2 as f32 - LABEL_PAD * scale;
        assert!(
            (b.2 as f32) < limit + 1.0,
            "表示の倍率 {scale}: 字の右端 {} は帯の右の余白の内側 {limit}",
            b.2
        );
        // 右端の数画素は「…」の点だけ。点は字の下の方にあり、上の半分には掛からない。
        let tail = (b.2 - (4.0 * scale) as u32, bar.1, b.2 + 1, bar.3);
        let dots = ink_box(&out, size.0, tail, dark_ink).expect("省略記号");
        let middle = (b.1 + b.3) / 2;
        assert!(
            dots.1 > middle,
            "表示の倍率 {scale}: 右端の字の上端 {} は字の中ほど {middle} より下（省略記号）",
            dots.1
        );
    }
}

#[test]
fn 狭い帯には名前を出さない() {
    let Some((device, queue)) = gpu() else { return };
    // 0.2 秒は約 12 論理ピクセルで、余白を除くと名前を入れる幅が無い。
    let m = model(vec![
        Item::layer(0.0, 0.2, ItemKind::Text, true).with_label("x")
    ]);
    let (mut ui, size) = ui_at(&device, 1.0, m.clone());
    let out = render_sized(&device, &queue, &mut ui, size);
    let bar = bar_box(&ui, &m, 1.0, 0, (0.0, 0.2));
    assert_eq!(ink_box(&out, size.0, bar, dark_ink), None);
}

#[test]
fn 目盛りの時刻と日本語の名前を描く() {
    let Some((device, queue)) = gpu() else { return };
    let scale = 1.5;
    let m = model(vec![
        Item::layer(0.0, DURATION, ItemKind::Text, true).with_label("日本語の字幕")
    ]);
    let (mut ui, size) = ui_at(&device, scale, m.clone());
    let out = render_sized(&device, &queue, &mut ui, size);
    let t = ui.layout().timeline;
    // 目盛りの時刻は灰色の明るい字。目盛りの線（赤が 107）と再生位置の線（緑が 90）は字にしない。
    let ruler_ink = |p: [u8; 3]| p.iter().all(|c| *c > 120);
    let x0 = t.x + (VIEW_MARGIN + 2.0) * scale;
    let ruler_bottom = t.y + RULER_H * scale;
    let ruler = (
        x0 as u32,
        t.y as u32,
        (x0 + 60.0) as u32,
        ruler_bottom as u32,
    );
    let r = ink_box(&out, size.0, ruler, ruler_ink).expect("0:00 の字");
    assert!((r.3 as f32) < ruler_bottom, "目盛りの帯の中");
    let bar = bar_box(&ui, &m, scale, 0, (0.0, DURATION as f32));
    let b = ink_box(&out, size.0, bar, dark_ink).expect("名前の字");
    // 全角 6 字は、字の大きさ 12 の 1.5 倍で 1 字 18 画素（字の左右の空きを除いて 5 字分より広い）。
    assert!(b.2 - b.0 > 5 * 18, "幅 {}", b.2 - b.0);
}

#[test]
fn 表示が変わらない間はシェーピングし直さず_頂点も作り直さない() {
    let Some((device, queue)) = gpu() else { return };
    let items = (0..5)
        .map(|i| Item::layer(0.0, DURATION, ItemKind::Image, true).with_label(&format!("{i}.png")))
        .collect();
    let (mut ui, size) = ui_at(&device, 1.0, model(items));
    render_sized(&device, &queue, &mut ui, size);
    let first = ui.text_stats();
    assert_eq!(first.uploads, 1);
    assert!(first.shaped >= 5, "{first:?}");
    render_sized(&device, &queue, &mut ui, size);
    ui.set_playhead(3.0);
    render_sized(&device, &queue, &mut ui, size);
    let moved = ui.text_stats();
    assert_eq!(
        moved.uploads, 2,
        "見出しの列の上の角のタイムコードだけが変わる"
    );
    assert!(moved.shaped <= first.shaped + 1, "{moved:?} {first:?}");
    render_sized(&device, &queue, &mut ui, size);
    assert_eq!(ui.text_stats(), moved, "再生位置が止まっている間");
    let t = ui.layout().timeline;
    ui.pointer(PointerEvent::Wheel {
        x: t.x + t.w / 2.0,
        y: t.y + 40.0,
        dy: 1.0,
        ctrl: true,
        shift: false,
    });
    render_sized(&device, &queue, &mut ui, size);
    let zoomed = ui.text_stats();
    assert_eq!(zoomed.uploads, 3, "拡大すると置き場所が変わる");
    render_sized(&device, &queue, &mut ui, size);
    assert_eq!(ui.text_stats(), zoomed);
}
