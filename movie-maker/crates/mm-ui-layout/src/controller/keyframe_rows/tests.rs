use super::*;
use crate::controller::PointerEvent;
use crate::header::disclosure_center;
use crate::keyframe_rows::keyframe_center;
use crate::keyframe_rows::tests::{model, row};
use crate::rows::{RowHeights, Rows};
use crate::time_view::TimeView;
use crate::timeline::Item;

const SIZE: (f32, f32) = (1000.0, 300.0);

/// `m` を並べた Controller。`open` なら title のキーフレームの行を開く。1 秒 100 px、左端が 0 秒。
fn controller(m: &TimelineModel, open: bool) -> TimelineController {
    let mut c = TimelineController::new(TimeView {
        origin: 0.0,
        px_per_sec: 100.0,
    });
    let mut heights = RowHeights::default();
    if open {
        heights.keyframe_rows_open.insert("title".into());
    }
    c.set_rows(Rows::of(&m.items, &heights));
    c.resize(SIZE);
    c
}

fn press(
    c: &mut TimelineController,
    m: &TimelineModel,
    (x, y): (f32, f32),
    alt: bool,
) -> Vec<Action> {
    let mods = Mods { shift: false, alt };
    c.handle(PointerEvent::Down { x, y, mods }, m, 0.0)
}

/// 行 `row` の時刻 `t` のひし形の中心を押す。
fn press_diamond(
    c: &mut TimelineController,
    m: &TimelineModel,
    (row, t): (usize, f64),
    alt: bool,
) -> Vec<Action> {
    let p = at(c, row, t);
    press(c, m, p, alt)
}

fn index(row: usize, keyframe: usize) -> KeyframeIndex {
    KeyframeIndex {
        item: 0,
        row,
        keyframe,
    }
}

/// 行 `row` の時刻 `t` のひし形の中心。
fn at(c: &TimelineController, row: usize, t: f64) -> (f32, f32) {
    keyframe_center(&c.viewport(), (0, row), t)
}

fn pick(row: usize, keyframe: Option<usize>) -> Action {
    Action::Keyframe(KeyframeAction::Select(Some(RowPick {
        item: 0,
        row,
        keyframe,
    })))
}

#[test]
fn 見出しのキーフレームの開閉を押すと_そのレイヤーの行の開閉になり_選択は変えない() {
    let m = model();
    let mut c = controller(&m, false);
    let p = disclosure_center(&c.viewport(), &m.items[0], 0);
    let toggle = Action::Keyframe(KeyframeAction::ToggleRows(Some(0)));
    assert_eq!(press(&mut c, &m, p, false), [toggle]);
    assert_eq!(c.selected, None);
}

#[test]
fn show_keyframe_editor_は選んだレイヤー_選んでいなければ全部のレイヤーを開閉する() {
    let m = model();
    let mut c = controller(&m, false);
    let show = PointerEvent::Command(TimelineCommand::ShowKeyframeEditor);
    let all = Action::Keyframe(KeyframeAction::ToggleRows(None));
    assert_eq!(c.handle(show, &m, 0.0), [all]);
    c.selected = Some(0);
    let one = Action::Keyframe(KeyframeAction::ToggleRows(Some(0)));
    assert_eq!(c.handle(show, &m, 0.0), [one]);
}

#[test]
fn キーフレームを押すとレイヤーと行とキーフレームを選ぶ() {
    let m = model();
    let mut c = controller(&m, true);
    c.selected_cut = Some((0, 0));
    let actions = press_diamond(&mut c, &m, (0, 4.0), false);
    let cut_off = Action::SelectCut(None);
    assert_eq!(
        actions,
        [Action::Select(Some(0)), cut_off, pick(0, Some(1))]
    );
    let selected = c.paint_state(0.0).keyframe_rows.selected;
    assert_eq!(
        selected.and_then(RowPick::keyframe_index),
        Some(index(0, 1))
    );
    assert!(c.keyframe_pick().is_some());
}

#[test]
fn ドラッグは離したときに_1_回の_move_keyframe_になる() {
    let m = model();
    let mut c = controller(&m, true);
    press_diamond(&mut c, &m, (0, 1.0), false);
    let (x, y) = at(&c, 0, 2.5);
    assert!(c.handle(PointerEvent::Move { x, y }, &m, 0.0).is_empty());
    let st = c.paint_state(0.0);
    assert_eq!(st.keyframe_rows.dragging, Some((index(0, 0), 2.5)));
    assert_eq!(st.drag_tooltip.map(|t| t.by), Some(1.5));
    let moved = Action::Keyframe(KeyframeAction::Move {
        keyframe: index(0, 0),
        to: 2.5,
    });
    assert_eq!(c.handle(PointerEvent::Up, &m, 0.0), [moved]);
    assert_eq!(c.paint_state(0.0).keyframe_rows.dragging, None);
}

