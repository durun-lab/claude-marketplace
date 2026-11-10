use super::*;
use crate::timeline_layout::tests::TestViewport;
use crate::timeline_layout::{HEADER_W, RULER_H, TOOLBAR_H};

fn model() -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![
            Item::layer(0.0, 10.0, ItemKind::Text, true),
            Item::layer(2.0, 4.0, ItemKind::Footage, true),
        ],
        ..TimelineModel::default()
    }
}

/// 幅 1200、高さ 300 の Timeline。
fn viewport(m: &TimelineModel) -> TestViewport {
    TestViewport::new(m, (1200.0, 300.0))
}

#[test]
fn 目盛りの帯は_ruler() {
    let m = model();
    let f = viewport(&m);
    assert_eq!(hit_test(&m, &f.vp(), 300.0, TOOLBAR_H + 10.0), Hit::Ruler);
}

#[test]
fn 帯の左右の端はつまみ_中は本体() {
    let m = model();
    let f = viewport(&m);
    let y = f.in_bar(1);
    let at = |x| hit_test(&m, &f.vp(), x + HEADER_W, y);
    assert_eq!(
        at(201.0),
        Hit::Layer {
            index: 1,
            part: Part::InHandle
        }
    );
    assert_eq!(
        at(300.0),
        Hit::Layer {
            index: 1,
            part: Part::Body
        }
    );
    assert_eq!(
        at(399.0),
        Hit::Layer {
            index: 1,
            part: Part::OutHandle
        }
    );
    assert_eq!(at(450.0), Hit::Empty);
}

#[test]
fn 狭い帯ではつまみが幅の三分の一に収まる() {
    let mut m = model();
    m.items[1].end = 2.06; // 6 px
    let f = viewport(&m);
    let mid = hit_test(&m, &f.vp(), HEADER_W + 203.0, f.in_bar(1));
    assert!(matches!(
        mid,
        Hit::Layer {
            part: Part::Body,
            ..
        }
    ));
}

#[test]
fn 帯と帯の隙間は何も無い() {
    let m = model();
    let f = viewport(&m);
    let (top, h) = f.rows.bar(0);
    let y = TOOLBAR_H + RULER_H + top + h + 1.0;
    assert_eq!(hit_test(&m, &f.vp(), 300.0, y), Hit::Empty);
}

#[test]
fn スクロールした分だけ下の行に当たり_目盛りの帯の下に隠れた行には当たらない() {
    let items = (0..20)
        .map(|i| Item::layer(0.0, 10.0, ItemKind::Image, true).with_id(&i.to_string()))
        .collect();
    let m = TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items,
        ..TimelineModel::default()
    };
    let mut f = viewport(&m);
    f.scroll_y = 28.0 * 5.0;
    let y = f.in_bar(7);
    assert_eq!(
        hit_test(&m, &f.vp(), 300.0, y),
        Hit::Layer {
            index: 7,
            part: Part::Body
        }
    );
    assert_eq!(
        hit_test(&m, &f.vp(), 300.0, TOOLBAR_H + RULER_H - 1.0),
        Hit::Ruler
    );
}

#[test]
fn 縦のスクロールバーはスクロールボックスとそれ以外を分ける() {
    let items = vec![Item::layer(0.0, 10.0, ItemKind::Image, true); 50];
    let m = TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items,
        ..TimelineModel::default()
    };
    let f = viewport(&m);
    let thumb = f.vp().thumb();
    let x = f.layout.scrollbar.x + 4.0;
    let hit = |y| hit_test(&m, &f.vp(), x, y);
    assert_eq!(hit(thumb.y + 2.0), Hit::Scrollbar { thumb: true });
    assert_eq!(hit(thumb.bottom() + 10.0), Hit::Scrollbar { thumb: false });
}

#[test]
fn ズームバーはスクロールボックスの両端と中と溝を分ける() {
    let m = model();
    let mut f = viewport(&m);
    // 10 秒のうち 2 秒から 7 秒が見えている。スクロールボックスは溝の 2/10 から 7/10。
    f.time = TimeView {
        origin: 2.0,
        px_per_sec: f64::from(f.layout.row_area.w) / 5.0,
    };
    let bar = f.layout.zoombar;
    let (left, right) = (bar.x + bar.w * 0.2, bar.x + bar.w * 0.7);
    let at = |x| hit_test(&m, &f.vp(), x, bar.y + bar.h / 2.0);
    let part = |part| Hit::ZoomBar { part };
    assert_eq!(at(left - 100.0), part(ZoomBarPart::TrackBefore));
    assert_eq!(at(left + 3.0), part(ZoomBarPart::StartEdge));
    assert_eq!(at((left + right) / 2.0), part(ZoomBarPart::Thumb));
    assert_eq!(at(right - 3.0), part(ZoomBarPart::EndEdge));
    assert_eq!(at(right + 100.0), part(ZoomBarPart::TrackAfter));
    assert_eq!(at(HEADER_W / 2.0), Hit::Empty, "見出しの列の下は溝の外");
    let above = hit_test(&m, &f.vp(), (left + right) / 2.0, bar.y - 1.0);
    assert!(!matches!(above, Hit::ZoomBar { .. }), "行の領域の下端まで");
}

