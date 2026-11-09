//! スナップ（入/切、ドラッグ中の反転、吸い付いた目標、キーフレームとマーカー）とドラッグ中のツールチップの試験。

use super::tests::{ctl, down, in_bar};
use super::*;
use crate::drag_tip::DragTip;
use crate::gesture::Side;
use crate::snap::SnapKind;
use crate::timeline::{CutBar, Item, ItemKind};

/// 文字のレイヤー（0-10 秒、キーフレームは 6 秒）と撮影素材のレイヤー（2-4 秒）。マーカーは 8 秒。
fn model() -> TimelineModel {
    let mut text = Item::layer(0.0, 10.0, ItemKind::Text, true);
    text.keyframes = vec![6.0];
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![text, Item::layer(2.0, 4.0, ItemKind::Footage, true)],
        markers: vec![8.0],
    }
}

/// 撮影素材のレイヤーの出つまみ（4 秒）を掴み、`x` まで引く。再生位置は 7 秒。
fn drag_out(c: &mut TimelineController, m: &TimelineModel, x: f32) -> Vec<Action> {
    let y = in_bar(c, 1);
    c.handle(down(399.0, y), m, PLAYHEAD);
    c.handle(PointerEvent::Move { x, y }, m, PLAYHEAD)
}

const PLAYHEAD: f64 = 7.0;

fn by_of(actions: &[Action]) -> f64 {
    match actions {
        [Action::Edit {
            by, done: false, ..
        }] => *by,
        other => panic!("{other:?}"),
    }
}

fn snapped_kind(c: &TimelineController) -> Option<SnapKind> {
    c.paint_state(PLAYHEAD).snap.map(|s| s.kind)
}

#[test]
fn 吸い付いたときだけ描く状態に目標を載せる() {
    let m = model();
    let mut c = ctl(&m);
    let by = by_of(&drag_out(&mut c, &m, 697.0));
    assert!((by - 3.0).abs() < 1e-9, "再生位置の 7 秒へ: {by}");
    assert_eq!(snapped_kind(&c), Some(SnapKind::Playhead));
    let y = in_bar(&c, 1);
    c.handle(PointerEvent::Move { x: 555.0, y }, &m, PLAYHEAD);
    assert_eq!(
        snapped_kind(&c),
        None,
        "どの目標からも遠ければガイド線を出さない"
    );
}

#[test]
fn スナップを切るとつまみはフレームの境目に丸まり_吸い付かない() {
    let m = model();
    let mut c = ctl(&m);
    c.snapping = false;
    let by = by_of(&drag_out(&mut c, &m, 697.0));
    assert!((by - (quantize(6.97, 30.0) - 4.0)).abs() < 1e-9, "{by}");
    assert_eq!(snapped_kind(&c), None);
}

#[test]
fn ドラッグしていなければ入_切のコマンドで入_切を切り替える() {
    let m = model();
    let mut c = ctl(&m);
    assert!(c.snapping, "既定は入");
    assert!(c
        .handle(PointerEvent::ToggleSnapping, &m, PLAYHEAD)
        .is_empty());
    assert!(!c.snapping);
    c.handle(PointerEvent::ToggleSnapping, &m, PLAYHEAD);
    assert!(c.snapping);
    assert!(c.handle(PointerEvent::SnapKeyUp, &m, PLAYHEAD).is_empty());
    assert!(c.snapping, "離しただけでは変えない");
}

#[test]
fn ドラッグ中の入_切のキーは押している間だけ反転し_入_切そのものは変えない() {
    let m = model();
    let mut c = ctl(&m);
    drag_out(&mut c, &m, 697.0);
    let held = by_of(&c.handle(PointerEvent::ToggleSnapping, &m, PLAYHEAD));
    assert!((held - (quantize(6.97, 30.0) - 4.0)).abs() < 1e-9, "{held}");
    assert_eq!(snapped_kind(&c), None, "押している間は吸い付かない");
    assert!(c.snapping, "入/切は変えない");
    let released = by_of(&c.handle(PointerEvent::SnapKeyUp, &m, PLAYHEAD));
    assert!((released - 3.0).abs() < 1e-9, "{released}");
    assert_eq!(snapped_kind(&c), Some(SnapKind::Playhead));
}

