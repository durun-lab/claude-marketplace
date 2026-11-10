use super::*;
use crate::controller::{Action, Mods, PointerEvent, TimelineController};
use crate::geom::{Quad, QuadShape};
use crate::paint::bar_color;
use crate::paint::tests::state;
use crate::paint::timeline_quads;
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
        ..TimelineModel::default()
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
fn 見出しの部品は左から帯_略号_名前_有効_ロック_mute_solo_で_音を持てないレイヤーは名前が広い() {
    let row = Rect::new(0.0, 26.0, HEADER_W, 28.0);
    let plain = cells(row, &Item::layer(0.0, 1.0, ItemKind::Image, true));
    let voice = cells(
        row,
        &Item::layer(0.0, 1.0, ItemKind::Footage, true).with_audio(false, false),
    );
    for c in [plain, voice] {
        assert!(c.kind_strip.right() <= c.abbr.x && c.abbr.right() <= c.name.x);
        assert!(c.name.right() <= c.enabled.x && c.enabled.right() <= c.lock.x);
        assert_eq!(c.lock.right().min(row.right()), c.lock.right());
    }
    assert_eq!((plain.mute, plain.solo), (None, None));
    let (mute, solo) = (voice.mute.unwrap(), voice.solo.unwrap());
    assert!(voice.lock.right() <= mute.x && mute.right() <= solo.x);
    assert!(solo.right() <= row.right());
    assert!(plain.name.w > voice.name.w);
    assert_eq!(
        plain.kind_strip.y,
        row.y + 1.0,
        "行の上端の 1 px は行の境界の線"
    );
}

#[test]
fn キーフレームの開閉は見出しの右端に置き_キーフレームのあるレイヤーだけに描く() {
    let row = Rect::new(0.0, 26.0, HEADER_W, 28.0);
    let voice = Item::layer(0.0, 1.0, ItemKind::Footage, true).with_audio(false, false);
    let c = cells(row, &voice);
    assert!(c.solo.unwrap().right() <= c.disclosure.x);
    assert!(c.disclosure.right() <= row.right());
    let m = model(vec![
        Item::layer(0.0, 5.0, ItemKind::Text, true).with_keyframes(&[1.0]),
        Item::layer(0.0, 5.0, ItemKind::Image, true),
    ]);
    let f = TestViewport::new(&m, SIZE);
    let vp = f.vp();
    let tris: Vec<Rect> = timeline_quads(&m, &vp, &state(0.0))
        .into_iter()
        .filter(|q| q.shape == QuadShape::TriRight)
        .map(|q| q.rect)
        .collect();
    let first = cells(row_rect(&vp, 0), &m.items[0]).disclosure;
    assert_eq!(tris.len(), 1, "キーフレームのある行だけ");
    assert!(tris[0].x >= first.x && tris[0].right() <= first.right());
    assert!(tris[0].y >= first.y && tris[0].bottom() <= first.bottom());
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
        assert_eq!(bar_color(&on), item_color(kind));
        let off = bar_color(&Item::layer(0.0, 1.0, kind, false));
        assert!(off.0[3] < 1.0, "{kind:?}");
    }
}

#[test]
fn タイムコードは_時_分_秒_フレームで_フレームは時刻を含むフレーム() {
    assert_eq!(timecode(0.0, 30.0), "00:00:00:00");
    assert_eq!(timecode(83.0 + 12.0 / 30.0, 30.0), "00:01:23:12");
    assert_eq!(timecode(3661.5, 30.0), "01:01:01:15");
    assert_eq!(
        timecode(2.0 + 5.9 / 30.0, 30.0),
        "00:00:02:05",
        "フレームの途中は切り捨て"
    );
    assert_eq!(timecode(-1.0, 30.0), "00:00:00:00");
    assert_eq!(
        timecode(1.0, 0.0),
        "00:00:01:00",
        "fps が 0 でもおかしくならない"
    );
    assert_eq!(
        timecode(83.5, 30000.0 / 1001.0),
        "00:01:23:12",
        "29.97 fps は 29.97 で数えたフレームを 1 秒 30 フレームで書く（ノンドロップ）"
    );
}

