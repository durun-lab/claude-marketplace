//! ツール（Selection、Trim、Blade）の試験。帯の中のドラッグとクリックがツールで変わること、レイヤーを動かすときの 0 での止まり方と
//! 吸い付き、Blade の割る時刻とホバーの線、ツールを選ぶコマンド。試験の x は行の領域の左端を 0 とする（1 秒 100 px）。

use super::tests::{ctl, down, in_bar, move_to};
use super::*;
use crate::snap::SnapKind;
use crate::timeline::{CutBar, Item, ItemKind};

const PLAYHEAD: f64 = 7.0;

/// 先頭がシーケンスのレイヤー（カット 0-3、3-4 秒の隙間、4-7 秒）、次が撮影素材のレイヤー（8-9.5 秒）、
/// 最後が文字のレイヤー（0-5 秒。キーフレームは 1 秒）。
fn model() -> TimelineModel {
    let cut = |start, end, gap| CutBar {
        start,
        end,
        gap,
        ..CutBar::default()
    };
    let mut seq = Item::layer(0.0, 20.0, ItemKind::Sequence, true);
    seq.cuts = vec![
        cut(0.0, 3.0, false),
        cut(3.0, 4.0, true),
        cut(4.0, 7.0, false),
    ];
    let mut text = Item::layer(0.0, 5.0, ItemKind::Text, true);
    text.keyframes = [1.0].into();
    TimelineModel {
        duration: 20.0,
        fps: 30.0,
        items: vec![seq, Item::layer(8.0, 9.5, ItemKind::Footage, true), text],
        ..TimelineModel::default()
    }
}

fn run(c: &mut TimelineController, m: &TimelineModel, ev: PointerEvent) -> Vec<Action> {
    c.handle(ev, m, PLAYHEAD)
}

/// 行 `row` の帯の x を押す。
fn press_row(c: &mut TimelineController, m: &TimelineModel, x: f32, row: usize) -> Vec<Action> {
    let y = in_bar(c, row);
    run(c, m, down(x, y))
}

/// 行 `row` の帯の上で x へポインターを動かす。
fn move_row(c: &mut TimelineController, m: &TimelineModel, x: f32, row: usize) -> Vec<Action> {
    let y = in_bar(c, row);
    run(c, m, move_to(x, y))
}

fn command(c: &mut TimelineController, m: &TimelineModel, command: TimelineCommand) {
    run(c, m, PointerEvent::Command(command));
}

fn edit_of(actions: &[Action]) -> (EditKind, f64, bool) {
    match actions {
        [Action::Edit { kind, by, done }] => (*kind, *by, *done),
        other => panic!("{other:?}"),
    }
}

fn snap_kind(c: &TimelineController) -> Option<SnapKind> {
    c.paint_state(PLAYHEAD).snap.map(|s| s.kind)
}

#[test]
fn 始めのツールは_selection() {
    assert_eq!(ctl(&model()).tool, Tool::Selection);
}

#[test]
fn ツールを選ぶコマンドは今のツールを変える() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::BladeTool);
    assert_eq!(c.tool, Tool::Blade);
    command(&mut c, &m, TimelineCommand::TrimTool);
    assert_eq!(c.tool, Tool::Trim);
    command(&mut c, &m, TimelineCommand::SelectionTool);
    assert_eq!(c.tool, Tool::Selection);
}

#[test]
fn ドラッグの途中はツールを変えない() {
    let m = model();
    let mut c = ctl(&m);
    press_row(&mut c, &m, 900.0, 1);
    command(&mut c, &m, TimelineCommand::BladeTool);
    assert_eq!(c.tool, Tool::Selection);
}

#[test]
fn selection_のレイヤーの中のドラッグはレイヤーを動かし_離したときに確定() {
    let m = model();
    let mut c = ctl(&m);
    press_row(&mut c, &m, 900.0, 1);
    let (kind, by, done) = edit_of(&move_row(&mut c, &m, 950.0, 1));
    assert_eq!(kind, EditKind::ShiftLayer { item: 1 });
    assert!((by - 0.5).abs() < 1e-9 && !done, "{by}");
    let shown = c.paint_state(PLAYHEAD).drag_tooltip.map(|d| d.by);
    assert_eq!(shown, Some(by), "ツールチップは動かした量");
    let (_, by, done) = edit_of(&run(&mut c, &m, PointerEvent::Up));
    assert!((by - 0.5).abs() < 1e-9 && done);
}

