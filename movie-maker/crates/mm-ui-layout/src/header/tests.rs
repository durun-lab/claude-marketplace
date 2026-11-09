use super::*;
use crate::controller::{Action, Mods, PointerEvent, TimelineController};
use crate::paint::tests::state;
use crate::palette::{item_color, tests::KINDS};
use crate::rows::{RowHeights, Rows};
use crate::text::tests::FakeMeasure;
use crate::time_view::TimeView;
use crate::timeline::{hit_test, Hit};
use crate::timeline_layout::tests::TestViewport;
use crate::timeline_layout::{HEADER_W, RULER_H};

/// 幅 800、高さ 200 の Timeline。
const SIZE: (f32, f32) = (800.0, 200.0);

fn model(items: Vec<Item>) -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items,
    }
}

fn labels_of(m: &TimelineModel, playhead: f64) -> Vec<Label> {
    let f = TestViewport::new(m, SIZE);
    let font = UiFont::default();
    labels(
        m,
        &f.vp(),
        &state(playhead),
        &font,
        &mut FakeMeasure::default(),
    )
}

#[test]
fn 見出しの列は幅_180_で_行の領域と目盛りの帯はその右から始まる() {
    let m = model(vec![Item::layer(0.0, 5.0, ItemKind::Text, true)]);
    let l = TestViewport::new(&m, SIZE).layout;
    assert_eq!(HEADER_W, 180.0);
    assert_eq!(l.headers.x, 0.0);
    assert_eq!(l.headers.w, HEADER_W);
    assert_eq!(l.row_area.x, HEADER_W);
    assert_eq!(l.row_area.w, SIZE.0 - HEADER_W);
    assert_eq!((l.ruler.x, l.ruler.w), (HEADER_W, SIZE.0 - HEADER_W));
    assert_eq!(l.corner, Rect::new(0.0, 0.0, HEADER_W, RULER_H));
}

#[test]
fn 部品は左から帯_略号_名前_有効_ロック_mute_solo_で_音の無いレイヤーは名前が広い() {
    let row = Rect::new(0.0, 26.0, HEADER_W, 28.0);
    let plain = cells(row, &Item::layer(0.0, 1.0, ItemKind::Image, true));
    let voice = cells(
        row,
        &Item::layer(0.0, 1.0, ItemKind::Footage, true).with_audio(false, false),
    );
    for c in [plain, voice] {
        assert!(c.stripe.right() <= c.abbr.x && c.abbr.right() <= c.name.x);
        assert!(c.name.right() <= c.enabled.x && c.enabled.right() <= c.lock.x);
        assert_eq!(c.lock.right().min(row.right()), c.lock.right());
    }
    assert_eq!((plain.mute, plain.solo), (None, None));
    let (mute, solo) = (voice.mute.unwrap(), voice.solo.unwrap());
    assert!(voice.lock.right() <= mute.x && mute.right() <= solo.x);
    assert!(solo.right() <= row.right());
    assert!(plain.name.w > voice.name.w);
    assert_eq!(plain.stripe.y, row.y + 1.0, "行の上端の 1 px は境の線");
}

#[test]
fn 略号は種類ごとに決まり_other_は無い() {
    let got: Vec<_> = [
        ItemKind::Footage,
        ItemKind::Image,
        ItemKind::Text,
        ItemKind::Solid,
        ItemKind::Composition,
        ItemKind::Sequence,
        ItemKind::Other,
    ]
    .map(abbreviation)
    .to_vec();
    assert_eq!(
        got,
        [
            Some("V"),
            Some("I"),
            Some("T"),
            Some("S"),
            Some("C"),
            Some("Seq"),
            None
        ]
    );
}

#[test]
fn 種類の色の帯は種類の色で_無効なレイヤーは薄い() {
    for kind in KINDS {
        let on = Item::layer(0.0, 1.0, kind, true);
        assert_eq!(crate::paint::bar_color(&on), item_color(kind));
    }
}

#[test]
fn 時刻は_時_分_秒_フレームで出す() {
    assert_eq!(timecode(0.0, 30.0), "00:00:00:00");
    assert_eq!(timecode(83.0 + 12.0 / 30.0, 30.0), "00:01:23:12");
    assert_eq!(timecode(3661.5, 30.0), "01:01:01:15");
    assert_eq!(timecode(-1.0, 30.0), "00:00:00:00");
    assert_eq!(timecode(1.0, 0.0), "00:00:01:00", "fps が 0 でも壊れない");
}

