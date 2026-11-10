//! 右クリックのメニューの前処理のテスト。ポインターの下のものを先に選ぶこと、当たった所、割る時刻、メニューキーの位置。
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
    right_click_with(c, m, (x, y), PLAYHEAD)
}

fn right_click_with(
    c: &mut TimelineController,
    m: &TimelineModel,
    (x, y): (f32, f32),
    playhead: f64,
) -> Vec<Action> {
    let ev = PointerEvent::ContextMenu { x: x + HEADER_W, y };
    c.handle(ev, m, playhead)
}

fn menu_of(actions: &[Action]) -> (Place, Option<f64>, (f32, f32)) {
    match actions.last() {
        Some(Action::ContextMenu { hit, time, at }) => {
            (Place::of(hit).expect("メニューを出す所"), *time, *at)
        }
        other => panic!("{other:?}"),
    }
}

fn near(time: Option<f64>, want: f64) -> bool {
    time.is_some_and(|t| (t - want).abs() < 1e-9)
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
    assert!(near(time, 5.0), "{time:?}");
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
fn レイヤーの帯の右クリックはカットの選択を外してレイヤーを選び_割る時刻を持たない() {
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
    assert_eq!(menu_of(&actions).1, None);
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
    assert!(near(time, 208.0 / 30.0), "{time:?}");
    c.snapping = true;
    let (_, time, _) = menu_of(&right_click_with(&mut c, &m, (605.0, y), 6.0));
    assert_eq!(time, Some(6.0), "再生位置の 5 px 先は再生位置に吸い付く");
}

#[test]
fn 吸い付いた再生位置がカットの端か外なら_右クリックしたカットを割る時刻を持たない() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    c.snapping = true;
    let (place, time, _) = menu_of(&right_click(&mut c, &m, 698.0, y));
    assert_eq!(
        (place, time),
        (Place::Cut, None),
        "再生位置 7 秒はカット 4-7 秒の端"
    );
    let (place, time, _) = menu_of(&right_click_with(&mut c, &m, (299.0, y), 3.04));
    assert_eq!(
        (place, time),
        (Place::Cut, None),
        "3.04 秒はカット 0-3 秒の外"
    );
    assert_eq!(c.selected_cut, Some((0, 0)), "右クリックしたカットは選ぶ");
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
    assert_eq!(time, Some(6.0), "再生位置はカット 4-7 秒の中");
    assert!(
        c.viewport().bar(0, 0.0, 20.0).contains(at.0, at.1),
        "{at:?}"
    );
    let (_, time, _) = menu_of(&c.handle(PointerEvent::ContextMenuKey, &m, 4.0));
    assert!(
        near(time, 5.5),
        "カットの始まりでは割れないので中央: {time:?}"
    );
    c.handle(down(100.0, y), &m, 1.0);
    let (_, time, _) = menu_of(&c.handle(PointerEvent::ContextMenuKey, &m, 9.0));
    assert!(
        near(time, 1.5),
        "再生位置がカットの外ならカットの中央: {time:?}"
    );
}

#[test]
fn メニューキーの点は_スクロールで隠れたカットを選んでいても行の領域に収める() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    c.handle(down(100.0, y), &m, 1.0);
    c.handle(PointerEvent::Up, &m, 1.0);
    c.view.origin = 15.0;
    let (place, _, at) = menu_of(&c.handle(PointerEvent::ContextMenuKey, &m, 1.0));
    let area = c.layout().row_area;
    assert_eq!(place, Place::Cut);
    assert_eq!(at.0, area.x, "カット 0-3 秒は左に隠れている");
    assert!((area.y..=area.bottom()).contains(&at.1), "{at:?}");
}

#[test]
fn メニューキーはレイヤーだけ選んでいればレイヤーのメニュー_何も選んでいなければ空きのメニュー() {
    let m = model();
    let mut c = ctl(&m);
    let (place, time, at) = menu_of(&c.handle(PointerEvent::ContextMenuKey, &m, PLAYHEAD));
    assert_eq!((place, time), (Place::Empty, None));
    let ruler = c.layout().ruler;
    assert!(at.1 > ruler.bottom(), "目盛りの帯の下: {at:?}");
    let y = in_bar(&c, 1);
    c.handle(down(900.0, y), &m, PLAYHEAD);
    c.handle(PointerEvent::Up, &m, PLAYHEAD);
    let (place, time, at) = menu_of(&c.handle(PointerEvent::ContextMenuKey, &m, PLAYHEAD));
    assert_eq!((place, time), (Place::Layer, None));
    let bar = c.viewport().bar(1, 8.0, 9.5);
    assert!(
        (bar.x..=bar.right()).contains(&at.0),
        "再生位置を帯の中に収める: {at:?}"
    );
}

