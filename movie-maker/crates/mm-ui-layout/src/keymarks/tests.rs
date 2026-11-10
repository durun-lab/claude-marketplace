use super::*;
use crate::controller::{Action, Mods, PointerEvent, TimelineController};
use crate::geom::QuadShape;
use crate::paint::tests::state;
use crate::paint::{timeline_quads, PaintInput};
use crate::palette::{self, Palette};
use crate::rows::{RowHeight, RowHeights, Rows};
use crate::time_view::TimeView;
use crate::timeline::{hit_test, Hit, ItemKind, Part, TimelineModel};
use crate::timeline_layout::tests::TestViewport;

fn model(keys: &[f64]) -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![Item::layer(0.0, 10.0, ItemKind::Text, true).with_keyframes(keys)],
        ..TimelineModel::default()
    }
}

const SIZE: (f32, f32) = (1200.0, 300.0);

/// 全部の行を Small にした行の並び。
fn small_rows(m: &TimelineModel) -> Rows {
    let heights = RowHeights {
        all: Some(RowHeight::Small),
        ..RowHeights::default()
    };
    Rows::of(&m.items, &heights)
}

#[test]
fn キーフレームの時刻は_timeline_の時刻の位置に写る() {
    let m = model(&[1.0, 2.5]);
    let f = TestViewport::new(&m, SIZE);
    let vp = f.vp();
    let xs: Vec<f32> = marks(&vp, &m.items[0]).iter().map(|k| k.x).collect();
    let x0 = vp.layout.row_area.x;
    assert_eq!(xs, [x0 + 100.0, x0 + 250.0]);
}

#[test]
fn 六_px_より近いキーフレームは先頭の位置の_1_つにまとまる() {
    // 1 秒 100 px。1.0 と 1.05（5 px）、1.059（5.9 px）は先頭にまとまり、1.06（6 px）は別。
    let m = model(&[1.0, 1.05, 1.059, 1.06, 2.0]);
    let f = TestViewport::new(&m, SIZE);
    let found = marks(&f.vp(), &m.items[0]);
    let keys: Vec<usize> = found.iter().map(|k| k.keyframe).collect();
    assert_eq!(keys, [0, 3, 4]);
}

#[test]
fn 横に動かしてもまとめ方は変わらない() {
    let keys: Vec<f64> = (0..40).map(|i| 1.0 + f64::from(i) * 0.04).collect();
    let m = model(&keys);
    let mut f = TestViewport::new(&m, SIZE);
    let all: Vec<usize> = marks(&f.vp(), &m.items[0])
        .iter()
        .map(|k| k.keyframe)
        .collect();
    f.time.origin = 1.5;
    let seen: Vec<usize> = marks(&f.vp(), &m.items[0])
        .iter()
        .map(|k| k.keyframe)
        .collect();
    assert!(seen.len() < all.len() && seen.iter().all(|k| all.contains(k)));
}

#[test]
fn 見えている範囲の外のキーフレームは出さない() {
    let m = model(&[1.0, 50.0]);
    let f = TestViewport::new(&m, SIZE);
    assert_eq!(marks(&f.vp(), &m.items[0]).len(), 1);
}

#[test]
fn ドラッグ中の_shift_layer_のマーカーは動かした時刻で描き_領域の外から入るものも描く() {
    let mut m = model(&[1.0, 15.0]);
    m.duration = 20.0;
    m.items[0].end = 20.0;
    let f = TestViewport::new(&m, SIZE);
    let vp = f.vp();
    assert!(
        vp.x_of(15.0) > vp.layout.row_area.right(),
        "15 秒は領域の外"
    );
    let centers = |by: f64| -> Vec<f32> {
        let st = crate::paint::PaintState {
            preview: Some((crate::gesture::EditKind::ShiftLayer { item: 0 }, by)),
            ..state(0.0)
        };
        let quads = timeline_quads(&PaintInput::new(&m, &vp, &st)).quads;
        let outer = quads.into_iter().filter(|q| q.shape == QuadShape::Diamond);
        let outer = outer.filter(|q| q.rect.w == MARK_PX);
        outer.map(|q| q.rect.x + q.rect.w / 2.0).collect()
    };
    let x0 = vp.layout.row_area.x;
    assert_eq!(centers(0.5), [x0 + 150.0], "1 秒のマーカーが 1.5 秒へ");
    assert_eq!(centers(-10.0), [x0 + 500.0], "15 秒のマーカーが 5 秒へ入る");
}

