//! 目盛りのドラッグ、レイヤーの端のつまみのドラッグ、選択、ホイール、ポインターが乗った所の試験。
//! 試験の x は行の領域の左端を 0 とし、イベントにするときに見出しの列の幅を足す（[`down`]、[`press`]、[`move_to`]）。

use super::*;
use crate::gesture::Side;
use crate::rows::{RowHeights, Rows};
use crate::time_view::MIN_PX_PER_SEC;
use crate::timeline::{Item, ItemKind};
use crate::timeline_layout::{HEADER_W, TOOLBAR_H};

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

/// `m` の行を既定の高さで並べた、幅 2000、高さ 300 の Timeline の操作。1 秒 100 px、左端が 0 秒。
pub(super) fn ctl(m: &TimelineModel) -> TimelineController {
    let mut c = TimelineController::new(TimeView {
        origin: 0.0,
        px_per_sec: 100.0,
    });
    c.set_rows(Rows::of(&m.items, &RowHeights::default()));
    c.resize((2000.0, 300.0));
    c
}

/// 行 `i` の帯の上端から 4 の y。
pub(super) fn in_bar(c: &TimelineController, i: usize) -> f32 {
    c.viewport().bar(i, 0.0, 0.0).y + 4.0
}

/// 行の領域の x の点を押す。
pub(super) fn down(x: f32, y: f32) -> PointerEvent {
    press(x, y, Mods::default())
}

/// 修飾キー `mods` を押しながら、行の領域の x の点を押す。
pub(super) fn press(x: f32, y: f32, mods: Mods) -> PointerEvent {
    PointerEvent::Down {
        x: x + HEADER_W,
        y,
        mods,
    }
}

/// 行の領域の x の点へポインターを動かす。
pub(super) fn move_to(x: f32, y: f32) -> PointerEvent {
    PointerEvent::Move { x: x + HEADER_W, y }
}

#[test]
fn 目盛りの上のドラッグは再生位置をフレームの境目で追う() {
    let m = model();
    let mut c = ctl(&m);
    let a = c.handle(down(251.0, TOOLBAR_H + 5.0), &m, 0.0);
    assert_eq!(a, vec![Action::Seek(quantize(2.51, 30.0))]);
    let b = c.handle(move_to(5000.0, 80.0), &m, 0.0);
    let Action::Seek(t) = b[0] else {
        panic!("{b:?}")
    };
    assert!(t < 10.0 && t > 9.9, "終わりの手前で止まる: {t}");
}

#[test]
fn レイヤーのつまみのドラッグは別のレイヤーの端と再生位置に吸い付き_trim_layer_になる() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    // 先頭の帯の終わり(10 秒)ではなく、2 番目の帯の出つまみを掴む。
    let a = c.handle(down(399.0, y), &m, 0.0);
    assert_eq!(a, vec![Action::Select(Some(1))]);
    let b = c.handle(move_to(697.0, y), &m, 7.0);
    let Action::Edit { kind, by, done } = b[0] else {
        panic!("{b:?}")
    };
    assert_eq!(
        kind,
        EditKind::TrimLayer {
            item: 1,
            side: Side::Out
        }
    );
    assert!((by - 3.0).abs() < 1e-9, "出つまみの 4 秒から 7 秒へ: {by}");
    assert!(!done);
    let done = c.handle(PointerEvent::Up, &m, 7.0);
    assert!(matches!(done[0], Action::Edit { done: true, .. }));
}

#[test]
fn スナップしないドラッグも離したときに確定する() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    c.handle(down(399.0, y), &m, 0.0);
    // どの目標からも遠い所（5.55 秒）で離す。
    let moved = c.handle(move_to(555.0, y), &m, 0.0);
    let Action::Edit { by, .. } = moved[0] else {
        panic!("{moved:?}")
    };
    let done = c.handle(PointerEvent::Up, &m, 0.0);
    assert_eq!(
        done,
        vec![Action::Edit {
            kind: EditKind::TrimLayer {
                item: 1,
                side: Side::Out
            },
            by,
            done: true
        }]
    );
    assert!(
        c.handle(PointerEvent::Up, &m, 0.0).is_empty(),
        "確定は 1 回だけ"
    );
}

#[test]
fn ドラッグ中だけ描く状態にドラッグ中の見た目を載せる() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    c.handle(down(399.0, y), &m, 0.0);
    let moved = c.handle(move_to(555.0, y), &m, 0.0);
    let Action::Edit { kind, by, .. } = moved[0] else {
        panic!("{moved:?}")
    };
    assert_eq!(c.paint_state(0.0).preview, Some((kind, by)));
    c.handle(PointerEvent::Up, &m, 0.0);
    assert_eq!(c.paint_state(0.0).preview, None, "離したら操作の結果を描く");
}

