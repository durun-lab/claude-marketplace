//! キーの表。キーの名前からコマンドの ID を引き、中身の実行は UI のコマンドの表（`commands.rs`）に任せる。
//! サーフェスと WebView のどちらから来たキーも同じ関数に入れる。
//! キーの名前は、英字、数字、矢印、Space などは `KeyboardEvent.code` の名前（`KeyZ`）、記号は押された文字（`=`）で、
//! JIS と US のキー配列で記号のキーの位置が違っても同じ名前になる。WebView の側は `web/transport.js` の `keyName` が同じ決まりで作る。

use std::sync::Arc;

use crate::win::commands::{self, Forward};
use crate::win::state::Shared;

/// コマンドの ID とキーの名前の組。ID の名前の順に並べ、同じ ID の中は画面に出すキーを先に置く。
const BINDINGS: &[(&str, &str)] = &[
    ("clip.extract", "Backspace"),
    ("clip.insert", ","),
    ("clip.lift", "Delete"),
    ("clip.overwrite", "."),
    ("clip.split", "KeyB"),
    ("edit.redo", "Ctrl+KeyY"),
    ("edit.redo", "Ctrl+Shift+KeyZ"),
    ("edit.undo", "Ctrl+KeyZ"),
    ("media.import", "Ctrl+KeyI"),
    ("playback.go_to_end", "End"),
    ("playback.go_to_start", "Home"),
    ("playback.next_edit", "ArrowDown"),
    ("playback.play_forward", "KeyL"),
    ("playback.play_reverse", "KeyJ"),
    ("playback.previous_edit", "ArrowUp"),
    ("playback.step_back", "ArrowLeft"),
    ("playback.step_back_second", "Shift+ArrowLeft"),
    ("playback.step_forward", "ArrowRight"),
    ("playback.step_forward_second", "Shift+ArrowRight"),
    ("playback.stop", "KeyK"),
    ("playback.toggle", "Space"),
    ("source.mark_in", "KeyI"),
    ("source.mark_out", "KeyO"),
    ("timeline.full_extent_zoom", "Shift+KeyZ"),
    ("timeline.zoom_in", "="),
    ("timeline.zoom_in", "+"),
    ("timeline.zoom_in", "Ctrl+="),
    ("timeline.zoom_out", "-"),
    ("timeline.zoom_out", "Ctrl+-"),
];

/// キーを押し続けたときに、OS のキーの繰り返しのたびに実行するコマンドの ID（ズームと、フレーム、秒、帯の端の移動）。
/// ID の名前の順。再生と停止、編集、全体表示の行き来は、繰り返すと押した回数より多く働くので最初の押下だけにする。
const REPEATS: &[&str] = &[
    "playback.next_edit",
    "playback.previous_edit",
    "playback.step_back",
    "playback.step_back_second",
    "playback.step_forward",
    "playback.step_forward_second",
    "timeline.zoom_in",
    "timeline.zoom_out",
];

/// code の名前のキー（英字、数字、矢印、Space など）の仮想キーコードの名前。WebView の `KeyboardEvent.code` と同じ名前にする。
/// 記号のキーはキー配列でキーの位置が違うので `None` にし、押された文字を名前にする（[`symbol_name`]）。
pub fn vk_name(vk: u32) -> Option<String> {
    let name = match vk {
        0x41..=0x5A => return Some(format!("Key{}", char::from_u32(vk)?)),
        0x30..=0x39 => return Some(format!("Digit{}", char::from_u32(vk)?)),
        0x70..=0x7B => return Some(format!("F{}", vk - 0x6F)),
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x1B => "Escape",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "ArrowLeft",
        0x26 => "ArrowUp",
        0x27 => "ArrowRight",
        0x28 => "ArrowDown",
        0x2D => "Insert",
        0x2E => "Delete",
        _ => return None,
    };
    Some(name.to_owned())
}