#[test]
fn small_の行と領域の端では縦線_収まる_medium_はひし形() {
    let m = model(&[0.0, 1.0]);
    let f = TestViewport::new(&m, SIZE);
    let vp = f.vp();
    let line = |x| is_line(&vp, 0, mark_rect(&vp, 0, x));
    assert!(line(vp.x_of(0.0)), "左端は領域からはみ出すので縦線");
    assert!(!line(vp.x_of(1.0)), "収まる Medium はひし形");
    let rows = small_rows(&m);
    let small = Viewport { rows: &rows, ..vp };
    assert!(is_line(&small, 0, mark_rect(&small, 0, vp.x_of(1.0))));
}

#[test]
fn small_の縦線は帯の下端から_4_px_で_名前の字の下の方にだけ掛かる() {
    let m = model(&[1.0]);
    let f = TestViewport::new(&m, SIZE);
    let rows = small_rows(&m);
    let vp = Viewport {
        rows: &rows,
        ..f.vp()
    };
    let bar = vp.bar(0, 0.0, 10.0);
    let line = line_rect(mark_rect(&vp, 0, vp.x_of(1.0)));
    assert_eq!((line.w, line.h), (2.0, 4.0));
    assert_eq!(line.bottom(), bar.bottom());
}

#[test]
fn 描くひし形は切らずに行の領域に収まり_縦線は領域で切る() {
    let m = model(&[0.0, 1.0]);
    let f = TestViewport::new(&m, SIZE);
    let quads = timeline_quads(&PaintInput::new(&m, &f.vp(), &state(0.0))).quads;
    let area = f.layout.row_area;
    let diamonds: Vec<_> = quads
        .iter()
        .filter(|q| q.shape == QuadShape::Diamond)
        .collect();
    assert_eq!(diamonds.len(), 2, "1 つの縁と中");
    assert!(diamonds
        .iter()
        .all(|q| q.rect.x >= area.x && q.rect.right() <= area.right()));
    let lines = quads
        .iter()
        .filter(|q| q.color == palette::CUT_SEAM && q.rect.w <= 4.0);
    assert!(lines.count() >= 1, "t=0 の縦線");
}

#[test]
fn ホバーしたマーカーは中を_accent_にし_縁を明るくする() {
    let m = model(&[1.0, 2.0]);
    let f = TestViewport::new(&m, SIZE);
    let hover = Hit::BarMark {
        item: 0,
        keyframe: 1,
    };
    let st = crate::paint::PaintState {
        hover,
        ..state(0.0)
    };
    let quads = timeline_quads(&PaintInput::new(&m, &f.vp(), &st)).quads;
    let colors = |w: f32| -> Vec<_> {
        quads
            .iter()
            .filter(|q| q.shape == QuadShape::Diamond && q.rect.w == w)
            .map(|q| q.color)
            .collect()
    };
    let p = Palette::DEFAULT;
    assert_eq!(colors(MARK_PX), [palette::CUT_SEAM, p.selection], "縁");
    assert_eq!(colors(MARK_PX - 3.0), [p.selection, p.accent], "中");
}

