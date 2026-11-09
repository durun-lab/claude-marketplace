use super::*;
use crate::geom::Quad;
use crate::labels::{timeline_labels, LABEL_PAD};
use crate::paint::tests::state;
use crate::paint::timeline_quads_split;
use crate::rows::{RowHeight, RowHeights, Rows};
use crate::text::tests::FakeMeasure;
use crate::text::UiFont;
use crate::timeline::{Item, ItemKind};
use crate::timeline_layout::tests::TestViewport;
use crate::timeline_layout::{TimelineLayout, HEADER_W, RULER_H};

const SIZE: (f32, f32) = (800.0, 200.0);

fn model(items: Vec<Item>) -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items,
        ..TimelineModel::default()
    }
}

fn footage(start: f64, end: f64) -> Item {
    Item::layer(start, end, ItemKind::Footage, true).with_thumb("hash-a")
}

fn thumbs(m: &TimelineModel, f: &TestViewport) -> Vec<ThumbQuad> {
    timeline_thumbs(m, &f.vp(), &state(0.0))
}

/// 全部の行を `height` にした見えている所。
fn with_heights(m: &TimelineModel, height: RowHeight) -> TestViewport {
    let mut f = TestViewport::new(m, SIZE);
    f.rows = Rows::of(
        &m.items,
        &RowHeights {
            all: Some(height),
            ..RowHeights::default()
        },
    );
    f.layout = TimelineLayout::new(SIZE, f.rows.content_height());
    f
}

#[test]
fn medium_の帯は左に帯の高さの_16_対_9_のサムネイルを出す() {
    let m = model(vec![footage(1.0, 4.0)]);
    let f = TestViewport::new(&m, SIZE);
    let bar = f.vp().bar(0, 1.0, 4.0);
    let t = thumbs(&m, &f);
    assert_eq!(t.len(), 1);
    assert_eq!(&*t[0].key, "hash-a");
    assert_eq!(bar.h, 25.0);
    assert_eq!(t[0].rect, Rect::new(bar.x, bar.y, 44.0, 25.0));
    assert_eq!(t[0].uv, Rect::new(0.0, 0.0, 1.0, 1.0));
    assert_eq!(t[0].alpha, 1.0);
}

#[test]
fn large_の帯は大きなサムネイルを出し_small_の行は出さない() {
    let m = model(vec![footage(0.0, 5.0)]);
    let large = thumbs(&m, &with_heights(&m, RowHeight::Large));
    assert_eq!((large[0].rect.w, large[0].rect.h), (94.0, 53.0));
    let small = thumbs(&m, &with_heights(&m, RowHeight::Small));
    assert!(small.is_empty());
}

#[test]
fn 帯の幅がサムネイルに足りなければ出さない() {
    let narrow = model(vec![footage(0.0, 0.4)]);
    assert!(thumbs(&narrow, &TestViewport::new(&narrow, SIZE)).is_empty());
    let fit = model(vec![footage(0.0, 0.5)]);
    assert_eq!(thumbs(&fit, &TestViewport::new(&fit, SIZE)).len(), 1);
}

#[test]
fn 鍵の無い帯と空きのカットには出さない() {
    let mut seq = Item::layer(0.0, 6.0, ItemKind::Sequence, true);
    let cut = |start, end, gap, key: Option<&str>| CutBar {
        start,
        end,
        gap,
        thumb: key.map(Into::into),
        ..CutBar::default()
    };
    seq.cuts = vec![
        cut(0.0, 2.0, false, Some("a")),
        cut(2.0, 3.0, true, Some("gap")),
        cut(3.0, 4.0, false, None),
        cut(4.0, 6.0, false, Some("b")),
    ];
    let m = model(vec![seq, Item::layer(0.0, 5.0, ItemKind::Text, true)]);
    let t = thumbs(&m, &TestViewport::new(&m, SIZE));
    let keys: Vec<&str> = t.iter().map(|q| &*q.key).collect();
    assert_eq!(keys, ["a", "b"], "カットごとに、そのカットの左へ");
    assert_eq!(t[1].rect.x, HEADER_W + 400.0);
}

#[test]
fn 帯の左端が画面の外に出てもサムネイルは見えている所の左に寄る() {
    let m = model(vec![footage(0.0, 8.0)]);
    let mut f = TestViewport::new(&m, SIZE);
    f.time.origin = 2.0;
    let t = thumbs(&m, &f);
    assert_eq!(t[0].rect.x, HEADER_W, "行の領域の左端");
}

#[test]
fn 目盛りの帯の下に隠れかけた行のサムネイルは行の領域で切り_サムネイルの側も切る() {
    let items = (0..10).map(|_| footage(0.0, 5.0)).collect();
    let m = model(items);
    let mut f = TestViewport::new(&m, SIZE);
    f.scroll_y = 10.0;
    let t = thumbs(&m, &f);
    let first = &t[0];
    assert_eq!(first.rect.y, RULER_H, "目盛りの帯の下から");
    let hidden = (RULER_H - f.vp().bar(0, 0.0, 5.0).y) / 25.0;
    assert!((first.uv.y - hidden).abs() < 1e-4, "{first:?}");
    assert!((first.uv.y + first.uv.h - 1.0).abs() < 1e-4);
    assert!(t.iter().all(|q| q.rect.bottom() <= SIZE.1));
}

#[test]
fn 無効なレイヤーのサムネイルは薄くする() {
    let m = model(vec![
        Item::layer(0.0, 5.0, ItemKind::Footage, false).with_thumb("a")
    ]);
    let t = thumbs(&m, &TestViewport::new(&m, SIZE));
    assert!(t[0].alpha < 1.0);
}

#[test]
fn サムネイルは帯の矩形の後で帯に重ねる矩形の前に描く() {
    let m = model(vec![footage(0.0, 5.0)]);
    let f = TestViewport::new(&m, SIZE);
    let mut st = state(0.0);
    st.selected = Some(0);
    let (quads, split) = timeline_quads_split(&m, &f.vp(), &st);
    let bar = f.vp().bar(0, 0.0, 5.0);
    let covers =
        |q: &Quad| q.rect.x >= bar.x && q.rect.y >= bar.y && q.rect.bottom() <= bar.bottom();
    assert!(
        quads[..split].iter().any(|q| q.rect == bar),
        "帯の矩形はサムネイルより前"
    );
    assert!(
        quads[split..].iter().any(|q| covers(q) && q.rect.w == 1.0),
        "選んだ外枠はサムネイルより後ろ"
    );
    assert_eq!(timeline_quads_split(&m, &f.vp(), &st).0.len(), quads.len());
}

#[test]
fn 名前はサムネイルの右から始める() {
    let label_x = |item: Item| {
        let m = model(vec![item.with_label("a.mp4")]);
        let f = TestViewport::new(&m, SIZE);
        let font = UiFont::default();
        let out = timeline_labels(&m, &f.vp(), &state(0.0), &font, &mut FakeMeasure::default());
        out.iter()
            .find(|l| &*l.text == "a.mp4")
            .map(|l| l.x)
            .unwrap()
    };
    let plain = label_x(Item::layer(1.0, 4.0, ItemKind::Footage, true));
    let with = label_x(footage(1.0, 4.0));
    assert_eq!(plain, HEADER_W + 100.0 + LABEL_PAD);
    assert_eq!(with, plain + 44.0);
}
