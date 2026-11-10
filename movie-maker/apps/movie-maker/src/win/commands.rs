//! UI のコマンドの表。呼び出し箇所（キー、メニュー、Tasks の項目）は、この表の ID でコマンドを指す。
//! 表は人が呼ぶコマンドだけを持ち、データの変更は実行した先の編集が操作（`Op`）で出す。

use std::sync::Arc;

use mm_doc::SEQUENCE_KIND;
use mm_ui_layout::{PointerEvent, TimelineCommand};
use serde::Serialize;

use crate::win::edit_actions::{self, EditAction};
use crate::win::keys;
use crate::win::menu::{self, Menu};
use crate::win::playback::Command;
use crate::win::state::Shared;

/// 押せるかが変わったことを WebView に知らせる Tauri のイベント名。中身は新しい表（`Vec<CommandView>`）。
pub const COMMANDS_CHANGED_EVENT: &str = "mm:commands-changed";
/// パネルが持ち主のコマンドを、メニューやキーから WebView の Shell に渡す Tauri のイベント名。中身はコマンドの ID。
pub const PANEL_COMMAND_EVENT: &str = "mm:panel-command";

/// コマンドを実行する所。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Owner {
    /// アプリ（Rust）が実行する。
    App(Action),
    /// WebView のパネル（`definePanel` の id）。開くのは WebView の Shell。
    Panel(&'static str),
}

/// アプリが実行するコマンドの中身。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// 名前のついた編集。編集のキューで計画して適用する。
    Edit(EditAction),
    /// 再生のコマンド。Source を最後にクリックしていれば Source の、そうでなければ Program の再生に向く。
    Playback(Command),
    /// Shy のコマンド。表示の状態を変えるだけで、操作にならない。
    Shy(ShyCommand),
    /// Timeline の表示と再生位置のコマンド。ポインターの入力のキューで描画スレッドへ渡し、Timeline の操作が
    /// ドラッグとの前後を保って実行する。
    Timeline(TimelineCommand),
    /// Tauri のウィンドウが実行するコマンド。
    Window(WindowAction),
}

/// Tauri のウィンドウが実行するコマンドの中身。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    /// 閉じるボタンと同じく、描画と再生を止めてからウィンドウを閉じる。
    Exit,
    /// アプリの名前とバージョンを出す。
    About,
    /// Keyboard Shortcuts のウィンドウを出す。開いていれば前に出す。
    ShowKeyboardShortcuts,
}

/// 表の 1 行。キーはキーの表（`keys.rs`）、メニューでの場所はメニューの並び（`menu.rs`）が持ち、
/// WebView に送るときに ID で引いて足す。
#[derive(Debug)]
pub struct Spec {
    /// `<分類>.<動詞>`。Tasks の項目の `command` と同じ形。
    pub id: &'static str,
    /// 画面の名前（英語）。
    pub name: &'static str,
    /// ツールチップの 2 行目。名前だけで分かるコマンドは持たない。
    pub description: Option<&'static str>,
    pub owner: Owner,
}