#[test]
fn 見出しの文字は略号と名前_音のあるレイヤーは_m_と_s_で_名前は_id() {
    let m = model(vec![
        Item::layer(0.0, 5.0, ItemKind::Text, true).with_id("fxtext-32"),
        Item::layer(0.0, 5.0, ItemKind::Footage, true)
            .with_id("v1")
            .with_audio(true, false),
    ]);
    let out = labels_of(&m, 0.0);
    let texts: Vec<&str> = out.iter().map(|l| &*l.text).collect();
    for want in ["T", "fxtext-32", "V", "v1", "M", "S"] {
        assert!(texts.contains(&want), "{want} in {texts:?}");
    }
    assert_eq!(texts.iter().filter(|t| **t == "M").count(), 1);
}

#[test]
fn 長い名前は切り詰めて_見出しの列の外に描かない() {
    let m = model(vec![Item::layer(0.0, 5.0, ItemKind::Text, true)
        .with_id("a-very-very-long-layer-identifier-0123456789")
        .with_audio(false, false)]);
    let out = labels_of(&m, 0.0);
    let name = out
        .iter()
        .find(|l| l.text.starts_with("a-very"))
        .expect("名前");
    assert!(name.cut.is_some(), "切り詰め");
    assert!(name.clip.right() <= HEADER_W);
    let c = cells(row_rect(&TestViewport::new(&m, SIZE).vp(), 0), &m.items[0]);
    assert!(name.x + name.cut.unwrap() <= c.name.right());
}

#[test]
fn 角に再生位置の時刻を出す() {
    let m = model(vec![Item::layer(0.0, 5.0, ItemKind::Text, true)]);
    let out = labels_of(&m, 2.0 + 5.0 / 30.0);
    let t = out
        .iter()
        .find(|l| &*l.text == "00:00:02:05")
        .expect("時刻");
    let corner = Rect::new(0.0, 0.0, HEADER_W, RULER_H);
    assert_eq!(t.clip, corner);
    assert!(t.x >= corner.x && t.y >= corner.y);
}

#[test]
fn 見出しの上に当たると_header_角は_corner() {
    let m = model(vec![
        Item::layer(0.0, 5.0, ItemKind::Text, true),
        Item::layer(0.0, 5.0, ItemKind::Image, true),
    ]);
    let f = TestViewport::new(&m, SIZE);
    let vp = f.vp();
    let y1 = vp.row(1).y + 5.0;
    assert_eq!(hit_test(&m, &vp, 20.0, y1), Hit::Header { index: 1 });
    assert_eq!(hit_test(&m, &vp, 20.0, 5.0), Hit::Corner);
    assert_eq!(hit_test(&m, &vp, 20.0, 190.0), Hit::Empty, "行の無い所");
    assert!(matches!(
        hit_test(&m, &vp, HEADER_W + 10.0, y1),
        Hit::Layer { index: 1, .. } | Hit::Empty
    ));
}

fn controller(m: &TimelineModel) -> TimelineController {
    let mut c = TimelineController::new(TimeView {
        origin: 0.0,
        px_per_sec: 100.0,
    });
    c.set_rows(Rows::of(&m.items, &RowHeights::default()));
    c.resize((800.0, 100.0));
    c
}

fn many(n: usize) -> TimelineModel {
    model(
        (0..n)
            .map(|_| Item::layer(0.0, 10.0, ItemKind::Image, true))
            .collect(),
    )
}

#[test]
fn 見出しのクリックはレイヤーを選ぶ_帯のクリックと同じ操作() {
    let m = many(3);
    let mut c = controller(&m);
    let y = c.viewport().row(2).y + 5.0;
    let down = PointerEvent::Down {
        x: 20.0,
        y,
        mods: Mods::default(),
    };
    let acts = c.handle(down, &m, 0.0);
    assert_eq!(acts, vec![Action::Select(Some(2))]);
    assert_eq!(c.paint_state(0.0).selected, Some(2));
}

#[test]
fn 見出しの上のホイールは縦にスクロールし_横は動かさない() {
    let m = many(40);
    let mut c = controller(&m);
    let view = c.view;
    let wheel = PointerEvent::Wheel {
        x: 20.0,
        y: 60.0,
        dy: -2.0,
        ctrl: false,
        shift: false,
    };
    c.handle(wheel, &m, 0.0);
    assert!(c.scroll_y > 0.0);
    assert_eq!(c.view, view);
}
