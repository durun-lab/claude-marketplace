//! 見出しの縦のドラッグ（重なりの順を変える）の試験。

use super::super::tests::ctl;
use super::*;
use crate::controller::{Mods, PointerEvent};
use crate::rows::RowHeight;
use crate::timeline::{Item, ItemKind, TimelineModel};

/// 見出しのボタンの外（名前の上）の x。
const NAME_X: f32 = 40.0;
/// 画像のレイヤーの行の高さ。
const ROW_H: f32 = RowHeight::Medium.px();

fn model(n: usize) -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: (0..n)
            .map(|_| Item::layer(0.0, 10.0, ItemKind::Image, true))
            .collect(),
        ..TimelineModel::default()
    }
}

fn down(y: f32) -> PointerEvent {
    PointerEvent::Down {
        x: NAME_X,
        y,
        mods: Mods::default(),
    }
}

fn move_to(y: f32) -> PointerEvent {
    PointerEvent::Move { x: NAME_X, y }
}

/// 行 `row` の縦の中央から `dy` ずれた y。
fn y_of(c: &TimelineController, (row, dy): (usize, f32)) -> f32 {
    let r = c.viewport().row(row);
    r.y + r.h / 2.0 + dy
}

fn drag(c: &mut TimelineController, m: &TimelineModel, from: f32, to: f32) -> Vec<Action> {
    c.handle(down(from), m, 0.0);
    c.handle(move_to(to), m, 0.0);
    c.handle(PointerEvent::Up, m, 0.0)
}

/// 行 `from` の中央から行 `to` の中央（から `dy`）へのドラッグを離す。
fn drag_rows(
    c: &mut TimelineController,
    m: &TimelineModel,
    from: usize,
    to: (usize, f32),
) -> Vec<Action> {
    let (a, b) = (y_of(c, (from, 0.0)), y_of(c, to));
    drag(c, m, a, b)
}

#[test]
fn 縦に動かして離すと_行の境界を指す_move_layer_になる() {
    let m = model(4);
    let mut c = ctl(&m);
    // 行 0 を行 2 の下半分へ。境界 3（行 2 と行 3 の間）に入る。
    let acts = drag_rows(&mut c, &m, 0, (2, 8.0));
    assert_eq!(
        acts,
        vec![Action::MoveLayer {
            from: 0,
            boundary: 3
        }]
    );
    // 行 3 を先頭の行の上半分へ。境界 0。
    let acts = drag_rows(&mut c, &m, 3, (0, -8.0));
    assert_eq!(
        acts,
        vec![Action::MoveLayer {
            from: 3,
            boundary: 0
        }]
    );
}

#[test]
fn 最後の行より下へ離すと_末尾の境界() {
    let m = model(3);
    let mut c = ctl(&m);
    let from = y_of(&c, (0, 0.0));
    let below = c.viewport().row(2).bottom() + 20.0;
    let acts = drag(&mut c, &m, from, below);
    assert_eq!(
        acts,
        vec![Action::MoveLayer {
            from: 0,
            boundary: 3
        }]
    );
}

#[test]
fn 自分の上か下の境界に離しても並びは変わらないので操作にしない() {
    let m = model(3);
    let mut c = ctl(&m);
    assert!(drag_rows(&mut c, &m, 1, (1, 10.0)).is_empty());
    assert!(drag_rows(&mut c, &m, 1, (1, -10.0)).is_empty());
}

#[test]
fn 少ししか動かさないクリックは選ぶだけで_操作にしない() {
    let m = model(3);
    let mut c = ctl(&m);
    let y = y_of(&c, (2, 0.0));
    let acts = c.handle(down(y), &m, 0.0);
    assert_eq!(acts, vec![Action::Select(Some(2))]);
    c.handle(move_to(y + 2.0), &m, 0.0);
    assert_eq!(c.paint_state(0.0).header_drop, None, "線は出さない");
    assert!(c.handle(PointerEvent::Up, &m, 0.0).is_empty());
}