/// コマンドの表。ID の名前の順に並べ、足すときもその位置に入れる。
pub static TABLE: &[Spec] = &[
    Spec {
        id: "app.exit",
        name: "Exit",
        description: None,
        owner: Owner::App(Action::Window(WindowAction::Exit)),
    },
    Spec {
        id: "app.show_about",
        name: "About",
        description: None,
        owner: Owner::App(Action::Window(WindowAction::About)),
    },
    Spec {
        id: "app.show_keyboard_shortcuts",
        name: "Keyboard Shortcuts…",
        description: Some("List every command with its key, and run one by name"),
        owner: Owner::App(Action::Window(WindowAction::ShowKeyboardShortcuts)),
    },
    Spec {
        id: "clip.add_transition",
        name: "Add Transition…",
        description: None,
        owner: Owner::Panel("transition"),
    },
    Spec {
        id: "clip.extract",
        name: "Extract",
        description: Some("Remove the selected clip and close the gap"),
        owner: Owner::App(Action::Edit(EditAction::Extract)),
    },
    Spec {
        id: "clip.insert",
        name: "Insert",
        description: Some("Insert the Source range at the edit point nearest the playhead"),
        owner: Owner::App(Action::Edit(EditAction::Insert)),
    },
    Spec {
        id: "clip.lift",
        name: "Lift",
        description: Some("Remove the selected clip and leave a gap"),
        owner: Owner::App(Action::Edit(EditAction::Lift)),
    },
    Spec {
        id: "clip.overwrite",
        name: "Overwrite",
        description: Some(
            "Replace the selected clip, or the one at the playhead, with the Source range",
        ),
        owner: Owner::App(Action::Edit(EditAction::Overwrite)),
    },
    Spec {
        id: "clip.split",
        name: "Split Clip",
        description: Some("Split the clip at the playhead"),
        owner: Owner::App(Action::Edit(EditAction::Split)),
    },
    Spec {
        id: "edit.redo",
        name: "Redo",
        description: None,
        owner: Owner::App(Action::Edit(EditAction::Redo)),
    },
    Spec {
        id: "edit.undo",
        name: "Undo",
        description: None,
        owner: Owner::App(Action::Edit(EditAction::Undo)),
    },
    Spec {
        id: "export.choose_target",
        name: "Choose Render Target…",
        description: None,
        owner: Owner::Panel("deliver"),
    },
    Spec {
        id: "export.start",
        name: "Export…",
        description: None,
        owner: Owner::Panel("deliver"),
    },
    Spec {
        id: "media.import",
        name: "Import Media…",
        description: None,
        owner: Owner::Panel("media"),
    },
    Spec {
        id: "playback.go_to_end",
        name: "Go to End",
        description: None,
        owner: Owner::App(Action::Playback(Command::Seek(f64::MAX))),
    },
    Spec {
        id: "playback.go_to_start",
        name: "Go to Start",
        description: None,
        owner: Owner::App(Action::Playback(Command::Seek(0.0))),
    },
    Spec {
        id: "playback.next_edit",
        name: "Next Edit",
        description: Some("Move the playhead to the next clip or layer boundary"),
        owner: Owner::App(Action::Timeline(TimelineCommand::NextEdit)),
    },
    Spec {
        id: "playback.play_forward",
        name: "Play Forward",
        description: None,
        owner: Owner::App(Action::Playback(Command::Play)),
    },
    Spec {
        id: "playback.play_reverse",
        name: "Play Reverse",
        description: Some("Play the video backward without sound"),
        owner: Owner::App(Action::Playback(Command::Reverse)),
    },
    Spec {
        id: "playback.previous_edit",
        name: "Previous Edit",
        description: Some("Move the playhead to the previous clip or layer boundary"),
        owner: Owner::App(Action::Timeline(TimelineCommand::PreviousEdit)),
    },
    Spec {
        id: "playback.step_back",
        name: "Step One Frame Back",
        description: None,
        owner: Owner::App(Action::Playback(Command::Step(-1))),
    },
    Spec {
        id: "playback.step_back_second",
        name: "Step One Second Back",
        description: None,
        owner: Owner::App(Action::Playback(Command::StepSeconds(-1))),
    },
    Spec {
        id: "playback.step_forward",
        name: "Step One Frame Forward",
        description: None,
        owner: Owner::App(Action::Playback(Command::Step(1))),
    },
    Spec {
        id: "playback.step_forward_second",
        name: "Step One Second Forward",
        description: None,
        owner: Owner::App(Action::Playback(Command::StepSeconds(1))),
    },
    Spec {
        id: "playback.stop",
        name: "Stop",
        description: None,
        owner: Owner::App(Action::Playback(Command::Pause)),
    },
    Spec {
        id: "playback.toggle",
        name: "Play / Stop",
        description: None,
        owner: Owner::App(Action::Playback(Command::Toggle)),
    },
    Spec {
        id: "source.clear_in_out",
        name: "Clear In and Out",
        description: Some("Remove the In and Out points of the clip open in Source"),
        owner: Owner::App(Action::Edit(EditAction::ClearInOut)),
    },
    Spec {
        id: "source.mark_in",
        name: "Mark In",
        description: Some("Mark the In point of the clip open in Source"),
        owner: Owner::App(Action::Edit(EditAction::MarkIn)),
    },
    Spec {
        id: "source.mark_out",
        name: "Mark Out",
        description: Some("Mark the Out point of the clip open in Source"),
        owner: Owner::App(Action::Edit(EditAction::MarkOut)),
    },
    Spec {
        id: "timeline.full_extent_zoom",
        name: "Full Extent Zoom",
        description: Some(
            "Fit the whole composition in the Timeline, or return to the previous zoom",
        ),
        owner: Owner::App(Action::Timeline(TimelineCommand::FullExtentZoom)),
    },
    Spec {
        id: "timeline.hide_shy_layers",
        name: "Hide Shy Layers",
        description: Some(
            "Hide the Shy layers from the Timeline; rendering and export don't change",
        ),
        owner: Owner::App(Action::Shy(ShyCommand::HideShyLayers)),
    },
    Spec {
        id: "timeline.shy_selected_layers",
        name: "Shy Selected Layers",
        description: Some("Mark the selected layer Shy so Hide Shy Layers can hide its row"),
        owner: Owner::App(Action::Shy(ShyCommand::ShySelectedLayers)),
    },
    Spec {
        id: "timeline.toggle_snapping",
        name: "Snapping",
        description: Some(
            "Snap dragged edges to the playhead, clip and layer edges, keyframes, and markers",
        ),
        owner: Owner::App(Action::Timeline(TimelineCommand::ToggleSnapping)),
    },
    Spec {
        id: "timeline.zoom_in",
        name: "Zoom In",
        description: Some("Zoom in around the playhead"),
        owner: Owner::App(Action::Timeline(TimelineCommand::ZoomIn)),
    },
    Spec {
        id: "timeline.zoom_out",
        name: "Zoom Out",
        description: Some("Zoom out around the playhead"),
        owner: Owner::App(Action::Timeline(TimelineCommand::ZoomOut)),
    },
];

