use super::*;
use crate::win::playback::Command;
use std::sync::mpsc::channel;

#[test]
fn 英字と数字と矢印は_code_の名前にし_記号のキーは押された文字を名前にする() {
    assert_eq!(vk_name(0x4A).as_deref(), Some("KeyJ"));
    assert_eq!(vk_name(0x31).as_deref(), Some("Digit1"));
    assert_eq!(vk_name(0x20).as_deref(), Some("Space"));
    assert_eq!(vk_name(0x27).as_deref(), Some("ArrowRight"));
    assert_eq!(vk_name(0x26).as_deref(), Some("ArrowUp"));
    assert_eq!(vk_name(0x28).as_deref(), Some("ArrowDown"));
    assert_eq!(vk_name(0x71).as_deref(), Some("F2"));
    assert_eq!(vk_name(0xBD), None, "VK_OEM_MINUS はキー配列で文字が違う");
    assert_eq!(vk_name(0xBC), None, "VK_OEM_COMMA も記号");
    assert_eq!(symbol_name(0xBD, Some('=')), "=");
    assert_eq!(symbol_name(0xBA, None), "VkBA", "文字にならない記号のキー");
}

#[test]
fn 記号は押された文字の名前で_shift_を付けず_jis_の_shift_マイナスはズームイン() {
    // JIS のキー配列の VK_OEM_MINUS（0xBD）は - と、Shift で =。VK_OEM_PLUS（0xBB）は ; と、Shift で +。
    // US のキー配列の VK_OEM_PLUS は = と、Shift で +。
    let name =
        |vk, typed, ctrl, shift| with_modifiers(&symbol_name(vk, Some(typed)), ctrl, shift, false);
    assert_eq!(name(0xBD, '=', false, true), "=", "JIS の Shift+-");
    assert_eq!(name(0xBD, '-', false, false), "-");
    assert_eq!(name(0xBD, '=', true, true), "Ctrl+=", "JIS の Ctrl+Shift+-");
    assert_eq!(
        name(0xBB, '+', false, true),
        "+",
        "JIS の Shift+; と US の Shift+="
    );
    assert_eq!(name(0xBB, '=', true, false), "Ctrl+=", "US の Ctrl+=");
    for key in ["=", "+", "Ctrl+="] {
        assert_eq!(command_for(key), Some("timeline.zoom_in"), "{key}");
    }
    for key in ["-", "Ctrl+-"] {
        assert_eq!(command_for(key), Some("timeline.zoom_out"), "{key}");
    }
}

#[test]
fn サーフェスと_webview_のどちらのキーも同じコマンドになる() {
    let shared = Arc::new(Shared::new());
    let (tx, rx) = channel();
    *shared.commands.lock().unwrap() = Some(tx);
    dispatch(&shared, "surface", "Space");
    dispatch(&shared, "webview", "Space");
    dispatch(&shared, "webview", "KeyZ");
    assert_eq!(rx.try_recv(), Ok(Command::Toggle));
    assert_eq!(rx.try_recv(), Ok(Command::Toggle));
    assert!(rx.try_recv().is_err(), "割り当ての無いキーは何も送らない");
}

#[test]
fn 修飾キーは_ctrl_shift_alt_の順に付き_shift_は英字と数字と矢印か_ctrl_と一緒のときだけ() {
    assert_eq!(with_modifiers("KeyZ", true, false, false), "Ctrl+KeyZ");
    assert_eq!(with_modifiers("KeyZ", true, true, false), "Ctrl+Shift+KeyZ");
    assert_eq!(with_modifiers("KeyZ", false, true, false), "Shift+KeyZ");
    assert_eq!(
        with_modifiers("ArrowLeft", false, true, false),
        "Shift+ArrowLeft"
    );
    assert_eq!(with_modifiers("Digit1", false, true, false), "Shift+Digit1");
    assert_eq!(with_modifiers("Space", false, true, false), "Space");
    assert_eq!(with_modifiers("Home", false, true, false), "Home");
    assert_eq!(
        with_modifiers("Delete", true, true, false),
        "Ctrl+Shift+Delete"
    );
    assert_eq!(with_modifiers("KeyX", false, false, true), "Alt+KeyX");
    assert_eq!(with_modifiers("KeyF", false, true, true), "Shift+Alt+KeyF");
}

type Expected = Vec<(&'static str, &'static str, commands::Owner)>;

