use super::*;
use crate::gesture::Side;
use crate::rows::{RowHeights, Rows};
use crate::timeline::{CutBar, Item, ItemKind};

fn model() -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![
            Item::layer(0.0, 10.0, ItemKind::Text, true),
            Item::layer(2.0, 4.0, ItemKind::Footage, true),
        ],
    }
}

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

pub(super) fn down(x: f32, y: f32) -> PointerEvent {
    PointerEvent::Down {
        x,
        y,
        mods: Mods::default(),
    }
}

#[test]
fn 目盛りの上のドラッグは再生位置をフレームの境目で追う() {
    let m = model();
    let mut c = ctl(&m);
    let a = c.handle(down(251.0, 5.0), &m, 0.0);
    assert_eq!(a, vec![Action::Seek(quantize(2.51, 30.0))]);
    let b = c.handle(PointerEvent::Move { x: 5000.0, y: 80.0 }, &m, 0.0);
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
    let b = c.handle(PointerEvent::Move { x: 697.0, y }, &m, 7.0);
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
    let moved = c.handle(PointerEvent::Move { x: 555.0, y }, &m, 0.0);
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
    let moved = c.handle(PointerEvent::Move { x: 555.0, y }, &m, 0.0);
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
    c.handle(PointerEvent::Move { x: 555.0, y }, &m, 0.0);
    assert!(c.handle(PointerEvent::Cancel, &m, 0.0).is_empty());
    let st = c.paint_state(0.0);
    assert_eq!((st.preview, st.guide), (None, None));
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
    let moved = c.handle(PointerEvent::Move { x: 777.0, y }, &m, 0.0);
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
    c.handle(PointerEvent::Move { x: 399.0, y }, &m, 0.0);
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
            x: 300.0,
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
            x: 300.0,
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
fn ポインターが乗った所を覚え_出たら消す() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    c.handle(PointerEvent::Move { x: 300.0, y }, &m, 0.0);
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
    let moved = c.handle(PointerEvent::Move { x: 250.0, y }, &m, 0.0);
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
    c.handle(
        PointerEvent::Down {
            x: 298.0,
            y,
            mods: Mods {
                shift: false,
                alt: true,
            },
        },
        &m,
        0.0,
    );
    let moved = c.handle(PointerEvent::Move { x: 270.0, y }, &m, 0.0);
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
    let moved = c.handle(PointerEvent::Move { x: 140.0, y }, &m, 0.0);
    let Action::Edit { kind, by, .. } = moved[0] else {
        panic!("{moved:?}")
    };
    assert_eq!(kind, EditKind::Slip { item: 0, cut: 0 });
    assert!((by + 0.4).abs() < 1e-9, "{by}");
    c.handle(PointerEvent::Up, &m, 0.0);

    c.handle(
        PointerEvent::Down {
            x: 350.0,
            y,
            mods: Mods {
                shift: true,
                alt: false,
            },
        },
        &m,
        0.0,
    );
    let moved = c.handle(PointerEvent::Move { x: 380.0, y }, &m, 0.0);
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
    let moved = c.handle(PointerEvent::Move { x: 697.0, y }, &m, 0.0);
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