#[test]
fn 動かさずに離したレイヤーのクリックは編集にしない() {
    let m = model();
    let mut c = ctl(&m);
    let a = press_row(&mut c, &m, 900.0, 1);
    assert_eq!(a, vec![Action::Select(Some(1))]);
    assert!(run(&mut c, &m, PointerEvent::Up).is_empty());
}

#[test]
fn レイヤーを動かしている途中の_esc_は編集を確定しない() {
    let m = model();
    let mut c = ctl(&m);
    press_row(&mut c, &m, 900.0, 1);
    move_row(&mut c, &m, 1000.0, 1);
    run(&mut c, &m, PointerEvent::Cancel);
    assert!(run(&mut c, &m, PointerEvent::Up).is_empty());
    assert_eq!(c.paint_state(PLAYHEAD).preview, None);
}

#[test]
fn レイヤーの帯の左端は_0_で止まる() {
    let m = model();
    let mut c = ctl(&m);
    press_row(&mut c, &m, 900.0, 1);
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 100.0, 1));
    assert!((by + 8.0).abs() < 1e-9, "8 秒の始まりが 0 まで: {by}");
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 0.0, 1));
    assert!((by + 8.0).abs() < 1e-9, "0 より前へ出さない: {by}");
}

#[test]
fn 始まりが_0_より前のレイヤーは今の位置より前へ動かさない() {
    let mut m = model();
    m.items[1] = Item::layer(-1.0, 3.0, ItemKind::Footage, true);
    let mut c = ctl(&m);
    press_row(&mut c, &m, 100.0, 1);
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 0.0, 1));
    assert_eq!(by, 0.0);
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 150.0, 1));
    assert!((by - 0.5).abs() < 1e-9, "後ろへは動かせる: {by}");
}

#[test]
fn 始まりがちょうど_0_のレイヤーは前へ動かさない() {
    let mut m = model();
    m.items[1] = Item::layer(0.0, 3.0, ItemKind::Footage, true);
    let mut c = ctl(&m);
    press_row(&mut c, &m, 100.0, 1);
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 50.0, 1));
    assert_eq!(by, 0.0);
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 150.0, 1));
    assert!((by - 0.5).abs() < 1e-9, "後ろへは動かせる: {by}");
}

#[test]
fn 吸い付く先が始まりを_0_より前に出すときは_0_で止め_吸い付いた表示は出さない() {
    // 1-3 秒のレイヤーの終わりを、文字のレイヤーの終わり（1.5 秒）へ寄せると始まりが -0.5 秒になる。
    let mut m = model();
    m.items[0].cuts.clear();
    m.items[1] = Item::layer(1.0, 3.0, ItemKind::Footage, true);
    m.items[2] = Item::layer(0.0, 1.5, ItemKind::Text, true);
    let mut c = ctl(&m);
    press_row(&mut c, &m, 200.0, 1);
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 52.0, 1));
    assert!((by + 1.0).abs() < 1e-9, "始まりの 1 秒まで: {by}");
    assert_eq!(snap_kind(&c), None);
    let shown = c.paint_state(PLAYHEAD).drag_tooltip.map(|d| d.by);
    assert_eq!(shown, Some(by), "ツールチップは止めた後の量");
}

#[test]
fn レイヤーを動かすと始まりが再生位置に吸い付き_ガイドを出す() {
    let m = model();
    let mut c = ctl(&m);
    press_row(&mut c, &m, 900.0, 1);
    // 始まり（8 秒）を 7.03 秒へ。再生位置の 7 秒に吸い付く。
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 803.0, 1));
    assert!((by + 1.0).abs() < 1e-9, "{by}");
    assert_eq!(snap_kind(&c), Some(SnapKind::Playhead));
}