/// 編集、取り込み、ウィンドウのキーと、指すコマンドの ID と持ち主。
fn expected_edits() -> Expected {
    use crate::win::commands::{Action, Owner, WindowAction};
    use crate::win::edit_actions::EditAction as E;
    let edit = |a| Owner::App(Action::Edit(a));
    vec![
        (
            "?",
            "app.show_keyboard_shortcuts",
            Owner::App(Action::Window(WindowAction::ShowKeyboardShortcuts)),
        ),
        ("Alt+KeyX", "source.clear_in_out", edit(E::ClearInOut)),
        ("KeyI", "source.mark_in", edit(E::MarkIn)),
        ("KeyO", "source.mark_out", edit(E::MarkOut)),
        (",", "clip.insert", edit(E::Insert)),
        (".", "clip.overwrite", edit(E::Overwrite)),
        ("Ctrl+KeyB", "clip.split", edit(E::Split)),
        ("Delete", "clip.lift", edit(E::Lift)),
        ("Backspace", "clip.extract", edit(E::Extract)),
        ("Ctrl+KeyZ", "edit.undo", edit(E::Undo)),
        ("Ctrl+KeyY", "edit.redo", edit(E::Redo)),
        ("Ctrl+Shift+KeyZ", "edit.redo", edit(E::Redo)),
        ("Ctrl+KeyI", "media.import", Owner::Panel("media")),
    ]
}

/// 再生と Timeline のキーと、指すコマンドの ID と持ち主。
fn expected_moves() -> Expected {
    use crate::win::commands::{Action, Owner};
    use mm_ui_layout::TimelineCommand::{BladeTool, SelectionTool, TrimTool};
    use mm_ui_layout::TimelineCommand::{FullExtentZoom, NextEdit, PreviousEdit};
    use mm_ui_layout::TimelineCommand::{NextKeyframe, PreviousKeyframe, ShowKeyframeEditor};
    use mm_ui_layout::TimelineCommand::{ToggleSnapping, ZoomIn, ZoomOut};
    use Command::{Pause, Play, Reverse, Seek, Step, StepSeconds, Toggle};
    let p = |c| Owner::App(Action::Playback(c));
    let t = |c| Owner::App(Action::Timeline(c));
    vec![
        ("Space", "playback.toggle", p(Toggle)),
        ("KeyJ", "playback.play_reverse", p(Reverse)),
        ("KeyK", "playback.stop", p(Pause)),
        ("KeyL", "playback.play_forward", p(Play)),
        ("ArrowLeft", "playback.step_back", p(Step(-1))),
        ("ArrowRight", "playback.step_forward", p(Step(1))),
        (
            "Shift+ArrowLeft",
            "playback.step_back_second",
            p(StepSeconds(-1)),
        ),
        (
            "Shift+ArrowRight",
            "playback.step_forward_second",
            p(StepSeconds(1)),
        ),
        ("ArrowUp", "playback.previous_edit", t(PreviousEdit)),
        ("ArrowDown", "playback.next_edit", t(NextEdit)),
        ("Home", "playback.go_to_start", p(Seek(0.0))),
        ("End", "playback.go_to_end", p(Seek(f64::MAX))),
        ("[", "playback.go_to_previous_keyframe", t(PreviousKeyframe)),
        ("]", "playback.go_to_next_keyframe", t(NextKeyframe)),
        (
            "KeyU",
            "timeline.show_keyframe_editor",
            t(ShowKeyframeEditor),
        ),
        ("Shift+KeyZ", "timeline.full_extent_zoom", t(FullExtentZoom)),
        ("KeyA", "timeline.selection_tool", t(SelectionTool)),
        ("KeyT", "timeline.trim_tool", t(TrimTool)),
        ("KeyB", "timeline.blade_tool", t(BladeTool)),
        ("KeyN", "timeline.toggle_snapping", t(ToggleSnapping)),
        ("=", "timeline.zoom_in", t(ZoomIn)),
        ("+", "timeline.zoom_in", t(ZoomIn)),
        ("Ctrl+=", "timeline.zoom_in", t(ZoomIn)),
        ("-", "timeline.zoom_out", t(ZoomOut)),
        ("Ctrl+-", "timeline.zoom_out", t(ZoomOut)),
    ]
}

/// 2 つのコマンドに割り当てたキーと、その 2 つ目のコマンド。キーの先頭のコマンドは `expected_edits` と `expected_moves` が確かめる。
const TWO_COMMAND_KEYS: &[(&str, &str)] = &[("Delete", "timeline.remove_keyframe")];