/// 記号のキーの名前。押された文字（今のキー配列で、Shift だけを押した状態で文字にしたもの）か、文字にならなければ仮想キーの番号。
pub fn symbol_name(vk: u32, typed: Option<char>) -> String {
    match typed {
        Some(c) => c.to_string(),
        None => format!("Vk{vk:02X}"),
    }
}

/// 修飾キーをキーの名前の前に Ctrl、Shift、Alt の順に付ける。Shift は英字、数字、矢印には付け、記号には付けない
/// （押された文字に Shift が入っている。JIS のキー配列の `=` は Shift+- で押すが、名前は `=`）。Space などほかの名前のキーは
/// Ctrl と一緒のときだけ付ける（Shift を押しながらの Space は Space のまま）。
pub fn with_modifiers(name: &str, ctrl: bool, shift: bool, alt: bool) -> String {
    let mut out = String::new();
    if ctrl {
        out.push_str("Ctrl+");
    }
    if shift && shift_in_name(name, ctrl) {
        out.push_str("Shift+");
    }
    if alt {
        out.push_str("Alt+");
    }
    out.push_str(name);
    out
}

/// Shift を押したときに、名前 `name` のキーの名前に Shift を付けるか。記号のキーの名前は押された文字 1 字。
fn shift_in_name(name: &str, ctrl: bool) -> bool {
    let symbol = name.chars().count() == 1;
    let named = ["Key", "Digit", "Arrow"]
        .iter()
        .any(|p| name.starts_with(p));
    !symbol && (named || ctrl)
}

/// キーに割り当てたコマンドの ID。
pub fn command_for(key: &str) -> Option<&'static str> {
    BINDINGS.iter().find(|(_, k)| *k == key).map(|(id, _)| *id)
}

/// キー `key` を押し続けたときの繰り返しも、コマンドを実行するか（[`REPEATS`]）。
pub fn repeats(key: &str) -> bool {
    command_for(key).is_some_and(|id| REPEATS.contains(&id))
}

/// コマンドに割り当てたキー。先頭が画面に出すキー。
pub fn keys_of(id: &str) -> Vec<&'static str> {
    BINDINGS
        .iter()
        .filter(|(i, _)| *i == id)
        .map(|(_, k)| *k)
        .collect()
}

/// キーの名前の先頭に付く修飾キー。`with_modifiers` が付ける順。
const MODIFIERS: [&str; 3] = ["Ctrl+", "Shift+", "Alt+"];

/// キーの名前を画面に出す見た目にする（`Ctrl+KeyZ` は `Ctrl+Z`、`ArrowLeft` は `←`）。
/// 修飾キーは先頭から順に外す。記号のキーは押された文字が名前になり（`+` もキーになる）、最後の `+` で切ると記号を落とすため。
pub fn label(key: &str) -> String {
    let mut out = String::new();
    let mut name = key;
    for modifier in MODIFIERS {
        if let Some(rest) = name.strip_prefix(modifier).filter(|r| !r.is_empty()) {
            out.push_str(modifier);
            name = rest;
        }
    }
    out.push_str(match name {
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        _ => name
            .strip_prefix("Key")
            .or_else(|| name.strip_prefix("Digit"))
            .unwrap_or(name),
    });
    out
}

