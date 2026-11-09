use super::*;
use crate::keymarks::MARK_PX;
use crate::paint::tests::state;
use crate::paint::timeline_quads_with;
use crate::rows::{RowHeight, Rows};
use crate::time_view::TimeView;
use crate::timeline::{CutBar, ItemKind, TimelineModel};
use crate::timeline_layout::{TimelineLayout, Viewport, HEADER_W};
use crate::wave::{Peaks, Waves};

const SIZE: (f32, f32) = (800.0, 200.0);

/// 10 秒ぶんの、どこも `peak` のピーク。
fn flat(peak: f32) -> Waves {
    let mut w = Waves::default();
    w.insert("a".into(), Peaks::new(100.0, 1, &[peak; 1000]));
    w
}

fn src(at: f64, src: f64) -> Option<WaveSource> {
    Some(WaveSource {
        asset: "a".into(),
        at,
        src,
        speed: 1.0,
    })
}

fn layer_model() -> TimelineModel {
    let mut item = Item::layer(1.0, 4.0, ItemKind::Footage, true);
    item.wave = src(1.0, 0.0);
    TimelineModel {
        duration: 5.0,
        fps: 30.0,
        items: vec![item],
        ..TimelineModel::default()
    }
}

fn seq_model() -> TimelineModel {
    let cut = |start, end, gap, wave| CutBar {
        start,
        end,
        gap,
        wave,
        ..CutBar::default()
    };
    let mut seq = Item::layer(0.0, 5.0, ItemKind::Sequence, true);
    seq.cuts = vec![
        cut(0.0, 2.0, false, src(0.0, 3.0)),
        cut(2.0, 3.0, true, src(2.0, 0.0)),
    ];
    TimelineModel {
        duration: 5.0,
        fps: 30.0,
        items: vec![seq],
        ..TimelineModel::default()
    }
}

/// 行を `height` で並べ、波形を足したときに増える矩形（波形）と、帯の矩形を返す。
fn drawn(m: &TimelineModel, height: RowHeight, waves: &Waves) -> (Vec<Quad>, Rect) {
    let rows = Rows::new(m.items.iter().map(|_| height));
    let layout = TimelineLayout::new(SIZE, rows.content_height());
    let vp = Viewport {
        layout: &layout,
        rows: &rows,
        time: TimeView {
            origin: 0.0,
            px_per_sec: 100.0,
        },
        scroll_y: 0.0,
    };
    let st = state(0.0);
    let without = timeline_quads_with(m, &vp, &st, &Waves::default());
    let with = timeline_quads_with(m, &vp, &st, waves);
    let added = with.into_iter().filter(|q| !without.contains(q)).collect();
    (added, vp.bar(0, m.items[0].start, m.items[0].end))
}

#[test]
fn medium_は下端のマーカーの高さに収め_名前の上の部分に掛けない() {
    let (quads, bar) = drawn(&layer_model(), RowHeight::Medium, &flat(0.25));
    assert!(!quads.is_empty());
    for q in &quads {
        assert!(q.rect.y >= bar.bottom() - MARK_PX, "{:?}", q.rect);
        assert!(q.rect.bottom() <= bar.bottom());
        assert!(q.rect.x >= bar.x && q.rect.right() <= bar.right() + 0.01);
    }
}

#[test]
fn large_は帯の下半分() {
    let (quads, bar) = drawn(&seq_model(), RowHeight::Large, &flat(0.25));
    assert!(!quads.is_empty());
    for q in &quads {
        assert!(q.rect.y >= bar.y + bar.h / 2.0 - 1.0, "{:?}", q.rect);
        assert!(q.rect.bottom() <= bar.bottom());
    }
}

#[test]
fn small_は出さない() {
    let (quads, _) = drawn(&layer_model(), RowHeight::Small, &flat(0.25));
    assert!(quads.is_empty());
}

#[test]
fn ピークがまだ無い素材は描かない() {
    let (quads, _) = drawn(&layer_model(), RowHeight::Medium, &Waves::default());
    assert!(quads.is_empty());
}

#[test]
fn 隙間のカットには描かず_カットの範囲の中だけ描く() {
    let (quads, _) = drawn(&seq_model(), RowHeight::Large, &flat(0.25));
    let right = quads.iter().map(|q| q.rect.right()).fold(0.0, f32::max);
    assert!(
        right <= HEADER_W + 200.0 + 0.01,
        "最初のカット（0 から 2 秒）より右には描かない: {right}"
    );
}

#[test]
fn 振幅が大きいほど波形が高い() {
    let (big, _) = drawn(&layer_model(), RowHeight::Medium, &flat(1.0));
    let (small, _) = drawn(&layer_model(), RowHeight::Medium, &flat(0.25));
    assert!(big[0].rect.h > small[0].rect.h);
    assert_eq!(big[0].rect.h, MARK_PX, "満振幅は領域いっぱい");
}

#[test]
fn 同じ高さの列は_1_つの矩形にまとめる() {
    let (quads, bar) = drawn(&layer_model(), RowHeight::Medium, &flat(0.25));
    assert_eq!(quads.len(), 1, "一定の音は 1 本");
    assert_eq!(quads[0].rect.w, bar.w);
}