#[test]
fn esc_で取りやめると操作にしない() {
    let m = model();
    let mut c = controller(&m, true);
    press_diamond(&mut c, &m, (0, 1.0), false);
    let (x, y) = at(&c, 0, 2.5);
    c.handle(PointerEvent::Move { x, y }, &m, 0.0);
    assert!(c.handle(PointerEvent::Cancel, &m, 0.0).is_empty());
    assert!(c.handle(PointerEvent::Up, &m, 0.0).is_empty());
    assert_eq!(c.paint_state(0.0).keyframe_rows.dragging, None);
    assert!(c.keyframe_pick().is_some(), "選択は残る");
}

#[test]
fn 動かさずに離すとクリックで_操作にしない() {
    let m = model();
    let mut c = controller(&m, true);
    let p = at(&c, 0, 1.0);
    press(&mut c, &m, p, false);
    c.handle(PointerEvent::Move { x: p.0, y: p.1 }, &m, 0.0);
    assert!(c.handle(PointerEvent::Up, &m, 0.0).is_empty());
}

#[test]
fn 再生位置とほかの行のキーフレームに吸い付き_自分の行のキーフレームには吸い付かない() {
    let m = model();
    let mut c = controller(&m, true);
    press_diamond(&mut c, &m, (0, 1.0), false);
    let (x, y) = at(&c, 0, 3.0);
    c.handle(PointerEvent::Move { x: x + 4.0, y }, &m, 3.0);
    let st = c.paint_state(3.0);
    assert_eq!(st.keyframe_rows.dragging, Some((index(0, 0), 3.0)));
    assert_eq!(st.snap.map(|s| s.kind), Some(SnapKind::Playhead));
    let (x, _) = at(&c, 0, 2.0);
    c.handle(PointerEvent::Move { x: x + 4.0, y }, &m, 3.0);
    let snapped = c.paint_state(3.0).snap.map(|s| (s.time, s.kind));
    assert_eq!(
        snapped,
        Some((2.0, SnapKind::Keyframe { item: 0 })),
        "Position の行のキーフレーム"
    );
    let (x, _) = at(&c, 0, 4.0);
    c.handle(PointerEvent::Move { x: x + 4.0, y }, &m, 3.0);
    assert_eq!(
        c.paint_state(3.0).snap,
        None,
        "自分の行の 4.0 には吸い付かない"
    );
}

#[test]
fn 動かせる範囲に留め_同じ行のほかのキーフレームに重ねて離すと操作にしない() {
    let mut m = model();
    m.items[0].keyframe_rows[1].keyframes[0].max = 2.5;
    let mut c = controller(&m, true);
    press_diamond(&mut c, &m, (1, 2.0), false);
    let (x, y) = at(&c, 1, 6.0);
    c.handle(PointerEvent::Move { x, y }, &m, 0.0);
    assert_eq!(
        c.paint_state(0.0).keyframe_rows.dragging,
        Some((index(1, 0), 2.5))
    );
    c.handle(PointerEvent::Up, &m, 0.0);
    press_diamond(&mut c, &m, (0, 1.0), false);
    let (x, y) = at(&c, 0, 4.0);
    c.handle(PointerEvent::Move { x, y }, &m, 0.0);
    assert!(c.handle(PointerEvent::Up, &m, 0.0).is_empty());
}

#[test]
fn remove_keyframe_は選んだキーフレームを外し_行の選択は残す() {
    let m = model();
    let mut c = controller(&m, true);
    let remove = PointerEvent::Command(TimelineCommand::RemoveKeyframe);
    assert!(
        c.handle(remove, &m, 0.0).is_empty(),
        "選んでいなければ何もしない"
    );
    press_diamond(&mut c, &m, (0, 4.0), false);
    c.handle(PointerEvent::Up, &m, 0.0);
    let removed = Action::Keyframe(KeyframeAction::Remove(index(0, 1)));
    assert_eq!(c.handle(remove, &m, 0.0), [removed, pick(0, None)]);
    assert!(c.handle(remove, &m, 0.0).is_empty());
    assert!(c.keyframe_pick().is_some());
}

#[test]
fn alt_クリックはその時刻に打ち_打ったキーフレームを選ぶ() {
    let m = model();
    let mut c = controller(&m, true);
    let (x, y) = at(&c, 1, 3.0);
    let add = Action::Keyframe(KeyframeAction::Add {
        item: 0,
        row: 1,
        time: 3.0,
    });
    let actions = press(&mut c, &m, (x + 1.0, y), true);
    assert_eq!(actions, [Action::Select(Some(0)), pick(1, None), add]);
    let mut after = m.clone();
    after.items[0].keyframe_rows[1] = row("Transform › Position", &[2.0, 3.0]);
    c.model_changed(&after);
    let selected = c.paint_state(0.0).keyframe_rows.selected;
    assert_eq!(
        selected.and_then(RowPick::keyframe_index),
        Some(index(1, 1))
    );
}

#[test]
fn alt_クリックをキーフレームの上で押すと選ぶだけ() {
    let m = model();
    let mut c = controller(&m, true);
    let actions = press_diamond(&mut c, &m, (0, 1.0), true);
    assert_eq!(actions, [Action::Select(Some(0)), pick(0, Some(0))]);
}