#[test]
fn 見えない行のマーカーは描かない() {
    let items = (0..20)
        .map(|_| Item::layer(0.0, 10.0, ItemKind::Image, true).with_keyframes(&[1.0]))
        .collect();
    let m = TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items,
        ..TimelineModel::default()
    };
    let mut f = TestViewport::new(&m, SIZE);
    f.scroll_y = 28.0 * 10.0;
    let vp = f.vp();
    let quads = timeline_quads(&PaintInput::new(&m, &vp, &state(0.0))).quads;
    let whole = vp
        .visible_rows()
        .filter(|&i| !is_line(&vp, i, mark_rect(&vp, i, vp.x_of(1.0))))
        .count();
    let diamonds = quads.iter().filter(|q| q.shape == QuadShape::Diamond);
    assert!(whole < 20, "見えない行は数えない");
    assert_eq!(diamonds.count(), whole * 2, "全部見える行の分（縁と中）");
}

#[test]
fn 重なるマーカーの矩形では中心が近い方に当たる() {
    // 1 秒 100 px。1.0 と 1.07 は 7 px 離れ、別のマーカーで矩形が 2 px 重なる。
    let m = model(&[1.0, 1.07]);
    let f = TestViewport::new(&m, SIZE);
    let vp = f.vp();
    let y = mark_rect(&vp, 0, vp.x_of(1.0)).y + 4.5;
    let at = |x| mark_at(&vp, 0, &m.items[0], (x, y)).map(|k| k.keyframe);
    assert_eq!(at(vp.x_of(1.0) + 3.0), Some(0));
    assert_eq!(at(vp.x_of(1.0) + 4.0), Some(1));
}

#[test]
fn レイヤーの始まりのマーカーは端のつまみより勝ち_つまみはマーカーの上で掴める() {
    let m = TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![Item::layer(2.0, 6.0, ItemKind::Text, true).with_keyframes(&[2.0])],
        ..TimelineModel::default()
    };
    let f = TestViewport::new(&m, SIZE);
    let vp = f.vp();
    let x = vp.x_of(2.0) + 2.0;
    let mark = mark_rect(&vp, 0, vp.x_of(2.0));
    let marker = Hit::BarMark {
        item: 0,
        keyframe: 0,
    };
    assert_eq!(hit_test(&m, &vp, x, mark.y + 4.5), marker);
    let handle = Hit::Layer {
        index: 0,
        part: Part::InHandle,
    };
    assert_eq!(hit_test(&m, &vp, x, mark.y - 1.0), handle);
}

fn controller(m: &TimelineModel) -> TimelineController {
    let mut c = TimelineController::new(TimeView {
        origin: 0.0,
        px_per_sec: 100.0,
    });
    c.set_rows(Rows::of(&m.items, &RowHeights::default()));
    c.resize(SIZE);
    c
}

#[test]
fn マーカーのクリックは再生位置をその時刻へ動かす() {
    let m = model(&[1.0, 2.5]);
    let mut c = controller(&m);
    let vp = c.viewport();
    let x = vp.x_of(2.5);
    let y = mark_rect(&vp, 0, x).y + 4.5;
    assert_eq!(c.handle(PointerEvent::Move { x, y }, &m, 0.0), []);
    let hover = Hit::BarMark {
        item: 0,
        keyframe: 1,
    };
    assert_eq!(c.hover, hover);
    let ev = PointerEvent::Down {
        x,
        y,
        mods: Mods::default(),
    };
    let actions = c.handle(ev, &m, 0.0);
    assert_eq!(
        actions,
        [Action::Select(Some(0)), Action::Seek(2.5)],
        "レイヤーも選び、いちばん近いフレームの先頭へ"
    );
    assert!(
        c.handle(PointerEvent::Up, &m, 0.0).is_empty(),
        "ドラッグにならない"
    );
}

#[test]
fn マーカーの外の帯のクリックは今まで通り() {
    let m = model(&[1.0]);
    let mut c = controller(&m);
    let vp = c.viewport();
    let x = vp.x_of(5.0);
    let y = vp.bar(0, 0.0, 0.0).y + 4.0;
    let ev = PointerEvent::Down {
        x,
        y,
        mods: Mods::default(),
    };
    assert_eq!(c.handle(ev, &m, 0.0), [Action::Select(Some(0))]);
}