/// Alt+英字のキーの名前（`Alt+KeyF`）の英字を小文字にしたもの。Ctrl や Shift と一緒のキーは含めない。
fn access_char(key: &str) -> Option<char> {
    let mut letters = key.strip_prefix("Alt+Key")?.chars();
    match (letters.next(), letters.next()) {
        (Some(c), None) if c.is_ascii_uppercase() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

/// キーを 1 つ受けるエントリーポイント。`source` は "surface" か "webview"。WebView2 は Alt+英字をウィンドウに渡さず
/// アクセスキーが効かないので、表に無い WebView の Alt+英字はウィンドウのフックでメニューを開かせる。
pub fn dispatch(shared: &Arc<Shared>, source: &str, key: &str) {
    let _span = tracing::info_span!("window.key").entered();
    let result = match (command_for(key), source, access_char(key)) {
        (Some(id), _, _) => commands::run_from_key(shared, id),
        (None, "webview", Some(c)) => shared.window_hook.send(Forward::MenuKey(c)),
        _ => Ok(()),
    };
    if let Err(e) = result {
        tracing::warn!(key, error = %e, "キーのコマンドを実行できない");
    }
    tracing::debug!(source, key, "key");
}

#[cfg(test)]
mod tests {
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
        let name = |vk, typed, ctrl, shift| {
            with_modifiers(&symbol_name(vk, Some(typed)), ctrl, shift, false)
        };
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

    /// 編集と取り込みのキーと、指すコマンドの ID と持ち主。
    fn expected_edits() -> Expected {
        use crate::win::commands::{Action, Owner};
        use crate::win::edit_actions::EditAction as E;
        let edit = |a| Owner::App(Action::Edit(a));
        vec![
            ("KeyI", "source.mark_in", edit(E::MarkIn)),
            ("KeyO", "source.mark_out", edit(E::MarkOut)),
            (",", "clip.insert", edit(E::Insert)),
            (".", "clip.overwrite", edit(E::Overwrite)),
            ("KeyB", "clip.split", edit(E::Split)),
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
        use mm_ui_layout::TimelineCommand::{FullExtentZoom, NextEdit, PreviousEdit};
        use mm_ui_layout::TimelineCommand::{ZoomIn, ZoomOut};
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
            ("Shift+KeyZ", "timeline.full_extent_zoom", t(FullExtentZoom)),
            ("=", "timeline.zoom_in", t(ZoomIn)),
            ("+", "timeline.zoom_in", t(ZoomIn)),
            ("Ctrl+=", "timeline.zoom_in", t(ZoomIn)),
            ("-", "timeline.zoom_out", t(ZoomOut)),
            ("Ctrl+-", "timeline.zoom_out", t(ZoomOut)),
        ]
    }

    #[test]
    fn キーからコマンドの_id_を引き_持ち主のコマンドを実行する() {
        let cases: Expected = expected_edits()
            .into_iter()
            .chain(expected_moves())
            .collect();
        assert_eq!(cases.len(), BINDINGS.len(), "全部のキーを確かめる");
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
    fn timeline_のコマンドのキーは描画スレッドに積む() {
        let shared = Arc::new(Shared::new());
        dispatch(&shared, "surface", "=");
        dispatch(&shared, "webview", "Shift+KeyZ");
        dispatch(&shared, "surface", "ArrowDown");
        use mm_ui_layout::TimelineCommand::{FullExtentZoom, NextEdit, ZoomIn};
        assert_eq!(
            *shared.timeline_commands.lock().unwrap(),
            [ZoomIn, FullExtentZoom, NextEdit]
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
    fn キーは重ならず_id_の順に並び_指すコマンドは表にある() {
        for (i, (id, key)) in BINDINGS.iter().enumerate() {
            let same = BINDINGS.iter().filter(|(_, k)| k == key).count();
            assert_eq!(same, 1, "{key} が 2 つのコマンドを指す");
            assert!(commands::find(id).is_some(), "{id} が表に無い");
            if let Some((next, _)) = BINDINGS.get(i + 1) {
                assert!(id <= next, "{id} と {next} の順");
            }
        }
    }

    #[test]
    fn キーの見た目() {
        assert_eq!(label("KeyB"), "B");
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
    fn source_を開いて向けていれば再生のキーは_source_に行く() {
        let shared = Arc::new(Shared::new());
        let (tx, rx) = channel();
        *shared.commands.lock().unwrap() = Some(tx);
        shared.source.set_active(true);
        dispatch(&shared, "surface", "KeyL");
        assert_eq!(rx.try_recv(), Ok(Command::Play), "素材が無ければ Program");
    }
}