#[test]
fn ドラッグ中は入る位置を出し_取りやめると線も操作も捨てる() {
    let m = model(4);
    let mut c = ctl(&m);
    let (from, to) = (y_of(&c, (0, 0.0)), y_of(&c, (2, -8.0)));
    c.handle(down(from), &m, 0.0);
    c.handle(move_to(to), &m, 0.0);
    assert_eq!(c.paint_state(0.0).header_drop, Some(2));
    assert!(c.handle(PointerEvent::Cancel, &m, 0.0).is_empty());
    assert_eq!(c.paint_state(0.0).header_drop, None);
    let up = c.handle(PointerEvent::Up, &m, 0.0);
    assert!(up.is_empty(), "離しても確定しない");
}

#[test]
fn 縦にスクロールした後も_ポインターの下の行の境界に入る() {
    let m = model(30);
    let mut c = ctl(&m);
    c.scroll_to(10.0 * ROW_H);
    // 行 12 を行 14 の下半分へ。境界 15（行の添字はスクロールで変わらない）。
    let acts = drag_rows(&mut c, &m, 12, (14, 8.0));
    assert_eq!(
        acts,
        vec![Action::MoveLayer {
            from: 12,
            boundary: 15
        }]
    );
}

#[test]
fn 行の領域の上や下へ出すと_見えている端の境界に寄せる() {
    let m = model(30);
    let mut c = ctl(&m);
    c.scroll_to(10.0 * ROW_H);
    let from = y_of(&c, (12, 0.0));
    let area = c.layout().headers;
    // 目盛りの帯まで上げても、見えていない行 0 から 9 の間には入れず、見えている先頭の行 10 の上の境界。
    let acts = drag(&mut c, &m, from, area.y - 20.0);
    assert_eq!(
        acts,
        vec![Action::MoveLayer {
            from: 12,
            boundary: 10
        }]
    );
    let acts = drag(&mut c, &m, from, area.bottom() + 40.0);
    let [Action::MoveLayer { boundary, .. }] = acts[..] else {
        panic!("{acts:?}");
    };
    let line = c.viewport().row(boundary).y;
    assert!(
        boundary > 13 && line <= area.bottom(),
        "線の見える境界: {boundary} {line}"
    );
}

#[test]
fn ドラッグしたままホイールで縦にスクロールすると_入る位置を置き直す() {
    let m = model(30);
    let mut c = ctl(&m);
    let (from, to) = (y_of(&c, (0, 0.0)), y_of(&c, (3, 8.0)));
    c.handle(down(from), &m, 0.0);
    c.handle(move_to(to), &m, 0.0);
    assert_eq!(c.paint_state(0.0).header_drop, Some(4));
    // 見出しの上のホイールで 120 px 下へ。同じポインターの位置は行 8 の上半分。
    let wheel = PointerEvent::Wheel {
        x: NAME_X,
        y: to,
        dy: -2.0,
        ctrl: false,
        shift: false,
    };
    c.handle(wheel, &m, 0.0);
    assert_eq!(c.paint_state(0.0).header_drop, Some(8));
    let up = c.handle(PointerEvent::Up, &m, 0.0);
    assert_eq!(
        up,
        vec![Action::MoveLayer {
            from: 0,
            boundary: 8
        }]
    );
}

#[test]
fn 高い_sequence_の行は_その行の縦の中央で上と下の境界が分かれる() {
    let mut m = model(3);
    m.items[1] = Item::layer(0.0, 10.0, ItemKind::Sequence, true);
    let mut c = ctl(&m);
    let large = c.viewport().row(1);
    assert!(large.h > 2.0 * ROW_H - 1.0, "Large の行");
    let middle = large.y + large.h / 2.0;
    let (top, bottom) = (y_of(&c, (0, 0.0)), y_of(&c, (2, 0.0)));
    let acts = drag(&mut c, &m, top, middle + 4.0);
    assert_eq!(
        acts,
        vec![Action::MoveLayer {
            from: 0,
            boundary: 2
        }]
    );
    let acts = drag(&mut c, &m, bottom, middle - 4.0);
    assert_eq!(
        acts,
        vec![Action::MoveLayer {
            from: 2,
            boundary: 1
        }]
    );
}