#[test]
fn 取り消したドラッグは確定せず_見た目も戻す() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    c.handle(down(399.0, y), &m, 0.0);
    c.handle(move_to(555.0, y), &m, 0.0);
    assert!(c.handle(PointerEvent::Cancel, &m, 0.0).is_empty());
    let st = c.paint_state(0.0);
    assert_eq!((st.preview, st.snap, st.drag_tooltip), (None, None, None));
    assert!(
        c.handle(PointerEvent::Up, &m, 0.0).is_empty(),
        "取り消した後に離しても操作にしない"
    );
}

#[test]
fn 撮影素材のレイヤーのつまみは素材の終わりより先へ引けない() {
    let mut m = model();
    let mut c = ctl(&m);
    m.items[1].trim.max_end = 5.0;
    let y = in_bar(&c, 1);
    c.handle(down(399.0, y), &m, 0.0);
    let moved = c.handle(move_to(777.0, y), &m, 0.0);
    let Action::Edit { by, .. } = moved[0] else {
        panic!("{moved:?}")
    };
    assert!(
        (by - 1.0).abs() < 1e-9,
        "アウト点のつまみの 4 秒から素材の終わりの 5 秒まで: {by}"
    );
}

#[test]
fn 動かさずに離したクリックは編集にしない() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    c.handle(down(399.0, y), &m, 0.0);
    c.handle(move_to(399.0, y), &m, 0.0);
    assert!(c.handle(PointerEvent::Up, &m, 0.0).is_empty());
}

#[test]
fn モデルの並びが変わったら範囲の外の選択とドラッグを捨てる() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    c.handle(down(399.0, y), &m, 0.0);
    assert_eq!(c.selected, Some(1));
    c.model_changed(&TimelineModel {
        items: vec![Item::layer(0.0, 10.0, ItemKind::Text, true)],
        ..m.clone()
    });
    assert_eq!(c.selected, None);
    assert!(
        c.handle(PointerEvent::Up, &m, 0.0).is_empty(),
        "古いドラッグは確定しない"
    );
}

#[test]
fn 本体のクリックは選ぶだけ_空きで選択を外す() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    assert_eq!(
        c.handle(down(300.0, y), &m, 0.0),
        vec![Action::Select(Some(1))]
    );
    assert!(c.handle(PointerEvent::Up, &m, 0.0).is_empty());
    assert_eq!(
        c.handle(down(900.0, y), &m, 0.0),
        vec![Action::Select(None)]
    );
}

#[test]
fn ホイールは_ctrl_で拡大_なしで移動() {
    let m = model();
    let mut c = ctl(&m);
    let t = c.view.t_of(300.0);
    c.handle(
        PointerEvent::Wheel {
            x: 300.0 + HEADER_W,
            y: 0.0,
            dy: 2.0,
            ctrl: true,
            shift: false,
        },
        &m,
        0.0,
    );
    assert!(c.view.px_per_sec > 100.0);
    assert!((c.view.t_of(300.0) - t).abs() < 1e-6);
    let origin = c.view.origin;
    c.handle(
        PointerEvent::Wheel {
            x: 300.0 + HEADER_W,
            y: 0.0,
            dy: 1.0,
            ctrl: false,
            shift: false,
        },
        &m,
        0.0,
    );
    assert!(c.view.origin > origin, "奥へ回すと先の時刻が見える");
}

#[test]
fn ctrl_ホイールの縮小は長いコンポジションでも全体が収まるところまで() {
    let zoom_out = PointerEvent::Wheel {
        x: 300.0 + HEADER_W,
        y: 0.0,
        dy: -100.0,
        ctrl: true,
        shift: false,
    };
    let mut long = model();
    long.duration = 3600.0;
    let mut c = ctl(&long);
    c.handle(zoom_out, &long, 0.0);
    let fit = TimeView::fit(3600.0, c.layout().row_area.w, VIEW_MARGIN).px_per_sec;
    assert!(
        (c.view.px_per_sec - fit).abs() < 1e-9,
        "{}",
        c.view.px_per_sec
    );
    let short = model();
    let mut c = ctl(&short);
    c.handle(zoom_out, &short, 0.0);
    assert_eq!(
        c.view.px_per_sec, MIN_PX_PER_SEC,
        "短ければ MIN_PX_PER_SEC まで"
    );
}

#[test]
fn ポインターが乗った所を覚え_出たら消す() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    c.handle(move_to(300.0, y), &m, 0.0);
    assert_eq!(
        c.hover,
        Hit::Layer {
            index: 1,
            part: Part::Body
        }
    );
    c.handle(PointerEvent::Leave, &m, 0.0);
    assert_eq!(c.hover, Hit::Empty);
}