#[test]
fn キーからコマンドの_id_を引き_持ち主のコマンドを実行する() {
    let cases: Expected = expected_edits()
        .into_iter()
        .chain(expected_moves())
        .collect();
    let all = cases.len() + TWO_COMMAND_KEYS.len();
    assert_eq!(all, BINDINGS.len(), "全部のキーを確かめる");
    for (key, id, owner) in cases {
        assert_eq!(command_for(key), Some(id), "{key}");
        let spec = commands::find(id).expect(id);
        assert_eq!(spec.owner, owner, "{key}");
    }
    assert_eq!(command_for("KeyZ"), None);
    assert_eq!(keys_of("edit.redo"), ["Ctrl+KeyY", "Ctrl+Shift+KeyZ"]);
    assert_eq!(keys_of("timeline.zoom_in"), ["=", "+", "Ctrl+="]);
    assert!(keys_of("export.start").is_empty());
}

#[test]
fn timeline_のコマンドのキーはポインターの入力のキューに積む() {
    use mm_ui_layout::PointerEvent::Command;
    use mm_ui_layout::TimelineCommand::{FullExtentZoom, NextEdit, ZoomIn};
    let shared = Arc::new(Shared::new());
    dispatch(&shared, "surface", "=");
    dispatch(&shared, "webview", "Shift+KeyZ");
    dispatch(&shared, "surface", "ArrowDown");
    assert_eq!(
        *shared.input.lock().unwrap(),
        [Command(ZoomIn), Command(FullExtentZoom), Command(NextEdit)]
    );
}

#[test]
fn 押し続けで繰り返すのはズームと移動のキーだけ() {
    for key in [
        "=",
        "+",
        "Ctrl+-",
        "ArrowLeft",
        "Shift+ArrowRight",
        "ArrowUp",
        "[",
        "]",
    ] {
        assert!(repeats(key), "{key}");
    }
    for key in [
        "Space",
        "KeyL",
        "Shift+KeyZ",
        "Delete",
        "Ctrl+KeyZ",
        "Home",
        "KeyQ",
    ] {
        assert!(!repeats(key), "{key}");
    }
    for id in REPEATS {
        assert!(!keys_of(id).is_empty(), "{id} はキーの表にある");
    }
}

/// ウィンドウのフックに渡ったものを記録する。
fn record_forwards(shared: &Shared) -> Arc<std::sync::Mutex<Vec<Forward>>> {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = seen.clone();
    shared
        .window_hook
        .set(move |f| sink.lock().unwrap().push(f));
    seen
}

#[test]
fn ctrl_i_は_import_media_をウィンドウのフックで_shell_に渡す() {
    let shared = Arc::new(Shared::new());
    let seen = record_forwards(&shared);
    dispatch(&shared, "surface", "Ctrl+KeyI");
    dispatch(&shared, "webview", "Ctrl+KeyI");
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Forward::Panel("media.import"),
            Forward::Panel("media.import")
        ]
    );
}

#[test]
fn webview_の表に無い_alt_英字はウィンドウにメニューを開かせる() {
    let shared = Arc::new(Shared::new());
    let seen = record_forwards(&shared);
    dispatch(&shared, "webview", "Alt+KeyF");
    dispatch(&shared, "surface", "Alt+KeyE");
    dispatch(&shared, "webview", "Ctrl+Alt+KeyE");
    dispatch(&shared, "webview", "Alt+Digit1");
    assert_eq!(
        *seen.lock().unwrap(),
        [Forward::MenuKey('f')],
        "サーフェスの Alt+英字は WM_SYSCHAR で開く"
    );
    assert_eq!(access_char("Alt+KeyT"), Some('t'));
    assert_eq!(access_char("Alt+KeyF10"), None);
    assert_eq!(access_char("KeyF"), None);
}

#[test]
fn キーは_2_つのコマンドに割り当てたもののほかは重ならず_id_の順に並び_指すコマンドは表にある() {
    for (i, (id, key)) in BINDINGS.iter().enumerate() {
        let same = BINDINGS.iter().filter(|(_, k)| k == key).count();
        let two = TWO_COMMAND_KEYS.iter().any(|(k, _)| k == key);
        assert_eq!(same, 1 + usize::from(two), "{key} が 2 つのコマンドを指す");
        assert!(commands::find(id).is_some(), "{id} が表に無い");
        if let Some((next, _)) = BINDINGS.get(i + 1) {
            assert!(id <= next, "{id} と {next} の順");
        }
    }
}