/// ID で表の行を引く。
pub fn find(id: &str) -> Option<&'static Spec> {
    TABLE.iter().find(|s| s.id == id)
}

/// 押せるかとチェックの状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct State {
    pub enabled: bool,
    /// 入/切のあるコマンドだけが持つ。
    pub checked: Option<bool>,
}

/// 押せるかを決める今の状態。表を作るたびに 1 度だけ読む。
#[derive(Default)]
struct Ready {
    /// Source に素材を開いている。イン点・アウト点と insert、overwrite は Source の範囲を使う。
    source_open: bool,
    /// 開いているコンポジションにシーケンスのレイヤーがある。split はそのカットを割る。
    sequence: bool,
    /// Timeline でカットを選んでいる。lift と extract は選んだカットを抜く。
    cut_selected: bool,
    /// スナップの入/切（描画スレッドが置いた表示の状態）。
    snapping: bool,
    /// Shy の行を隠すかどうか（表示の状態）と、選んでいるレイヤーの Shy のフラグ。
    shy: shy::ShyReady,
}

impl Ready {
    fn of(shared: &Shared) -> Self {
        let sequence = shared.doc().is_some_and(|doc| {
            doc.project
                .composition(&doc.comp)
                .is_ok_and(|c| c.layers.iter().any(|l| l.source.kind == SEQUENCE_KIND))
        });
        Self {
            source_open: shared.source.handle().is_some(),
            sequence,
            cut_selected: shared.edit.selected_cut().is_some(),
            snapping: shared.view.snapping(),
            shy: shy::ShyReady::of(shared),
        }
    }

    fn state(&self, spec: &Spec) -> State {
        use EditAction::{ClearInOut, Extract, Insert, Lift, MarkIn, MarkOut, Overwrite, Split};
        let enabled = match spec.owner {
            Owner::App(Action::Edit(ClearInOut | MarkIn | MarkOut)) => self.source_open,
            // シーケンスのレイヤーが無ければ作って入れるので、Source の範囲だけで押せる。
            Owner::App(Action::Edit(Insert | Overwrite)) => self.source_open,
            Owner::App(Action::Edit(Split)) => self.sequence,
            Owner::App(Action::Edit(Lift | Extract)) => self.cut_selected,
            Owner::App(Action::Shy(command)) => self.shy.enabled(command),
            Owner::App(_) | Owner::Panel(_) => true,
        };
        let checked = match spec.owner {
            Owner::App(Action::Shy(command)) => self.shy.checked(command),
            Owner::App(Action::Timeline(TimelineCommand::ToggleSnapping)) => Some(self.snapping),
            Owner::App(_) | Owner::Panel(_) => None,
        };
        State { enabled, checked }
    }
}

/// WebView に送る表の 1 行。`ui/sdk` の `UiCommand` と同じ形。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommandView {
    pub id: &'static str,
    pub name: &'static str,
    /// キーの表の名前（`Ctrl+KeyZ`）。先頭が画面に出すキー。
    pub keys: Vec<&'static str>,
    /// 先頭のキーの見た目（`Ctrl+Z`）。キーが無ければ `None`。
    pub key_label: Option<String>,
    pub description: Option<&'static str>,
    /// コマンドが入っているメニューバーのメニュー。
    pub menu: Option<Menu>,
    /// メニューの中の位置（グループをまたいだ通し番号）。メニューに無いコマンドは `None`。
    pub menu_order: Option<usize>,
    /// 持ち主の WebView のパネルの id。アプリが実行するコマンドは `None`。
    pub panel: Option<&'static str>,
    #[serde(flatten)]
    pub state: State,
}

