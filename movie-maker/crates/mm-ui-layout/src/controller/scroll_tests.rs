//! 縦のスクロール（Shift+ホイール、スクロールバー、行を見せる位置）の試験。

use super::tests::{ctl, down, in_bar};
use super::*;
use crate::rows::{RowHeights, Rows};
use crate::timeline::{Item, ItemKind};
use crate::timeline_layout::{HEADER_W, TOOLBAR_H};

/// 画像のレイヤーを `n` 本（Medium の行が `n` 本）。
fn many(n: usize) -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: (0..n)
            .map(|_| Item::layer(0.0, 10.0, ItemKind::Image, true))
            .collect(),
        ..TimelineModel::default()
    }
}

fn wheel(x: f32, y: f32, dy: f32, shift: bool) -> PointerEvent {
    PointerEvent::Wheel {
        x,
        y,
        dy,
        ctrl: false,
        shift,
    }
}

#[test]
fn shift_ホイールは縦にスクロールし_範囲に収め_横は動かさない() {
    let m = many(100);
    let mut c = ctl(&m);
    let start = c.view;
    c.handle(wheel(300.0, 100.0, -2.0, true), &m, 0.0);
    assert_eq!(
        c.scroll_y,
        2.0 * SCROLL_STEP_PX,
        "手前へ回すと下の行が見える"
    );
    assert_eq!(c.view, start);
    c.handle(wheel(300.0, 100.0, 5.0, true), &m, 0.0);
    assert_eq!(c.scroll_y, 0.0, "上端で止まる");
    c.handle(wheel(300.0, 100.0, -1000.0, true), &m, 0.0);
    let rows_h = c.layout().row_area.h;
    assert_eq!(c.scroll_y, 100.0 * 28.0 - rows_h, "下端で止まる");
}

#[test]
fn スクロールした後は見えている行に当たる() {
    let m = many(100);
    let mut c = ctl(&m);
    c.handle(wheel(300.0, 100.0 + TOOLBAR_H, -3.0, true), &m, 0.0);
    // 180 px 送ると、y 100 は行の並びの 254 px で、行 9（252 から 280）の帯の上端。
    assert_eq!(
        c.hover,
        Hit::Layer {
            index: 9,
            part: Part::Body
        },
        "ポインターの下に来た行に乗った所を合わせ直す"
    );
    let row = (3.0 * SCROLL_STEP_PX / 28.0).ceil() as usize;
    let a = c.handle(down(300.0, in_bar(&c, row)), &m, 0.0);
    assert_eq!(a, vec![Action::Select(Some(row))]);
}

#[test]
fn スクロールボックスを引くとスクロールし_溝を押すと_1_画面ぶん送る() {
    let m = many(100);
    let mut c = ctl(&m);
    let track = c.layout().scrollbar;
    assert!(track.w > 0.0, "中身が入らないので出る");
    let thumb = c.viewport().thumb();
    let x = track.x + track.w / 2.0;
    assert!(
        c.handle(down(x - HEADER_W, thumb.y + 5.0), &m, 0.0)
            .is_empty(),
        "選択は変えない"
    );
    c.handle(
        PointerEvent::Move {
            x,
            y: thumb.y + 105.0,
        },
        &m,
        0.0,
    );
    let want = scroll_for_thumb(track, 2800.0, thumb.y + 100.0);
    assert!((c.scroll_y - want).abs() < 1e-3, "{} {want}", c.scroll_y);
    assert_eq!(c.paint_state(0.0).hover, Hit::Scrollbar { thumb: true });
    c.handle(PointerEvent::Up, &m, 0.0);
    let before = c.scroll_y;
    let below = c.viewport().thumb().bottom() + 20.0;
    c.handle(down(x - HEADER_W, below), &m, 0.0);
    assert_eq!(
        c.scroll_y,
        before + c.layout().row_area.h,
        "下の溝で 1 画面ぶん下へ"
    );
}

#[test]
fn スクロールバーの上のホイールは縦にスクロールする() {
    let m = many(100);
    let mut c = ctl(&m);
    let track = c.layout().scrollbar;
    c.handle(wheel(track.x + 2.0, 100.0, -1.0, false), &m, 0.0);
    assert_eq!(c.scroll_y, SCROLL_STEP_PX);
}

#[test]
fn 行を見せると全部見える所までだけスクロールし_大きさが変わってもスクロールを範囲に収める() {
    let m = many(100);
    let mut c = ctl(&m);
    c.reveal(50);
    let rows_h = c.layout().row_area.h;
    assert_eq!(c.scroll_y, 51.0 * 28.0 - rows_h, "下端に合わせる");
    c.reveal(50);
    assert_eq!(c.scroll_y, 51.0 * 28.0 - rows_h, "見えていれば動かない");
    c.set_rows(Rows::of(&many(3).items, &RowHeights::default()));
    assert_eq!(c.scroll_y, 0.0, "中身が入れば 0");
}