#[test]
fn レイヤーを動かすと終わりもほかの帯の端に吸い付く() {
    // 終わり（9.5 秒）を 5.03 秒へ。文字のレイヤーの終わり（5 秒）に吸い付く。シーケンスのカットの端は遠ざける。
    let mut m = model();
    m.items[0].cuts.clear();
    let mut c = ctl(&m);
    press_row(&mut c, &m, 900.0, 1);
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 453.0, 1));
    assert!((by + 4.5).abs() < 1e-9, "{by}");
    let kind = snap_kind(&c);
    assert!(
        matches!(kind, Some(SnapKind::LayerEdge { item: 2, .. })),
        "{kind:?}"
    );
}

#[test]
fn 動かすレイヤーの自分のキーフレームには吸い付かない() {
    let mut m = model();
    m.items[0].cuts.clear();
    m.items[2] = Item::layer(11.0, 14.0, ItemKind::Text, true);
    m.items[2].keyframes = [12.0].into();
    let mut c = ctl(&m);
    // 始まりを 11 秒から、自分のキーフレームの 12 秒へ（ちょうど重なる）。
    press_row(&mut c, &m, 1250.0, 2);
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 1350.0, 2));
    assert!((by - 1.0).abs() < 1e-9, "{by}");
    assert_eq!(snap_kind(&c), None);
}

#[test]
fn snapping_が切なら吸い付かずフレームにそろえる() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::ToggleSnapping);
    press_row(&mut c, &m, 900.0, 1);
    let (_, by, _) = edit_of(&move_row(&mut c, &m, 803.0, 1));
    assert!((by + 29.0 / 30.0).abs() < 1e-9, "{by}");
    assert_eq!(snap_kind(&c), None);
}

#[test]
fn ドラッグ中に_snapping_のキーを押すと吸い付きが反転する() {
    let m = model();
    let mut c = ctl(&m);
    press_row(&mut c, &m, 900.0, 1);
    move_row(&mut c, &m, 803.0, 1);
    let flipped = run(
        &mut c,
        &m,
        PointerEvent::Command(TimelineCommand::ToggleSnapping),
    );
    assert!((edit_of(&flipped).1 + 29.0 / 30.0).abs() < 1e-9);
    let released = run(&mut c, &m, PointerEvent::SnapKeyUp);
    assert!((edit_of(&released).1 + 1.0).abs() < 1e-9);
}

#[test]
fn selection_のカットの中は選ぶだけでドラッグしても編集にしない() {
    let m = model();
    let mut c = ctl(&m);
    let a = press_row(&mut c, &m, 100.0, 0);
    assert_eq!(
        a,
        vec![Action::Select(Some(0)), Action::SelectCut(Some((0, 0)))]
    );
    assert!(move_row(&mut c, &m, 140.0, 0).is_empty());
    assert!(run(&mut c, &m, PointerEvent::Up).is_empty());
}

#[test]
fn trim_のカットの中のドラッグは_slip() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::TrimTool);
    press_row(&mut c, &m, 100.0, 0);
    let (kind, ..) = edit_of(&move_row(&mut c, &m, 140.0, 0));
    assert_eq!(kind, EditKind::Slip { item: 0, cut: 0 });
}

#[test]
fn trim_のレイヤーの中は選ぶだけ() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::TrimTool);
    assert_eq!(
        press_row(&mut c, &m, 900.0, 1),
        vec![Action::Select(Some(1))]
    );
    assert!(move_row(&mut c, &m, 950.0, 1).is_empty());
}

#[test]
fn 帯の端のドラッグはどのツールでも同じ() {
    for tool_command in [TimelineCommand::SelectionTool, TimelineCommand::TrimTool] {
        let m = model();
        let mut c = ctl(&m);
        command(&mut c, &m, tool_command);
        press_row(&mut c, &m, 949.0, 1);
        let (kind, ..) = edit_of(&move_row(&mut c, &m, 900.0, 1));
        assert!(
            matches!(kind, EditKind::TrimLayer { item: 1, .. }),
            "{tool_command:?}"
        );
    }
}