mod keyframes {
    use super::*;
    use crate::keyframe_rows::tests::model;
    use crate::keyframe_rows::{keyframe_center, KeyframeAction, KeyframeIndex, RowPick};
    use crate::rows::{RowHeights, Rows};
    use crate::time_view::TimeView;

    /// title のキーフレームの行（Opacity 1、4 秒、Transform › Position 2 秒）を開いた Controller。1 秒 100 px。
    fn opened(m: &TimelineModel) -> TimelineController {
        let mut c = TimelineController::new(TimeView {
            origin: 0.0,
            px_per_sec: 100.0,
        });
        let mut heights = RowHeights::default();
        heights.keyframe_rows_open.insert("title".into());
        c.set_rows(Rows::of(&m.items, &heights));
        c.resize((1000.0, 300.0));
        c
    }

    fn click(c: &mut TimelineController, m: &TimelineModel, (x, y): (f32, f32)) -> Vec<Action> {
        c.handle(PointerEvent::ContextMenu { x, y }, m, 0.0)
    }

    fn pick(row: usize, keyframe: Option<usize>) -> Action {
        let pick = RowPick {
            item: 0,
            row,
            keyframe,
        };
        Action::Keyframe(KeyframeAction::Select(Some(pick)))
    }

    #[test]
    fn ひし形の右クリックはひし形を選んでキーフレームのメニューを出し_ドラッグは始めない() {
        let m = model();
        let mut c = opened(&m);
        let p = keyframe_center(&c.viewport(), (0, 0), 4.0);
        let actions = click(&mut c, &m, p);
        assert_eq!(actions[0], Action::Select(Some(0)));
        assert!(actions.contains(&pick(0, Some(1))), "{actions:?}");
        assert_eq!(menu_of(&actions), (Place::Keyframe, None, p));
        let moved = PointerEvent::Move {
            x: p.0 + 50.0,
            y: p.1,
        };
        assert!(c.handle(moved, &m, 0.0).is_empty());
        assert_eq!(
            c.paint_state(0.0).keyframe_rows.dragging,
            None,
            "動かさない"
        );
    }

    #[test]
    fn キーフレームの行のひし形の無い所は行だけを選んで行のメニューを出す() {
        let m = model();
        let mut c = opened(&m);
        let p = keyframe_center(&c.viewport(), (0, 1), 6.0);
        let actions = click(&mut c, &m, p);
        assert!(actions.contains(&pick(1, None)), "{actions:?}");
        assert_eq!(menu_of(&actions).0, Place::KeyframeRow);
    }

    #[test]
    fn レイヤーの帯の右クリックはキーフレームの行の選択を外し_目盛りの右クリックは残す() {
        let m = model();
        let mut c = opened(&m);
        let p = keyframe_center(&c.viewport(), (0, 0), 1.0);
        click(&mut c, &m, p);
        let ruler = c.layout().ruler;
        let actions = click(&mut c, &m, (p.0, ruler.y + ruler.h / 2.0));
        assert_eq!(actions.len(), 1, "選択の操作は出さない");
        assert!(c.keyframe_pick().is_some());
        let bar = c.viewport().bar(1, 0.0, 8.0);
        let actions = click(&mut c, &m, (bar.x + 300.0, bar.y + 4.0));
        let dropped = Action::Keyframe(KeyframeAction::Select(None));
        assert!(actions.contains(&dropped), "{actions:?}");
        assert_eq!(menu_of(&actions).0, Place::Layer);
        assert_eq!(c.keyframe_pick(), None);
    }

    #[test]
    fn メニューキーは選んでいるひし形の位置でキーフレームのメニューを出す() {
        let m = model();
        let mut c = opened(&m);
        let p = keyframe_center(&c.viewport(), (0, 1), 2.0);
        click(&mut c, &m, p);
        let actions = c.handle(PointerEvent::ContextMenuKey, &m, 7.0);
        let at = KeyframeIndex {
            item: 0,
            row: 1,
            keyframe: 0,
        };
        let menu = Action::ContextMenu {
            hit: Hit::RowKeyframe(at),
            time: None,
            at: p,
        };
        assert_eq!(actions, [menu]);
    }
}
