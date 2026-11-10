//! 右クリックのメニューの前処理のテスト。ポインターの下のカットとレイヤーを先に選ぶこと、当たった所、割る時刻、メニューキーの位置。
//! テストの x は行の領域の左端を 0 とする（1 秒 100 px）。

use super::tests::{ctl, down, in_bar};
use super::*;
use crate::pointed::Place;
use crate::timeline::{CutBar, Item, ItemKind};
use crate::timeline_layout::HEADER_W;

const PLAYHEAD: f64 = 7.0;

/// 先頭がシーケンスのレイヤー（カット 0-3、4-7 秒）、次が撮影素材のレイヤー（8-9.5 秒）。
fn model() -> TimelineModel {
    let cut = |start, end| CutBar {
        start,
        end,
        ..CutBar::default()
    };
    let mut seq = Item::layer(0.0, 20.0, ItemKind::Sequence, true);
    seq.cuts = vec![cut(0.0, 3.0), cut(4.0, 7.0)];
    TimelineModel {
        duration: 20.0,
        fps: 30.0,
        items: vec![seq, Item::layer(8.0, 9.5, ItemKind::Footage, true)],
        ..TimelineModel::default()
    }
}

fn right_click(c: &mut TimelineController, m: &TimelineModel, x: f32, y: f32) -> Vec<Action> {
    let ev = PointerEvent::ContextMenu { x: x + HEADER_W, y };
    c.handle(ev, m, PLAYHEAD)
}

fn menu_of(actions: &[Action]) -> (Place, f64, (f32, f32)) {
    match actions.last() {
        Some(Action::ContextMenu { hit, time, at }) => {
            (Place::of(hit).expect("メニューを出す所"), *time, *at)
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn カットの右クリックはレイヤーとカットを選んでからカットのメニューを出す() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    let actions = right_click(&mut c, &m, 500.0, y);
    assert_eq!(
        actions[..2],
        [Action::Select(Some(0)), Action::SelectCut(Some((0, 1)))]
    );
    let (place, time, at) = menu_of(&actions);
    assert_eq!(place, Place::Cut);
    assert!((time - 5.0).abs() < 1e-9, "{time}");
    assert_eq!(at, (500.0 + HEADER_W, y));
    assert_eq!((c.selected, c.selected_cut), (Some(0), Some((0, 1))));
}

#[test]
fn 別のカットを選んでいても右クリックしたカットに選び直す() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    c.handle(down(100.0, y), &m, PLAYHEAD);
    assert_eq!(c.selected_cut, Some((0, 0)));
    right_click(&mut c, &m, 500.0, y);
    assert_eq!(c.selected_cut, Some((0, 1)));
}

#[test]
fn レイヤーの帯の右クリックはカットの選択を外してレイヤーを選ぶ() {
    let m = model();
    let mut c = ctl(&m);
    let y0 = in_bar(&c, 0);
    right_click(&mut c, &m, 100.0, y0);
    let y1 = in_bar(&c, 1);
    let actions = right_click(&mut c, &m, 900.0, y1);
    assert_eq!(
        actions[..2],
        [Action::Select(Some(1)), Action::SelectCut(None)]
    );
    assert_eq!(menu_of(&actions).0, Place::Layer);
    assert_eq!((c.selected, c.selected_cut), (Some(1), None));
}

#[test]
fn 見出しの右クリックはレイヤーを選んで見出しのメニューを出す() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    let ev = PointerEvent::ContextMenu { x: 20.0, y };
    let actions = c.handle(ev, &m, PLAYHEAD);
    assert_eq!(actions[0], Action::Select(Some(1)));
    assert_eq!(menu_of(&actions).0, Place::Header);
    assert_eq!(c.selected, Some(1));
}

#[test]
fn 目盛りと空きの右クリックは選択を変えない() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    right_click(&mut c, &m, 500.0, y);
    let ruler = c.layout().ruler;
    let actions = right_click(&mut c, &m, 300.0, ruler.y + ruler.h / 2.0);
    assert_eq!(actions.len(), 1, "選択の操作は出さない");
    assert_eq!(menu_of(&actions).0, Place::Ruler);
    let empty = c.layout().row_area.bottom() - 2.0;
    let actions = right_click(&mut c, &m, 1500.0, empty);
    assert_eq!(actions.len(), 1);
    assert_eq!(menu_of(&actions).0, Place::Empty);
    assert_eq!(c.selected_cut, Some((0, 1)), "選んでいたカットのまま");
}

#[test]
fn 割る時刻はフレームにそろえ_スナップが入なら再生位置に吸い付く() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    c.snapping = false;
    // 6.93 秒は 30fps の 208 フレーム目の手前（6.9333…）。
    let (_, time, _) = menu_of(&right_click(&mut c, &m, 693.0, y));
    assert!((time - 208.0 / 30.0).abs() < 1e-9, "{time}");
    c.snapping = true;
    let (_, time, _) = menu_of(&right_click(&mut c, &m, 698.0, y));
    assert_eq!(time, PLAYHEAD, "再生位置の 6 px 手前は再生位置に吸い付く");
}

#[test]
fn メニューを出さない所とドラッグ中の右クリックは何も返さない() {
    let m = model();
    let mut c = ctl(&m);
    let corner = c.layout().corner;
    let ev = PointerEvent::ContextMenu {
        x: corner.x + 1.0,
        y: corner.y + 1.0,
    };
    assert!(c.handle(ev, &m, PLAYHEAD).is_empty(), "見出しの上の角");
    let y = in_bar(&c, 1);
    c.handle(down(900.0, y), &m, PLAYHEAD);
    assert!(right_click(&mut c, &m, 500.0, y).is_empty(), "ドラッグ中");
}

#[test]
fn メニューキーは選んでいるカットの再生位置か中央で出す() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    right_click(&mut c, &m, 500.0, y);
    let actions = c.handle(PointerEvent::ContextMenuKey, &m, 6.0);
    let (place, time, at) = menu_of(&actions);
    assert_eq!((place, actions.len()), (Place::Cut, 1));
    assert_eq!(time, 6.0, "再生位置はカット 4-7 秒の中");
    assert!(
        c.viewport().bar(0, 0.0, 20.0).contains(at.0, at.1),
        "{at:?}"
    );
    c.handle(down(100.0, y), &m, 1.0);
    let (_, time, _) = menu_of(&c.handle(PointerEvent::ContextMenuKey, &m, 9.0));
    assert!(
        (time - 1.5).abs() < 1e-9,
        "再生位置がカットの外ならカットの中央: {time}"
    );
}

#[test]
fn メニューキーはレイヤーだけ選んでいればレイヤーのメニュー_何も選んでいなければ空きのメニュー() {
    let m = model();
    let mut c = ctl(&m);
    let (place, time, at) = menu_of(&c.handle(PointerEvent::ContextMenuKey, &m, PLAYHEAD));
    assert_eq!(place, Place::Empty);
    assert_eq!(time, PLAYHEAD);
    let ruler = c.layout().ruler;
    assert!(at.1 > ruler.bottom(), "目盛りの帯の下: {at:?}");
    let y = in_bar(&c, 1);
    c.handle(down(900.0, y), &m, PLAYHEAD);
    c.handle(PointerEvent::Up, &m, PLAYHEAD);
    let (place, time, _) = menu_of(&c.handle(PointerEvent::ContextMenuKey, &m, PLAYHEAD));
    assert_eq!(place, Place::Layer);
    assert!(
        (8.0..=9.5).contains(&time),
        "再生位置を帯の中に収める: {time}"
    );
}
