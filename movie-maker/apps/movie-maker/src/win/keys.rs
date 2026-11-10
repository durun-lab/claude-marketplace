//! キーの表。キーの名前からコマンドの ID を引き、中身の実行は UI のコマンドの表（`commands.rs`）に任せる。
//! サーフェスと WebView のどちらから来たキーも同じ関数に入れる。
//! キーの名前は、英字、数字、矢印、Space などは `KeyboardEvent.code` の名前（`KeyZ`）、記号は押された文字（`=`）で、
//! JIS と US のキー配列で記号のキーの位置が違っても同じ名前になる。WebView の側は `web/transport.js` の `keyName` が同じ決まりで作る。

use std::sync::Arc;

use crate::win::commands::{self, Forward};
use crate::win::state::Shared;

/// コマンドの ID とキーの名前の組。ID の名前の順に並べ、同じ ID の中は画面に出すキーを先に置く。
const BINDINGS: &[(&str, &str)] = &[
    ("app.show_keyboard_shortcuts", "?"),
    ("clip.extract", "Backspace"),
    ("clip.insert", ","),
    ("clip.lift", "Delete"),
    ("clip.overwrite", "."),
    ("clip.split", "Ctrl+KeyB"),
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
    ("source.clear_in_out", "Alt+KeyX"),
    ("source.mark_in", "KeyI"),
    ("source.mark_out", "KeyO"),
    ("timeline.blade_tool", "KeyB"),
    ("timeline.full_extent_zoom", "Shift+KeyZ"),
    ("timeline.selection_tool", "KeyA"),
    ("timeline.toggle_snapping", "KeyN"),
    ("timeline.trim_tool", "KeyT"),
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

/// コマンドのキーを画面に出す見た目（`keys_of` の先頭を `label` に通したもの）。割り当てが無ければ空。
pub fn key_label_of(id: &str) -> String {
    keys_of(id).first().map(|k| label(k)).unwrap_or_default()
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

/// キーを離した（サーフェスだけが送る）。押している間だけ効くコマンド（ドラッグ中のスナップの反転）を戻す。
/// 修飾キーを先に離しても戻るよう、修飾キーを付けない名前で引く。
pub fn release(shared: &Shared, key: &str) {
    if let Some(id) = command_for(key) {
        commands::release_from_key(shared, id);
    }
}

#[cfg(test)]
mod tests;