#[test]
fn 見出しの文字は略号と名前_音を持てるレイヤーは_m_と_s_で_名前は_id() {
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
fn mute_と_solo_の字は入なら背景の上で読める字_切なら薄い字() {
    let m = model(vec![Item::layer(0.0, 5.0, ItemKind::Footage, true)
        .with_id("v1")
        .with_audio(true, false)]);
    let out = labels_of(&m, 0.0);
    let color = |text: &str| out.iter().find(|l| &*l.text == text).expect(text).color;
    let p = Palette::DEFAULT;
    assert_eq!(color("M"), text_on(p.mute), "入");
    assert_eq!(color("S"), mark_color(&p, false), "切");
    assert!(color("S").0[3] < 1.0);
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
fn 見出しの列の上の角に再生位置のタイムコードを出す() {
    let m = model(vec![Item::layer(0.0, 5.0, ItemKind::Text, true)]);
    let out = labels_of(&m, 2.0 + 5.0 / 30.0);
    let t = out
        .iter()
        .find(|l| &*l.text == "00:00:02:05")
        .expect("タイムコード");
    let corner = Rect::new(0.0, 0.0, HEADER_W, RULER_H);
    assert_eq!(t.clip, corner);
    assert!(t.x >= corner.x && t.y >= corner.y);
}

#[test]
fn 見出しの上に当たると_header_見出しの列の上の角は_corner() {
    let m = model(vec![
        Item::layer(0.0, 5.0, ItemKind::Text, true),
        Item::layer(0.0, 5.0, ItemKind::Image, true),
    ]);
    let f = TestViewport::new(&m, SIZE);
    let vp = f.vp();
    let y1 = f.in_bar(1);
    assert_eq!(hit_test(&m, &vp, 20.0, y1), Hit::Header { index: 1 });
    assert_eq!(hit_test(&m, &vp, 20.0, 5.0), Hit::Corner);
    assert_eq!(hit_test(&m, &vp, 20.0, 190.0), Hit::Empty, "行の無い所");
    assert!(matches!(
        hit_test(&m, &vp, HEADER_W + 50.0, y1),
        Hit::Layer { index: 1, .. }
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

#[test]
fn 見出しの上の_ctrl_ホイールは行の領域の左端を中心に拡大する() {
    let m = many(3);
    let mut c = controller(&m);
    let left = c.view.t_of(0.0);
    let zoom = PointerEvent::Wheel {
        x: 20.0,
        y: 60.0,
        dy: 2.0,
        ctrl: true,
        shift: false,
    };
    c.handle(zoom, &m, 0.0);
    assert!(c.view.px_per_sec > 100.0);
    assert!((c.view.t_of(0.0) - left).abs() < 1e-9);
}

#[test]
fn 見出しの列の上の角のクリックは選択を変えない() {
    let m = many(3);
    let mut c = controller(&m);
    let click = |x: f32, y: f32| PointerEvent::Down {
        x,
        y,
        mods: Mods::default(),
    };
    let y = c.viewport().row(1).y + 5.0;
    c.handle(click(20.0, y), &m, 0.0);
    c.handle(PointerEvent::Up, &m, 0.0);
    assert!(c.handle(click(20.0, 5.0), &m, 0.0).is_empty());
    assert_eq!(c.paint_state(0.0).selected, Some(1));
}

/// 行 `i` の見出しのボタンの中心を押す。
fn press_button(
    c: &mut TimelineController,
    m: &TimelineModel,
    (i, button): (usize, HeaderButton),
) -> Vec<Action> {
    let (x, y) = button_center(&c.viewport(), &m.items[i], i, button).expect("ボタンがある");
    let mods = Mods::default();
    c.handle(PointerEvent::Down { x, y, mods }, m, 0.0)
}

#[test]
fn ボタンを押すと見出しのボタンの操作になり_レイヤーの選択は変わらない() {
    let m = model(vec![
        Item::layer(0.0, 5.0, ItemKind::Footage, true).with_audio(false, false),
        Item::layer(0.0, 5.0, ItemKind::Image, true),
    ]);
    let mut c = controller(&m);
    for (index, button) in [
        (0, HeaderButton::Enabled),
        (0, HeaderButton::Lock),
        (0, HeaderButton::Mute),
        (0, HeaderButton::Solo),
        (1, HeaderButton::Enabled),
        (1, HeaderButton::Lock),
    ] {
        let acts = press_button(&mut c, &m, (index, button));
        assert_eq!(acts, vec![Action::HeaderButton { index, button }]);
        assert_eq!(c.selected, None, "選ばない");
    }
}

#[test]
fn 音を持てないレイヤーに_mute_と_solo_のボタンは無い() {
    let m = model(vec![Item::layer(0.0, 5.0, ItemKind::Image, true)]);
    let f = TestViewport::new(&m, SIZE);
    for b in [HeaderButton::Mute, HeaderButton::Solo] {
        assert_eq!(button_center(&f.vp(), &m.items[0], 0, b), None);
    }
}

#[test]
fn ボタンの上のホバーは_header_button_で_名前の上は_header() {
    let m = model(vec![Item::layer(0.0, 5.0, ItemKind::Image, true)]);
    let f = TestViewport::new(&m, SIZE);
    let vp = f.vp();
    let (x, y) = button_center(&vp, &m.items[0], 0, HeaderButton::Lock).unwrap();
    let lock = Hit::HeaderButton {
        index: 0,
        button: HeaderButton::Lock,
    };
    assert_eq!(hit_test(&m, &vp, x, y), lock);
    let (nx, ny) = name_center(&vp, &m.items[0], 0);
    assert_eq!(hit_test(&m, &vp, nx, ny), Hit::Header { index: 0 });
}

#[test]
fn ポインターが乗ったボタンは背景を明るくする() {
    let m = model(vec![Item::layer(0.0, 5.0, ItemKind::Image, true)]);
    let f = TestViewport::new(&m, SIZE);
    let (x, y) = button_center(&f.vp(), &m.items[0], 0, HeaderButton::Lock).unwrap();
    let hover = Hit::HeaderButton {
        index: 0,
        button: HeaderButton::Lock,
    };
    let plain = timeline_quads(&m, &f.vp(), &state(0.0));
    let hot = PaintState {
        hover,
        ..state(0.0)
    };
    let hot = timeline_quads(&m, &f.vp(), &hot);
    let behind = |quads: &[Quad]| {
        quads
            .iter()
            .filter(|q| q.rect.contains(x, y) && q.rect.w <= BUTTON_W + 0.5)
            .count()
    };
    assert!(behind(&hot) > behind(&plain));
}

#[test]
fn 無効なレイヤーの見出しに斜線を引く_有効なら引かない() {
    let on = model(vec![Item::layer(0.0, 5.0, ItemKind::Image, true)]);
    let off = model(vec![Item::layer(0.0, 5.0, ItemKind::Image, false)]);
    let count = |m: &TimelineModel| {
        let f = TestViewport::new(m, SIZE);
        let quads = timeline_quads(m, &f.vp(), &state(0.0));
        let dot = |q: &&Quad| q.rect.right() <= HEADER_W && q.rect.w <= 3.0 && q.rect.h <= 3.0;
        quads.iter().filter(dot).count()
    };
    assert!(count(&off) > count(&on) + 20, "斜線の点が並ぶ");
}

#[test]
fn 隠した行があれば角に隠した数を出す_無ければ出さない() {
    let mut m = model(vec![Item::layer(0.0, 5.0, ItemKind::Text, true)]);
    assert!(!labels_of(&m, 0.0)
        .iter()
        .any(|l| l.text.ends_with("hidden")));
    m.hidden = 4;
    let out = labels_of(&m, 0.0);
    let hidden = out.iter().find(|l| &*l.text == "4 hidden").expect("数");
    assert_eq!(hidden.clip, Rect::new(0.0, 0.0, HEADER_W, RULER_H));
    let time = out.iter().find(|l| &*l.text == "00:00:00:00").unwrap();
    assert!(hidden.x > time.x, "タイムコードの右");
}

#[test]
fn 見出しのドラッグで入る位置に線を出す() {
    let m = many(3);
    let f = TestViewport::new(&m, SIZE);
    let plain = timeline_quads(&m, &f.vp(), &state(0.0)).len();
    let st = PaintState {
        header_drop: Some(1),
        ..state(0.0)
    };
    let quads = timeline_quads(&m, &f.vp(), &st);
    assert_eq!(quads.len(), plain + 2, "見出しの列と行の領域に 1 本ずつ");
    let y = f.vp().row(1).y;
    let on_edge = |q: &Quad| (q.rect.y + q.rect.h / 2.0 - y).abs() < 0.01;
    assert!(quads
        .iter()
        .any(|q| q.rect.x == 0.0 && q.rect.w == HEADER_W && on_edge(q)));
}
