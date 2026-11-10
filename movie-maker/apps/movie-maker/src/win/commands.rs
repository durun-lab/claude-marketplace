//! UI のコマンドの表。呼び出し箇所（キー、メニュー、Tasks の項目）は、この表の ID でコマンドを指す。
//! 表は人が呼ぶコマンドだけを持ち、データの変更は実行した先の編集が操作（`Op`）で出す。

use std::sync::Arc;

use mm_doc::SEQUENCE_KIND;
use mm_ui_layout::{PointerEvent, TimelineCommand, Tool};
use serde::Serialize;

use crate::win::edit_actions::{self, EditAction};
use crate::win::keys;
use crate::win::menu::{self, Menu};
use crate::win::playback::Command;
use crate::win::state::Shared;
use crate::win::workspace::Workspace;

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
    /// 割り付けのコマンド（WebView の列の開け閉め、Reset UI Layout）。ユーザーの設定を変えるだけで、操作にならない。
    Layout(LayoutCommand),
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
    /// Keyboard Shortcuts のウィンドウを出す。開いていれば前面に出す。
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

// macro_rules! は書いた所より後でしか使えないので、表の mod（`table`）より前で読み込む。
#[macro_use]
mod show_panel;

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
    /// Timeline のキーフレームの行でキーフレームを選んでいる。Remove Keyframe はそのキーフレームを外す。
    keyframe_selected: bool,
    /// スナップの入/切（描画スレッドが置いた表示の状態）。
    snapping: bool,
    /// Timeline の今のツール（描画スレッドが置いたもの）。
    tool: Tool,
    /// Shy の行を隠すかどうか（表示の状態）と、選んでいるレイヤーの Shy のフラグ。
    shy: shy::ShyReady,
    /// 割り付け（WebView の列の開閉をチェックに出す）。
    workspace: Workspace,
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
            keyframe_selected: shared.keyframes.keyframe_selected(),
            snapping: shared.view.snapping(),
            tool: *shared.tool.lock().expect("tool"),
            shy: shy::ShyReady::of(shared),
            workspace: shared.workspace.get(),
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
            Owner::App(Action::Timeline(TimelineCommand::RemoveKeyframe)) => self.keyframe_selected,
            Owner::App(_) | Owner::Panel(_) => true,
        };
        let checked = match spec.owner {
            Owner::App(Action::Layout(command)) => layout::checked(&self.workspace, command),
            Owner::App(Action::Shy(command)) => self.shy.checked(command),
            Owner::App(Action::Timeline(TimelineCommand::ToggleSnapping)) => Some(self.snapping),
            Owner::App(Action::Timeline(command)) => tool_of(command).map(|tool| tool == self.tool),
            Owner::App(_) | Owner::Panel(_) => None,
        };
        State { enabled, checked }
    }
}

/// ツールを選ぶコマンドが選ぶツール。ツールのコマンドはチェックの状態（今のツール）を持つ。
fn tool_of(command: TimelineCommand) -> Option<Tool> {
    match command {
        TimelineCommand::SelectionTool => Some(Tool::Selection),
        TimelineCommand::TrimTool => Some(Tool::Trim),
        TimelineCommand::BladeTool => Some(Tool::Blade),
        _ => None,
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
    /// 全部のキーの見た目。並びは `keys` と同じで、Keyboard Shortcuts のウィンドウが全部を並べる。
    pub key_labels: Vec<String>,
    pub description: Option<&'static str>,
    /// コマンドが入っているメニューバーのメニュー（画面の名前で送る）。
    pub menu: Option<Menu>,
    /// メニューバーの中の位置（メニューとグループをまたいだ通し番号）。メニューに無いコマンドは `None`。
    pub menu_order: Option<usize>,
    /// 持ち主の WebView のパネルの id。アプリが実行するコマンドは `None`。
    pub panel: Option<&'static str>,
    #[serde(flatten)]
    pub state: State,
}

fn view(spec: &'static Spec, state: State) -> CommandView {
    let keys = keys::keys_of(spec.id);
    let place = menu::place_of(spec.id);
    CommandView {
        id: spec.id,
        name: spec.name,
        key_label: keys.first().map(|k| keys::label(k)),
        key_labels: keys.iter().map(|k| keys::label(k)).collect(),
        keys,
        description: spec.description,
        menu: place.map(|(menu, _)| menu),
        menu_order: place.map(|(_, order)| order),
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

/// 今押せるか。同じキーのコマンドから、今押せるものを選ぶのに使う（`keys::dispatch`）。
pub fn enabled(shared: &Shared, id: &str) -> bool {
    find(id).is_some_and(|spec| Ready::of(shared).state(spec).enabled)
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
        Owner::App(Action::Layout(command)) => layout::run(shared, command),
        Owner::App(Action::Shy(command)) => shy::run(shared, command),
        Owner::App(Action::Timeline(command)) => {
            timeline_input(shared, PointerEvent::Command(command));
        }
        Owner::App(Action::Window(action)) => {
            return shared.window_hook.send(Forward::Window(action))
        }
        Owner::Panel(panel) => {
            layout::reveal(shared, panel);
            return shared.window_hook.send(Forward::Panel(spec.id));
        }
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

mod layout;
mod shy;
mod table;
mod window_hook;

pub use layout::LayoutCommand;
pub use shy::ShyCommand;
pub use table::TABLE;
pub use window_hook::{Forward, WindowHook};

#[cfg(test)]
mod tests;
