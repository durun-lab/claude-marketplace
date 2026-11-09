use std::sync::mpsc::channel;

use super::*;

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
fn id_は組と動詞の形() {
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
    assert!(import.keys.is_empty());
    assert_eq!(import.key_label, None);
    assert_eq!(import.panel, Some("media"));
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
        !enabled(&first, "clip.lift"),
        "カットを選ぶまで lift は押せない"
    );
    assert!(enabled(&first, "clip.split"));
    assert!(
        changed(&shared, &mut sent).is_none(),
        "変わらなければ送らない"
    );

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
fn 再生のコマンドを_program_に送り_知らない_id_とパネルのコマンドは断る() {
    let shared = Arc::new(Shared::new());
    let (tx, rx) = channel();
    *shared.commands.lock().unwrap() = Some(tx);
    run(&shared, "playback.toggle").expect("再生と停止");
    run(&shared, "playback.go_to_start").expect("先頭へ");
    run(&shared, "edit.undo").expect("JSON のプロジェクトの取り消し");
    assert_eq!(rx.try_recv(), Ok(Command::Toggle));
    assert_eq!(rx.try_recv(), Ok(Command::Seek(0.0)));
    assert_eq!(rx.try_recv(), Ok(Command::Undo));
    assert!(
        run(&shared, "media.import").is_err(),
        "パネルは WebView が開く"
    );
    assert!(run(&shared, "clip.nope").is_err());
    assert!(rx.try_recv().is_err(), "断ったコマンドは何も送らない");
}