#[test]
fn blade_のカットのクリックはポインターの時刻のフレームで割る() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::BladeTool);
    let a = press_row(&mut c, &m, 101.0, 0);
    let [Action::Split { item: 0, time }] = a[..] else {
        panic!("{a:?}")
    };
    assert!((time - 1.0).abs() < 1e-9, "1 秒に近いフレーム: {time}");
    assert!(run(&mut c, &m, PointerEvent::Up).is_empty());
    assert_eq!(c.selected_cut, None, "割るだけで選択は変えない");
}

#[test]
fn blade_の時刻は_snapping_が入なら再生位置に吸い付く() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::BladeTool);
    // 再生位置を 2 秒に置いて、2.03 秒をクリックする。
    let y = in_bar(&c, 0);
    let a = c.handle(down(203.0, y), &m, 2.0);
    let [Action::Split { time, .. }] = a[..] else {
        panic!("{a:?}")
    };
    assert_eq!(time, 2.0, "再生位置へ");
    command(&mut c, &m, TimelineCommand::ToggleSnapping);
    let b = c.handle(down(203.0, y), &m, 2.0);
    let [Action::Split { time, .. }] = b[..] else {
        panic!("{b:?}")
    };
    assert!(
        (time - 61.0 / 30.0).abs() < 1e-9,
        "吸い付かずフレームへ: {time}"
    );
}

#[test]
fn blade_のレイヤーと隙間のクリックは何もしない() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::BladeTool);
    assert!(press_row(&mut c, &m, 900.0, 1).is_empty());
    assert_eq!(c.selected, None);
    assert!(press_row(&mut c, &m, 350.0, 0).is_empty(), "隙間");
    let ruler = run(&mut c, &m, down(100.0, 30.0));
    assert!(matches!(ruler[..], [Action::Seek(_)]), "目盛りは今まで通り");
}

#[test]
fn blade_のホバーはカットの上に割る時刻の線を出す() {
    let m = model();
    let mut c = ctl(&m);
    move_row(&mut c, &m, 100.0, 0);
    assert_eq!(c.blade_line(), None, "Selection では出さない");
    command(&mut c, &m, TimelineCommand::BladeTool);
    move_row(&mut c, &m, 101.0, 0);
    let line = c.blade_line().unwrap();
    assert_eq!(line.item, 0);
    assert!((line.time - 1.0).abs() < 1e-9);
    move_row(&mut c, &m, 350.0, 0);
    assert_eq!(c.blade_line(), None, "隙間の上では出さない");
    move_row(&mut c, &m, 900.0, 1);
    assert_eq!(c.blade_line(), None, "レイヤーの上では出さない");
    move_row(&mut c, &m, 100.0, 0);
    run(&mut c, &m, PointerEvent::Leave);
    assert_eq!(c.blade_line(), None);
}

#[test]
fn blade_の割る時刻がカットの端に丸まる所では線を出さず_押しても割らない() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::BladeTool);
    // 4.01 秒は後ろのカット（4-7 秒）の始まり、6.99 秒は終わりに丸まる。
    for x in [401.0, 699.0] {
        move_row(&mut c, &m, x, 0);
        assert_eq!(c.blade_line(), None, "{x}");
        assert!(press_row(&mut c, &m, x, 0).is_empty(), "{x}");
        run(&mut c, &m, PointerEvent::Up);
    }
    move_row(&mut c, &m, 402.0, 0);
    assert!(c.blade_line().is_some(), "1 フレーム内側なら割れる");
}

#[test]
fn blade_の線は再生位置に吸い付いたときだけ吸い付いたフラグを持つ() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::BladeTool);
    let y = in_bar(&c, 0);
    c.handle(move_to(203.0, y), &m, 2.0);
    let line = c.blade_line().unwrap();
    assert!(line.snapped && line.time == 2.0, "{line:?}");
    c.handle(move_to(150.0, y), &m, 2.0);
    assert!(!c.blade_line().unwrap().snapped);
}

#[test]
fn ツールを替えると_blade_の線は消える() {
    let m = model();
    let mut c = ctl(&m);
    command(&mut c, &m, TimelineCommand::BladeTool);
    move_row(&mut c, &m, 100.0, 0);
    assert!(c.blade_line().is_some());
    command(&mut c, &m, TimelineCommand::SelectionTool);
    assert_eq!(c.blade_line(), None);
}
