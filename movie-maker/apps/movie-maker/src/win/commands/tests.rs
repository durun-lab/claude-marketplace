use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use mm_types::Id;

use super::*;
use crate::win::sequence_edit::tests::sample_project;

/// Shell が読むパネルの登録の列。パネルが持ち主のコマンドは、ここにあるパネルを指す。
const PANELS_TS: &str = include_str!("../../../../../ui/shell/src/panels.ts");

fn row<'a>(table: &'a [CommandView], id: &str) -> &'a CommandView {
    table.iter().find(|c| c.id == id).expect(id)
}

#[test]
fn id_は重ならず名前の順に並ぶ() {
    for pair in TABLE.windows(2) {
        assert!(pair[0].id < pair[1].id, "{} と {}", pair[0].id, pair[1].id);
    }
}

#[test]
fn id_は分類と動詞の形() {
    for spec in TABLE {
        let parts: Vec<&str> = spec.id.split('.').collect();
        assert_eq!(parts.len(), 2, "{}", spec.id);
        for part in parts {
            assert!(!part.is_empty(), "{}", spec.id);
            assert!(
                part.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                spec.id
            );
        }
    }
}

#[test]
fn パネルが持ち主のコマンドは登録されたパネルを指す() {
    for spec in TABLE {
        if let Owner::Panel(panel) = spec.owner {
            let export = format!("panel as {panel} }}");
            assert!(PANELS_TS.contains(&export), "{}: {panel}", spec.id);
        }
    }
}

#[test]
fn 表のキーの列はキーの表から引き_見た目を付ける() {
    let table = table(&Shared::new());
    let split = row(&table, "clip.split");
    assert_eq!(split.keys, ["KeyB"]);
    assert_eq!(split.key_label.as_deref(), Some("B"));
    assert_eq!(split.panel, None);
    let redo = row(&table, "edit.redo");
    assert_eq!(redo.keys, ["Ctrl+KeyY", "Ctrl+Shift+KeyZ"]);
    assert_eq!(redo.key_label.as_deref(), Some("Ctrl+Y"));
    let import = row(&table, "media.import");
    assert_eq!(import.keys, ["Ctrl+KeyI"]);
    assert_eq!(import.key_label.as_deref(), Some("Ctrl+I"));
    assert_eq!(import.panel, Some("media"));
    let export = row(&table, "export.start");
    assert!(export.keys.is_empty());
    assert_eq!(export.key_label, None);
}

#[test]
fn webview_へは_ui_sdk_の_ui_command_と同じ形で送る() {
    let table = table(&Shared::new());
    let json = serde_json::to_value(row(&table, "clip.lift")).expect("JSON にする");
    assert_eq!(
        json,
        serde_json::json!({
            "id": "clip.lift",
            "name": "Lift",
            "keys": ["Delete"],
            "key_label": "Delete",
            "description": "Remove the selected clip and leave a gap",
            "menu": "edit",
            "panel": null,
            "enabled": false,
            "checked": null,
        })
    );
}

#[test]
fn 押せるかが変わると表を送り直す() {
    let shared = Shared::new();
    let mut sent = None;
    let enabled = |table: &[CommandView], id: &str| row(table, id).state.enabled;
    let first = changed(&shared, &mut sent).expect("最初は送る");
    assert!(
        !enabled(&first, "clip.split"),
        "シーケンスのレイヤーが無ければ split は押せない"
    );
    assert!(
        !enabled(&first, "clip.lift"),
        "カットを選ぶまで lift は押せない"
    );
    assert!(enabled(&first, "playback.toggle"));
    assert!(
        changed(&shared, &mut sent).is_none(),
        "変わらなければ送らない"
    );

    shared.publish_doc(Arc::new(sample_project()), Id::new("main"));
    let opened = changed(&shared, &mut sent).expect("シーケンスのあるプロジェクトを開くと送る");
    assert!(enabled(&opened, "clip.split"));
    assert!(!enabled(&opened, "clip.lift"));

    shared.edit.select_cut(Some("k1".to_owned()));
    let selected = changed(&shared, &mut sent).expect("カットを選ぶと送る");
    assert!(enabled(&selected, "clip.lift"));
    assert!(enabled(&selected, "clip.extract"));
    assert!(
        !enabled(&selected, "clip.insert"),
        "Source を開くまで insert は押せない"
    );
    assert!(changed(&shared, &mut sent).is_none());

    shared.edit.select_cut(None);
    let cleared = changed(&shared, &mut sent).expect("選択を外すと送る");
    assert!(!enabled(&cleared, "clip.lift"));
}

