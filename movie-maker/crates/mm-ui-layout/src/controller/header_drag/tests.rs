//! 見出しの縦のドラッグ（重なりの順を変える）の試験。

use super::super::tests::ctl;
use super::*;
use crate::controller::{Mods, PointerEvent};
use crate::timeline::{Item, ItemKind, TimelineModel};

/// 見出しのボタンの外（名前の上）の x。
const NAME_X: f32 = 40.0;

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
fn 縦に動かして離すと_行の境目を指す_move_layer_になる() {
    let m = model(4);
    let mut c = ctl(&m);
    // 行 0 を行 2 の下半分へ。境目 3（行 2 と行 3 の間）に入る。
    let acts = drag_rows(&mut c, &m, 0, (2, 8.0));
    assert_eq!(acts, vec![Action::MoveLayer { from: 0, slot: 3 }]);
    // 行 3 を先頭の行の上半分へ。境目 0。
    let acts = drag_rows(&mut c, &m, 3, (0, -8.0));
    assert_eq!(acts, vec![Action::MoveLayer { from: 3, slot: 0 }]);
}

#[test]
fn 最後の行より下へ離すと_末尾の境目() {
    let m = model(3);
    let mut c = ctl(&m);
    let from = y_of(&c, (0, 0.0));
    let below = c.viewport().row(2).bottom() + 20.0;
    let acts = drag(&mut c, &m, from, below);
    assert_eq!(acts, vec![Action::MoveLayer { from: 0, slot: 3 }]);
}

#[test]
fn 自分の上か下の境目に離しても並びは変わらないので操作にしない() {
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
fn ドラッグ中は入る位置を出し_取り消すと線も操作も捨てる() {
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