fn view(spec: &'static Spec, state: State) -> CommandView {
    let keys = keys::keys_of(spec.id);
    CommandView {
        id: spec.id,
        name: spec.name,
        key_label: keys.first().map(|k| keys::label(k)),
        keys,
        description: spec.description,
        menu: menu::menu_of(spec.id),
        menu_order: menu::order_of(spec.id),
        panel: match spec.owner {
            Owner::Panel(panel) => Some(panel),
            Owner::App(_) => None,
        },
        state,
    }
}

/// 今の押せるかを付けた表。
pub fn table(shared: &Shared) -> Vec<CommandView> {
    let ready = Ready::of(shared);
    TABLE.iter().map(|s| view(s, ready.state(s))).collect()
}

/// 今は押せないコマンドの ID。サーフェスの再生の操作のバーがボタンを薄くするのに使う。
pub fn disabled_ids(shared: &Shared) -> Vec<&'static str> {
    let ready = Ready::of(shared);
    TABLE
        .iter()
        .filter(|s| !ready.state(s).enabled)
        .map(|s| s.id)
        .collect()
}

/// 押せるかが前に送った表から変わっていれば、新しい表を返す。同期のスレッドが周期ごとに呼び、イベントで送る。
pub fn changed(shared: &Shared, sent: &mut Option<Vec<State>>) -> Option<Vec<CommandView>> {
    let ready = Ready::of(shared);
    let states: Vec<State> = TABLE.iter().map(|s| ready.state(s)).collect();
    if sent.as_ref() == Some(&states) {
        return None;
    }
    let table = TABLE
        .iter()
        .zip(&states)
        .map(|(s, st)| view(s, *st))
        .collect();
    *sent = Some(states);
    Some(table)
}

fn spec_of(id: &str) -> Result<&'static Spec, String> {
    find(id).ok_or_else(|| format!("Unknown command: {id}"))
}

/// 押せるかを画面に出す呼び出し箇所（メニュー、Tasks の項目）からコマンドを実行する。押せないコマンドは拒否する。
pub fn run(shared: &Arc<Shared>, id: &str) -> Result<(), String> {
    let spec = spec_of(id)?;
    if !Ready::of(shared).state(spec).enabled {
        return Err(format!("{} isn't available now", spec.name));
    }
    execute(shared, spec)
}

/// キーからコマンドを実行する。押せるかは見ない。押せないときは編集が拒否した理由が画面に出て、何が足りないかが分かるため。
pub fn run_from_key(shared: &Arc<Shared>, id: &str) -> Result<(), String> {
    execute(shared, spec_of(id)?)
}

/// コマンドのキーを離した。押している間だけ効くコマンド（ドラッグ中のスナップの反転）に知らせる。
pub fn release_from_key(shared: &Shared, id: &str) {
    let snapping = Owner::App(Action::Timeline(TimelineCommand::ToggleSnapping));
    if find(id).is_some_and(|s| s.owner == snapping) {
        timeline_input(shared, PointerEvent::SnapKeyUp);
    }
}

/// Timeline のコマンドを、ポインターの入力のキュー（`Shared::input`）で描画スレッドへ渡す。
fn timeline_input(shared: &Shared, input: PointerEvent) {
    shared.input.lock().expect("input").push(input);
}

/// 編集と再生はここで実行し、Timeline のコマンドはポインターの入力のキューで描画スレッドへ、パネルとウィンドウの
/// コマンドはウィンドウのフックへ渡す。
fn execute(shared: &Arc<Shared>, spec: &'static Spec) -> Result<(), String> {
    match spec.owner {
        Owner::App(Action::Edit(action)) => edit_actions::run(shared, action),
        Owner::App(Action::Playback(command)) => play(shared, command),
        Owner::App(Action::Shy(command)) => shy::run(shared, command),
        Owner::App(Action::Timeline(command)) => {
            timeline_input(shared, PointerEvent::Command(command));
        }
        Owner::App(Action::Window(action)) => {
            return shared.window_hook.send(Forward::Window(action))
        }
        Owner::Panel(_) => return shared.window_hook.send(Forward::Panel(spec.id)),
    }
    Ok(())
}

/// Source を最後にクリックしていて素材を開いていれば Source の、そうでなければ Program の再生へ送る。
fn play(shared: &Shared, command: Command) {
    if shared.source.is_active() && shared.source.handle().is_some() {
        shared.source.send(command);
    } else {
        shared.send(command);
    }
}

mod shy;
mod window_hook;

pub use shy::ShyCommand;
pub use window_hook::{Forward, WindowHook};

#[cfg(test)]
mod tests;