#[test]
fn キーの見た目() {
    assert_eq!(label("KeyB"), "B");
    assert_eq!(label("Ctrl+KeyB"), "Ctrl+B");
    assert_eq!(label("Ctrl+KeyZ"), "Ctrl+Z");
    assert_eq!(label("Ctrl+Shift+KeyZ"), "Ctrl+Shift+Z");
    assert_eq!(label(","), ",");
    assert_eq!(label("."), ".");
    assert_eq!(label("ArrowLeft"), "←");
    assert_eq!(label("Shift+ArrowRight"), "Shift+→");
    assert_eq!(label("ArrowDown"), "↓");
    assert_eq!(label("Digit1"), "1");
    assert_eq!(label("Space"), "Space");
    assert_eq!(label("Delete"), "Delete");
    assert_eq!(label("Shift+KeyZ"), "Shift+Z");
    assert_eq!(label("Alt+KeyX"), "Alt+X");
    assert_eq!(label("ArrowUp"), "↑");
    assert_eq!(label("+"), "+", "記号のキーは押された文字");
    assert_eq!(label("Ctrl+="), "Ctrl+=");
    assert_eq!(label("Ctrl++"), "Ctrl++");
    assert_eq!(label("-"), "-");
}

#[test]
fn 空の状態の案内が引くコマンドにはキーがある() {
    assert_eq!(key_label_of("media.import"), "Ctrl+I");
    assert_eq!(key_label_of("source.mark_in"), "I");
    assert_eq!(key_label_of("source.mark_out"), "O");
    assert_eq!(key_label_of("clip.insert"), ",");
    assert_eq!(key_label_of("no.such"), "");
}

#[test]
fn n_はスナップの入_切を描画スレッドへ渡し_離すとドラッグ中の反転を戻す() {
    use mm_ui_layout::{PointerEvent, TimelineCommand};
    let toggle = PointerEvent::Command(TimelineCommand::ToggleSnapping);
    let shared = Arc::new(Shared::new());
    dispatch(&shared, "surface", "KeyN");
    release(&shared, "KeyN");
    release(&shared, "KeyK");
    dispatch(&shared, "webview", "KeyN");
    assert_eq!(
        *shared.input.lock().unwrap(),
        [toggle, PointerEvent::SnapKeyUp, toggle],
        "ドラッグとの前後を保つため、ポインターの入力のキューに入れる。K を離しても何もしない"
    );
}

#[test]
fn source_を開いて向けていれば再生のキーは_source_に行く() {
    let shared = Arc::new(Shared::new());
    let (tx, rx) = channel();
    *shared.commands.lock().unwrap() = Some(tx);
    shared.source.set_active(true);
    dispatch(&shared, "surface", "KeyL");
    assert_eq!(rx.try_recv(), Ok(Command::Play), "素材が無ければ Program");
}

#[test]
fn 疑問符は_keyboard_shortcuts_を開き_jis_と_us_のどちらの_shift_スラッシュも同じ名前() {
    // JIS も US も ? は Shift+/ で、押された文字が名前になる（Shift は付かない）。
    assert_eq!(
        with_modifiers(&symbol_name(0xBF, Some('?')), false, true, false),
        "?"
    );
    assert_eq!(command_for("?"), Some("app.show_keyboard_shortcuts"));
    assert_eq!(key_label_of("app.show_keyboard_shortcuts"), "?");
}

#[test]
fn delete_はキーフレームの行で選んだキーフレームがあれば外し_カットを選んでいればカットを抜く() {
    use crate::win::keyframe_edit::RowSelection;
    use mm_ui_layout::{PointerEvent, TimelineCommand};
    let shared = Arc::new(Shared::new());
    assert_eq!(
        command_now(&shared, "Delete"),
        Some("clip.lift"),
        "どれも押せなければ先頭"
    );
    shared.keyframes.select(Some(RowSelection {
        layer: "title".into(),
        path: Some("opacity".into()),
        keyframe: true,
    }));
    assert_eq!(
        command_now(&shared, "Delete"),
        Some("timeline.remove_keyframe")
    );
    dispatch(&shared, "surface", "Delete");
    let remove = PointerEvent::Command(TimelineCommand::RemoveKeyframe);
    assert_eq!(
        *shared.input.lock().unwrap(),
        [remove],
        "描画スレッドが選んだキーフレームを外す"
    );
    shared.edit.select_cut(Some("k1".into()));
    assert_eq!(
        command_now(&shared, "Delete"),
        Some("clip.lift"),
        "カットを選んでいればカットが先"
    );
}
