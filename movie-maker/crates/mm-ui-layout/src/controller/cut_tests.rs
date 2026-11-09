//! シーケンスのカットの選択とドラッグ（ripple、roll、slip、slide、スナップ）の試験。

use super::tests::{ctl, down, in_bar, move_to, press};
use super::*;
use crate::gesture::Side;
use crate::timeline::{CutBar, Item, ItemKind};

/// 先頭がシーケンスのレイヤー（カット 3 つ: 0-3、3-4、4-7 秒）、次が普通のレイヤー。
fn seq_model() -> TimelineModel {
    let cut = |start, end| CutBar {
        start,
        end,
        gap: false,
        ..CutBar::default()
    };
    let mut seq = Item::layer(0.0, 20.0, ItemKind::Sequence, true);
    seq.cuts = vec![cut(0.0, 3.0), cut(3.0, 4.0), cut(4.0, 7.0)];
    TimelineModel {
        duration: 20.0,
        fps: 30.0,
        items: vec![seq, Item::layer(10.0, 14.0, ItemKind::Text, true)],
        ..TimelineModel::default()
    }
}

#[test]
fn カットのクリックはレイヤーとカットの両方を選ぶ() {
    let m = seq_model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    let a = c.handle(down(150.0, y), &m, 0.0);
    assert_eq!(
        a,
        vec![Action::Select(Some(0)), Action::SelectCut(Some((0, 0)))]
    );
    assert!(c.handle(PointerEvent::Up, &m, 0.0).is_empty());
    let b = c.handle(down(900.0, y), &m, 0.0);
    assert_eq!(
        b,
        vec![Action::Select(Some(0)), Action::SelectCut(None)],
        "カットの外はレイヤー"
    );
}

#[test]
fn カットの出つまみのドラッグは_ripple_離したときに確定() {
    let m = seq_model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    c.handle(down(298.0, y), &m, 0.0);
    let moved = c.handle(move_to(250.0, y), &m, 0.0);
    assert_eq!(
        moved,
        vec![Action::Edit {
            kind: EditKind::Ripple {
                item: 0,
                cut: 0,
                side: Side::Out
            },
            by: -0.5,
            done: false
        }]
    );
    let done = c.handle(PointerEvent::Up, &m, 0.0);
    assert!(matches!(done[0], Action::Edit { done: true, by, .. } if (by + 0.5).abs() < 1e-9));
}

#[test]
fn alt_を押してつかんだカットの端は_roll() {
    let m = seq_model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    let alt = Mods {
        shift: false,
        alt: true,
    };
    c.handle(press(298.0, y, alt), &m, 0.0);
    let moved = c.handle(move_to(270.0, y), &m, 0.0);
    let Action::Edit { kind, by, .. } = moved[0] else {
        panic!("{moved:?}")
    };
    assert_eq!(kind, EditKind::Roll { item: 0, cut: 0 });
    assert!((by + 0.3).abs() < 1e-9, "{by}");
}

#[test]
fn カットの中のドラッグは_slip_で_shift_なら_slide() {
    let m = seq_model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    c.handle(down(100.0, y), &m, 0.0);
    let moved = c.handle(move_to(140.0, y), &m, 0.0);
    let Action::Edit { kind, by, .. } = moved[0] else {
        panic!("{moved:?}")
    };
    assert_eq!(kind, EditKind::Slip { item: 0, cut: 0 });
    assert!((by + 0.4).abs() < 1e-9, "{by}");
    c.handle(PointerEvent::Up, &m, 0.0);

    let shift = Mods {
        shift: true,
        alt: false,
    };
    c.handle(press(350.0, y, shift), &m, 0.0);
    let moved = c.handle(move_to(380.0, y), &m, 0.0);
    let Action::Edit { kind, by, .. } = moved[0] else {
        panic!("{moved:?}")
    };
    assert_eq!(kind, EditKind::Slide { item: 0, cut: 1 });
    assert!((by - 0.3).abs() < 1e-9, "{by}");
}

#[test]
fn カットの端は他のカットの端に吸い付く() {
    let m = seq_model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    // カット 0 の出つまみを 6.97 秒付近へ。カット 2 の出（7 秒）に吸い付く。
    c.handle(down(298.0, y), &m, 0.0);
    let moved = c.handle(move_to(697.0, y), &m, 0.0);
    let Action::Edit { by, .. } = moved[0] else {
        panic!("{moved:?}")
    };
    assert!((by - 4.0).abs() < 1e-9, "7 秒へ: {by}");
}

#[test]
fn カットからレイヤーへ選び直すとカットの選択が外れる() {
    let m = seq_model();
    let mut c = ctl(&m);
    c.handle(down(150.0, in_bar(&c, 0)), &m, 0.0);
    c.handle(PointerEvent::Up, &m, 0.0);
    let a = c.handle(down(1200.0, in_bar(&c, 1)), &m, 0.0);
    assert_eq!(a, vec![Action::Select(Some(1)), Action::SelectCut(None)]);
}