#[test]
fn スナップを切っていれば押している間だけ吸い付く() {
    let m = model();
    let mut c = ctl(&m);
    c.snapping = false;
    drag_out(&mut c, &m, 697.0);
    c.handle(PointerEvent::ToggleSnapping, &m, PLAYHEAD);
    assert_eq!(snapped_kind(&c), Some(SnapKind::Playhead));
    let done = c.handle(PointerEvent::Up, &m, PLAYHEAD);
    assert!(matches!(done[..], [Action::Edit { done: true, by, .. }] if (by - 3.0).abs() < 1e-9));
    assert!(!c.snapping);
    assert!(
        c.handle(PointerEvent::SnapKeyUp, &m, PLAYHEAD).is_empty(),
        "離したらドラッグと一緒に反転も終わる"
    );
    drag_out(&mut c, &m, 697.0);
    assert_eq!(snapped_kind(&c), None, "次のドラッグは切のまま");
}

#[test]
fn 押しただけで動かしていなければ入_切のキーで編集にしない() {
    let m = model();
    let mut c = ctl(&m);
    let y = in_bar(&c, 1);
    c.handle(down(396.0, y), &m, PLAYHEAD);
    assert!(c
        .handle(PointerEvent::ToggleSnapping, &m, PLAYHEAD)
        .is_empty());
    assert!(c.handle(PointerEvent::Up, &m, PLAYHEAD).is_empty());
    assert!(c.snapping);
}

#[test]
fn キーフレームとマーカーに吸い付く() {
    let m = model();
    let mut c = ctl(&m);
    let by = by_of(&drag_out(&mut c, &m, 603.0));
    assert!((by - 2.0).abs() < 1e-9, "キーフレームの 6 秒へ: {by}");
    assert_eq!(snapped_kind(&c), Some(SnapKind::Keyframe { item: 0 }));
    let y = in_bar(&c, 1);
    let at_marker = c.handle(PointerEvent::Move { x: 795.0, y }, &m, PLAYHEAD);
    assert!((by_of(&at_marker) - 4.0).abs() < 1e-9, "マーカーの 8 秒へ");
    assert_eq!(snapped_kind(&c), Some(SnapKind::Marker));
}

#[test]
fn 端が目標に届かなければ吸い付いたことにしない() {
    let mut m = model();
    m.items[1].trim.max_end = 5.0;
    let mut c = ctl(&m);
    let by = by_of(&drag_out(&mut c, &m, 697.0));
    assert!((by - 1.0).abs() < 1e-9, "素材の終わりで止まる: {by}");
    assert_eq!(snapped_kind(&c), None);
}

#[test]
fn カットの端はほかのカットの端を目標にする() {
    let cut = |start, end| CutBar {
        start,
        end,
        ..CutBar::default()
    };
    let mut seq = Item::layer(0.0, 20.0, ItemKind::Sequence, true);
    seq.cuts = vec![cut(0.0, 3.0), cut(3.0, 4.0), cut(4.0, 7.0)];
    let m = TimelineModel {
        duration: 20.0,
        fps: 30.0,
        items: vec![seq],
        ..TimelineModel::default()
    };
    let mut c = ctl(&m);
    let y = in_bar(&c, 0);
    c.handle(down(298.0, y), &m, 0.0);
    c.handle(PointerEvent::Move { x: 697.0, y }, &m, 0.0);
    let clip = SnapKind::ClipEdge {
        item: 0,
        cut: 2,
        side: Side::Out,
    };
    assert_eq!(c.paint_state(0.0).snap.map(|s| s.kind), Some(clip));
}

#[test]
fn ドラッグ中のツールチップはポインターの位置とポインターの向きの量を持つ() {
    let m = model();
    let mut c = ctl(&m);
    assert_eq!(c.paint_state(PLAYHEAD).drag, None);
    let by = by_of(&drag_out(&mut c, &m, 697.0));
    let y = in_bar(&c, 1);
    let tip = DragTip { at: (697.0, y), by };
    assert_eq!(c.paint_state(PLAYHEAD).drag, Some(tip));
    c.handle(PointerEvent::Up, &m, PLAYHEAD);
    assert_eq!(c.paint_state(PLAYHEAD).drag, None, "離したら消す");

    // slip は操作の量（素材のイン点）がポインターと逆向きなので、ツールチップは中身が動いた向きにする。
    let cut = CutBar {
        start: 0.0,
        end: 3.0,
        ..CutBar::default()
    };
    let mut seq = Item::layer(0.0, 10.0, ItemKind::Sequence, true);
    seq.cuts = vec![cut];
    let s = TimelineModel {
        items: vec![seq],
        ..model()
    };
    let mut c = ctl(&s);
    let y = in_bar(&c, 0);
    c.handle(down(100.0, y), &s, 0.0);
    let slip = by_of(&c.handle(PointerEvent::Move { x: 140.0, y }, &s, 0.0));
    assert!(slip < 0.0, "{slip}");
    let shown = c.paint_state(0.0).drag.map(|d| d.by);
    assert_eq!(shown, Some(-slip));
}