#[test]
fn 前後のキーフレームへは選んだレイヤー_選んでいなければ全部のレイヤーから求める() {
    let mut m = model();
    m.items[1] = m.items[1].clone().with_keyframes(&[3.0]);
    let mut c = controller(&m, false);
    let next = PointerEvent::Command(TimelineCommand::NextKeyframe);
    let prev = PointerEvent::Command(TimelineCommand::PreviousKeyframe);
    assert_eq!(c.handle(next, &m, 2.0), [Action::Seek(3.0)], "bg の 3.0");
    assert_eq!(c.handle(prev, &m, 2.0), [Action::Seek(1.0)]);
    c.selected = Some(0);
    assert_eq!(c.handle(next, &m, 2.0), [Action::Seek(4.0)]);
    assert_eq!(
        c.handle(next, &m, 4.01),
        [],
        "半フレームより近い所は今いる所"
    );
    c.selected = Some(1);
    assert_eq!(c.handle(prev, &m, 2.0), []);
}

#[test]
fn 選んだキーフレームは動かした後のモデルでも同じキーフレームを指す() {
    let m = model();
    let mut c = controller(&m, true);
    press_diamond(&mut c, &m, (0, 1.0), false);
    let (x, y) = at(&c, 0, 5.0);
    c.handle(PointerEvent::Move { x, y }, &m, 0.0);
    c.handle(PointerEvent::Up, &m, 0.0);
    let mut after = m.clone();
    after.items[0].keyframe_rows[0] = row("Opacity", &[4.0, 5.0]);
    c.model_changed(&after);
    let selected = c.paint_state(0.0).keyframe_rows.selected;
    assert_eq!(
        selected.and_then(RowPick::keyframe_index),
        Some(index(0, 1))
    );
    c.model_changed(&m);
    let selected = c.paint_state(0.0).keyframe_rows.selected;
    assert_eq!(
        selected,
        Some(RowPick {
            item: 0,
            row: 0,
            keyframe: None
        }),
        "無くなったら行だけ"
    );
}

#[test]
fn 並びが変わってもレイヤーの_id_と行の名前で引き直す() {
    let m = model();
    let mut c = controller(&m, true);
    press_diamond(&mut c, &m, (1, 2.0), false);
    let mut swapped = m.clone();
    swapped.items.reverse();
    c.model_changed(&swapped);
    let selected = c.paint_state(0.0).keyframe_rows.selected;
    assert_eq!(
        selected,
        Some(RowPick {
            item: 1,
            row: 1,
            keyframe: Some(0)
        })
    );
    let mut gone = m.clone();
    gone.items[0] = Item::layer(0.0, 8.0, crate::timeline::ItemKind::Text, true).with_id("title");
    c.model_changed(&gone);
    assert!(c.keyframe_pick().is_none(), "行が無くなれば外す");
}

#[test]
fn レイヤーや空きを押すと選択を外し_目盛りでは外さない() {
    let m = model();
    let mut c = controller(&m, true);
    press_diamond(&mut c, &m, (0, 1.0), false);
    c.handle(PointerEvent::Up, &m, 0.0);
    let ruler = (c.viewport().x_of(5.0), c.layout().ruler.y + 4.0);
    let actions = press(&mut c, &m, ruler, false);
    assert!(matches!(actions[..], [Action::Seek(_)]));
    c.handle(PointerEvent::Up, &m, 0.0);
    assert!(c.keyframe_pick().is_some());
    let bar = c.viewport().bar(1, 0.0, 8.0);
    let actions = press(&mut c, &m, (bar.x + 50.0, bar.y + 4.0), false);
    let off = Action::Keyframe(KeyframeAction::Select(None));
    assert_eq!(actions, [Action::Select(Some(1)), off]);
    assert!(c.keyframe_pick().is_none());
}

#[test]
fn キーフレームの行を閉じると選択を外す() {
    let m = model();
    let mut c = controller(&m, true);
    press_diamond(&mut c, &m, (0, 1.0), false);
    c.handle(PointerEvent::Up, &m, 0.0);
    c.set_rows(Rows::of(&m.items, &RowHeights::default()));
    assert!(c.keyframe_pick().is_none());
}

#[test]
fn 開いたキーフレームの行が見える所までスクロールする() {
    let mut m = model();
    let filler: Vec<Item> = (0..12)
        .map(|i| {
            Item::layer(0.0, 1.0, crate::timeline::ItemKind::Solid, true).with_id(&format!("f{i}"))
        })
        .collect();
    m.items.splice(0..0, filler);
    let mut c = TimelineController::new(TimeView {
        origin: 0.0,
        px_per_sec: 100.0,
    });
    let mut heights = RowHeights::default();
    heights.keyframe_rows_open.insert("title".into());
    c.set_rows(Rows::of(&m.items, &heights));
    c.resize((1000.0, 200.0));
    c.reveal_keyframe_rows(12);
    let vp = c.viewport();
    let below = vp.layout.row_area.y + vp.rows.below(12) - vp.scroll_y;
    assert!(
        (below - vp.layout.row_area.bottom()).abs() < 1e-3,
        "キーフレームの行の下端が行の領域の下端"
    );
}