#[test]
fn snapping_はチェックの状態に表示の状態の入_切を出し_変われば表を送り直す() {
    let shared = Shared::new();
    let mut sent = None;
    let checked = |table: &[CommandView]| row(table, "timeline.toggle_snapping").state.checked;
    let first = changed(&shared, &mut sent).expect("最初は送る");
    assert_eq!(checked(&first), Some(true), "表示の状態が無ければ入");
    assert_eq!(
        row(&first, "clip.lift").state.checked,
        None,
        "入/切の無いコマンド"
    );
    let mut state = mm_ui_layout::ViewState {
        time: mm_ui_layout::TimeView {
            origin: 0.0,
            px_per_sec: 100.0,
        },
        scroll_y: 0.0,
        heights: mm_ui_layout::RowHeights::default(),
        snapping: false,
    };
    shared.view.publish(state.clone());
    let off = changed(&shared, &mut sent).expect("切ると送る");
    assert_eq!(checked(&off), Some(false));
    state.scroll_y = 10.0;
    shared.view.publish(state);
    assert!(
        changed(&shared, &mut sent).is_none(),
        "スクロールでは送らない"
    );
}

#[test]
fn snapping_はメニューからもキーからも描画スレッドへ入_切を渡す() {
    let toggle = PointerEvent::Command(TimelineCommand::ToggleSnapping);
    let shared = Arc::new(Shared::new());
    run(&shared, "timeline.toggle_snapping").expect("メニュー");
    run_from_key(&shared, "timeline.toggle_snapping").expect("キー");
    release_from_key(&shared, "timeline.toggle_snapping");
    release_from_key(&shared, "playback.toggle");
    assert_eq!(
        *shared.input.lock().unwrap(),
        [toggle, toggle, PointerEvent::SnapKeyUp]
    );
}

#[test]
fn 再生のコマンドを_program_に送り_表に無い_id_は拒否する() {
    let shared = Arc::new(Shared::new());
    let (tx, rx) = channel();
    *shared.commands.lock().unwrap() = Some(tx);
    run(&shared, "playback.toggle").expect("再生と停止");
    run(&shared, "playback.go_to_start").expect("先頭へ");
    run(&shared, "edit.undo").expect("JSON のプロジェクトの取り消し");
    assert_eq!(rx.try_recv(), Ok(Command::Toggle));
    assert_eq!(rx.try_recv(), Ok(Command::Seek(0.0)));
    assert_eq!(rx.try_recv(), Ok(Command::Undo));
    assert_eq!(
        run(&shared, "media.import"),
        Err("The window isn't ready yet".to_owned()),
        "ウィンドウを作る前は、パネルのコマンドを渡す先が無い"
    );
    assert!(run(&shared, "clip.nope").is_err());
    assert!(rx.try_recv().is_err(), "拒否したコマンドは何も送らない");
}

#[test]
fn timeline_のコマンドはポインターの入力のキューに積み_秒の移動は再生へ送る() {
    use PointerEvent::Command as C;
    let shared = Arc::new(Shared::new());
    let (tx, rx) = channel();
    *shared.commands.lock().unwrap() = Some(tx);
    run(&shared, "timeline.zoom_in").expect("Zoom In");
    run_from_key(&shared, "timeline.full_extent_zoom").expect("Full Extent Zoom");
    run(&shared, "playback.next_edit").expect("Next Edit");
    assert_eq!(
        *shared.input.lock().unwrap(),
        [
            C(TimelineCommand::ZoomIn),
            C(TimelineCommand::FullExtentZoom),
            C(TimelineCommand::NextEdit)
        ]
    );
    assert!(
        rx.try_recv().is_err(),
        "Timeline のコマンドは再生へ送らない"
    );
    run(&shared, "playback.step_back_second").expect("Step One Second Back");
    assert_eq!(rx.try_recv(), Ok(Command::StepSeconds(-1)));
}

#[test]
fn パネルとウィンドウのコマンドはウィンドウのフックへ渡す() {
    let shared = Arc::new(Shared::new());
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = seen.clone();
    shared
        .window_hook
        .set(move |f| sink.lock().unwrap().push(f));
    run(&shared, "export.start").expect("Deliver を開く");
    run_from_key(&shared, "media.import").expect("Media を開いて取り込みを始める");
    run(&shared, "app.exit").expect("閉じる");
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Forward::Panel("export.start"),
            Forward::Panel("media.import"),
            Forward::Window(WindowAction::Exit),
        ]
    );
}

#[test]
fn 押せないコマンドは_run_が拒否し_キーからは編集が拒否した理由を出す() {
    let shared = Arc::new(Shared::new());
    assert_eq!(
        run(&shared, "clip.split"),
        Err("Split Clip isn't available now".to_owned()),
        "シーケンスのレイヤーが無ければ split は押せない"
    );
    assert_eq!(
        shared.edit.message(),
        "",
        "拒否したコマンドは編集を積まない"
    );
    run_from_key(&shared, "clip.split").expect("キーは押せるかを見ない");
    let start = Instant::now();
    while shared.edit.message().is_empty() && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !shared.edit.message().is_empty(),
        "編集のキューが拒否した理由を文言に置く"
    );
}