fn seq_model() -> TimelineModel {
    let cut = |start, end, gap| CutBar {
        start,
        end,
        gap,
        ..CutBar::default()
    };
    let mut seq = Item::layer(0.0, 10.0, ItemKind::Sequence, true);
    seq.cuts = vec![cut(0.0, 3.0, false), cut(3.0, 4.0, true)];
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![seq],
        ..TimelineModel::default()
    }
}

#[test]
fn シーケンスの上ではカットとその端に当たり_カットの外はレイヤーの本体() {
    let m = seq_model();
    let f = viewport(&m);
    let y = f.in_bar(0);
    let at = |x| hit_test(&m, &f.vp(), x + HEADER_W, y);
    let cut = |cut, part| Hit::Cut { item: 0, cut, part };
    assert_eq!(at(2.0), cut(0, Part::InHandle));
    assert_eq!(at(150.0), cut(0, Part::Body));
    assert_eq!(at(298.0), cut(0, Part::OutHandle));
    assert_eq!(at(350.0), cut(1, Part::Body), "隙間もカット");
    assert_eq!(
        at(700.0),
        Hit::Layer {
            index: 0,
            part: Part::Body
        }
    );
}

#[test]
fn カットの外にあるシーケンスのレイヤーの端はレイヤーのつまみ() {
    let m = seq_model();
    // 行の領域を 10 秒（1000 px）のレイヤーの右端まで見える幅にする。
    let f = TestViewport::new(&m, (HEADER_W + 1000.0, 300.0));
    let y = f.in_bar(0);
    let at = |x| hit_test(&m, &f.vp(), x + HEADER_W, y);
    let layer = |part| Hit::Layer { index: 0, part };
    assert_eq!(at(998.0), layer(Part::OutHandle), "カットは 4 秒まで");
    assert_eq!(at(700.0), layer(Part::Body));
    // 端がカットの中にあるときはカットのつまみが先。
    assert_eq!(
        at(2.0),
        Hit::Cut {
            item: 0,
            cut: 0,
            part: Part::InHandle
        }
    );
}

#[test]
fn 低い倍率の_1_フレームのカットは次のカットの左端のつまみを覆わない() {
    let mut m = seq_model();
    m.items[0].cuts[0].end = 1.0 / 30.0;
    m.items[0].cuts[1] = CutBar {
        start: 1.0 / 30.0,
        end: 4.0,
        ..CutBar::default()
    };
    let mut f = viewport(&m);
    f.time.px_per_sec = 10.0;
    let at = |x| hit_test(&m, &f.vp(), x + HEADER_W, f.in_bar(0));
    let cut = |cut, part| Hit::Cut { item: 0, cut, part };
    assert_eq!(at(1.0), cut(0, Part::Body), "1 フレームのカットも掴める");
    assert_eq!(
        at(5.0),
        cut(1, Part::InHandle),
        "次のカットの左端は次のカット"
    );
}

#[test]
fn 空の_sequence_の帯だけを再生位置の線が重なっても見える幅に広げる() {
    let mut m = model();
    m.items = vec![
        Item::layer(0.0, 1.0 / 30.0, ItemKind::Sequence, true),
        Item::layer(0.0, 1.0 / 30.0, ItemKind::Text, true),
    ];
    let mut f = viewport(&m);
    f.time.px_per_sec = 10.0;
    let vp = f.vp();
    let rect = |i: usize| bar_rect(&vp, i, &m.items[i], (0.0, 1.0 / 30.0));
    assert_eq!(rect(0).w, EMPTY_SEQUENCE_BAR_PX);
    assert_eq!(rect(1).w, MIN_BAR_PX, "ほかの短い帯は広げない");
    let at = |x: f32| hit_test(&m, &vp, x, f.in_bar(0));
    assert!(matches!(
        at(rect(0).right() - 1.0),
        Hit::Layer { index: 0, .. }
    ));
    assert_eq!(at(rect(0).right() + 1.0), Hit::Empty);
}
