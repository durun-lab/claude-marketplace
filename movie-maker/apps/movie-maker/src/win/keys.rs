//! キーの表。キーの名前からコマンドの ID を引き、中身の実行は UI のコマンドの表（`commands.rs`）に任せる。
//! サーフェスと WebView のどちらから来たキーも同じ関数に入れる。

use std::sync::Arc;

use crate::win::commands;
use crate::win::state::Shared;

/// コマンドの ID とキーの名前の組。ID の名前の順に並べ、同じ ID の中は画面に出すキーを先に置く。
const BINDINGS: &[(&str, &str)] = &[
    ("clip.extract", "Backspace"),
    ("clip.insert", "Comma"),
    ("clip.lift", "Delete"),
    ("clip.overwrite", "Period"),
    ("clip.split", "KeyB"),
    ("edit.redo", "Ctrl+KeyY"),
    ("edit.redo", "Ctrl+Shift+KeyZ"),
    ("edit.undo", "Ctrl+KeyZ"),
    ("playback.forward", "KeyL"),
    ("playback.go_to_end", "End"),
    ("playback.go_to_start", "Home"),
    ("playback.reverse", "KeyJ"),
    ("playback.step_back", "ArrowLeft"),
    ("playback.step_forward", "ArrowRight"),
    ("playback.stop", "KeyK"),
    ("playback.toggle", "Space"),
    ("source.mark_in", "KeyI"),
    ("source.mark_out", "KeyO"),
];

/// 仮想キーコードをキー名にする（WebView の `KeyboardEvent.code` に寄せた名前）。
pub fn vk_name(vk: u32) -> String {
    match vk {
        0x41..=0x5A => format!("Key{}", char::from_u32(vk).unwrap_or('?')),
        0x30..=0x39 => format!("Digit{}", char::from_u32(vk).unwrap_or('?')),
        0x20 => "Space".to_owned(),
        0x23 => "End".to_owned(),
        0x24 => "Home".to_owned(),
        0x08 => "Backspace".to_owned(),
        0x2E => "Delete".to_owned(),
        0xBC => "Comma".to_owned(),
        0xBE => "Period".to_owned(),
        0x25 => "ArrowLeft".to_owned(),
        0x27 => "ArrowRight".to_owned(),
        _ => format!("Vk{vk:02X}"),
    }
}

/// 修飾キーをキー名の前に付ける。Ctrl と Alt は常に、Shift は Ctrl と一緒のときだけ付ける
/// （Shift を押しながらの Space や矢印は、これまでどおりそのキーとして扱う）。
pub fn with_modifiers(name: &str, ctrl: bool, shift: bool, alt: bool) -> String {
    let mut out = String::new();
    if ctrl {
        out.push_str("Ctrl+");
        if shift {
            out.push_str("Shift+");
        }
    }
    if alt {
        out.push_str("Alt+");
    }
    out.push_str(name);
    out
}

/// キーに割り当てたコマンドの ID。
pub fn command_for(key: &str) -> Option<&'static str> {
    BINDINGS.iter().find(|(_, k)| *k == key).map(|(id, _)| *id)
}

/// コマンドに割り当てたキー。先頭が画面に出すキー。
pub fn keys_of(id: &str) -> Vec<&'static str> {
    BINDINGS
        .iter()
        .filter(|(i, _)| *i == id)
        .map(|(_, k)| *k)
        .collect()
}

/// キーの名前を画面に出す見た目にする（`Ctrl+KeyZ` は `Ctrl+Z`、`ArrowLeft` は `←`）。
pub fn label(key: &str) -> String {
    let (modifiers, name) = key.rsplit_once('+').unwrap_or(("", key));
    let shown = match name {
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "Comma" => ",",
        "Period" => ".",
        _ => name
            .strip_prefix("Key")
            .or_else(|| name.strip_prefix("Digit"))
            .unwrap_or(name),
    };
    if modifiers.is_empty() {
        shown.to_owned()
    } else {
        format!("{modifiers}+{shown}")
    }
}

/// キーを 1 つ受けるエントリーポイント。`source` は "surface" か "webview"。
pub fn dispatch(shared: &Arc<Shared>, source: &str, key: &str) {
    let _span = tracing::info_span!("window.key").entered();
    if let Some(id) = command_for(key) {
        if let Err(e) = commands::run(shared, id) {
            tracing::warn!(key, error = %e, "キーのコマンドを実行できない");
        }
    }
    tracing::debug!(source, key, "key");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::playback::Command;
    use std::sync::mpsc::channel;

    #[test]
    fn 英字と矢印は_code_の名前にする() {
        assert_eq!(vk_name(0x4A), "KeyJ");
        assert_eq!(vk_name(0x20), "Space");
        assert_eq!(vk_name(0x27), "ArrowRight");
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
    fn 修飾キーはキー名の前に付く_shift_は_ctrl_と一緒のときだけ() {
        assert_eq!(with_modifiers("KeyZ", true, false, false), "Ctrl+KeyZ");
        assert_eq!(with_modifiers("KeyZ", true, true, false), "Ctrl+Shift+KeyZ");
        assert_eq!(with_modifiers("Space", false, true, false), "Space");
        assert_eq!(with_modifiers("KeyX", false, false, true), "Alt+KeyX");
    }

    #[test]
    fn キーからコマンドの_id_を引く() {
        let cases = [
            ("KeyI", "source.mark_in"),
            ("KeyO", "source.mark_out"),
            ("Comma", "clip.insert"),
            ("Period", "clip.overwrite"),
            ("KeyB", "clip.split"),
            ("Delete", "clip.lift"),
            ("Backspace", "clip.extract"),
            ("Ctrl+KeyZ", "edit.undo"),
            ("Ctrl+KeyY", "edit.redo"),
            ("Ctrl+Shift+KeyZ", "edit.redo"),
            ("Space", "playback.toggle"),
            ("KeyJ", "playback.reverse"),
            ("KeyK", "playback.stop"),
            ("KeyL", "playback.forward"),
            ("ArrowLeft", "playback.step_back"),
            ("ArrowRight", "playback.step_forward"),
            ("Home", "playback.go_to_start"),
            ("End", "playback.go_to_end"),
        ];
        for (key, id) in cases {
            assert_eq!(command_for(key), Some(id), "{key}");
        }
        assert_eq!(command_for("KeyZ"), None);
        assert_eq!(keys_of("edit.redo"), ["Ctrl+KeyY", "Ctrl+Shift+KeyZ"]);
        assert!(keys_of("media.import").is_empty());
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
        assert_eq!(label("Comma"), ",");
        assert_eq!(label("Period"), ".");
        assert_eq!(label("ArrowLeft"), "←");
        assert_eq!(label("Digit1"), "1");
        assert_eq!(label("Space"), "Space");
        assert_eq!(label("Delete"), "Delete");
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
