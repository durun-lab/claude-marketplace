//! UI のコマンドの表。呼び出し箇所（キー、メニュー、Tasks の項目）は、この表の ID でコマンドを指す。
//! 表は人が呼ぶコマンドだけを持ち、データの変更は実行した先の編集が操作（`Op`）で出す。

use std::sync::{Arc, OnceLock};

use mm_doc::SEQUENCE_KIND;
use mm_ui_layout::PointerEvent;
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
    /// Timeline の表示のコマンド。描画スレッドの Timeline の操作が実行する。
    Timeline(TimelineAction),
    /// Tauri のウィンドウが実行するコマンド。
    Window(WindowAction),
}

/// Timeline の表示のコマンドの中身。ポインターの入力のキューで描画スレッドへ渡し、ドラッグとの前後を保つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineAction {
    /// Shy の印の付いた行を Timeline から外す・戻す。表示の状態を変えるだけ。
    HideShyLayers,
    /// 選んだレイヤーの Shy の印を付ける・外す。表示の状態を変えるだけ。
    ShySelectedLayers,
    /// スナップの入/切。ドラッグ中のキーは、押している間だけ反転する。
    ToggleSnapping,
}

/// Tauri のウィンドウが実行するコマンドの中身。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    /// 閉じるボタンと同じく、描画と再生を止めてからウィンドウを閉じる。
    Exit,
    /// アプリの名前とバージョンを出す。
    About,
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
        id: "playback.step_back",
        name: "Step One Frame Back",
        description: None,
        owner: Owner::App(Action::Playback(Command::Step(-1))),
    },
    Spec {
        id: "playback.step_forward",
        name: "Step One Frame Forward",
        description: None,
        owner: Owner::App(Action::Playback(Command::Step(1))),
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
        id: "timeline.hide_shy_layers",
        name: "Hide Shy Layers",
        description: Some(
            "Hide the Shy layers from the Timeline; rendering and export don't change",
        ),
        owner: Owner::App(Action::Timeline(TimelineAction::HideShyLayers)),
    },
    Spec {
        id: "timeline.shy_selected_layers",
        name: "Shy Selected Layers",
        description: Some("Mark the selected layer Shy so Hide Shy Layers can hide its row"),
        owner: Owner::App(Action::Timeline(TimelineAction::ShySelectedLayers)),
    },
    Spec {
        id: "timeline.toggle_snapping",
        name: "Snapping",
        description: Some(
            "Snap dragged edges to the playhead, clip and layer edges, keyframes, and markers",
        ),
        owner: Owner::App(Action::Timeline(TimelineAction::ToggleSnapping)),
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
struct Ready {
    /// Source に素材を開いている。イン点・アウト点と insert、overwrite は Source の範囲を使う。
    source_open: bool,
    /// 開いているコンポジションにシーケンスのレイヤーがある。split、insert、overwrite はそのカットを編集する。
    sequence: bool,
    /// Timeline でカットを選んでいる。lift と extract は選んだカットを抜く。
    cut_selected: bool,
    /// スナップの入/切（描画スレッドが置いた表示の状態）。
    snapping: bool,
    /// Timeline で選んでいるレイヤーがあり、それに Shy の印が付いているか（選んでいなければ `None`）。
    selected_shy: Option<bool>,
    /// Shy の行を隠している。
    hide_shy: bool,
}

impl Ready {
    fn of(shared: &Shared) -> Self {
        let sequence = shared.doc().is_some_and(|doc| {
            doc.project
                .composition(&doc.comp)
                .is_ok_and(|c| c.layers.iter().any(|l| l.source.kind == SEQUENCE_KIND))
        });
        let shy = shared.view.shy();
        Self {
            hide_shy: shy.hide,
            selected_shy: shared
                .selected_layer()
                .map(|id| shy.layers.contains(id.as_str())),
            source_open: shared.source.handle().is_some(),
            sequence,
            cut_selected: shared.edit.selected_cut().is_some(),
            snapping: shared.view.snapping(),
        }
    }

    fn state(&self, spec: &Spec) -> State {
        use EditAction::{Extract, Insert, Lift, MarkIn, MarkOut, Overwrite, Split};
        let enabled = match spec.owner {
            Owner::App(Action::Edit(MarkIn | MarkOut)) => self.source_open,
            Owner::App(Action::Edit(Insert | Overwrite)) => self.source_open && self.sequence,
            Owner::App(Action::Edit(Split)) => self.sequence,
            Owner::App(Action::Edit(Lift | Extract)) => self.cut_selected,
            Owner::App(Action::Timeline(TimelineAction::ShySelectedLayers)) => {
                self.selected_shy.is_some()
            }
            Owner::App(_) | Owner::Panel(_) => true,
        };
        let checked = match spec.owner {
            Owner::App(Action::Timeline(TimelineAction::HideShyLayers)) => Some(self.hide_shy),
            Owner::App(Action::Timeline(TimelineAction::ShySelectedLayers)) => self.selected_shy,
            Owner::App(Action::Timeline(TimelineAction::ToggleSnapping)) => Some(self.snapping),
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
    let snapping = Owner::App(Action::Timeline(TimelineAction::ToggleSnapping));
    if find(id).is_some_and(|s| s.owner == snapping) {
        timeline_input(shared, PointerEvent::SnapKeyUp);
    }
}

/// Timeline の表示のコマンドを、ポインターの入力のキュー（`Shared::input`）で描画スレッドへ渡す。
fn timeline_input(shared: &Shared, input: PointerEvent) {
    shared.input.lock().expect("input").push(input);
}

/// 編集と再生と Timeline の表示はここで実行し、パネルとウィンドウのコマンドはウィンドウのフックへ渡す。
fn execute(shared: &Arc<Shared>, spec: &'static Spec) -> Result<(), String> {
    match spec.owner {
        Owner::App(Action::Edit(action)) => edit_actions::run(shared, action),
        Owner::App(Action::Playback(command)) => play(shared, command),
        Owner::App(Action::Timeline(TimelineAction::HideShyLayers)) => {
            shared.view.toggle_hide_shy();
        }
        Owner::App(Action::Timeline(TimelineAction::ShySelectedLayers)) => {
            if let Some(id) = shared.selected_layer() {
                shared.view.toggle_shy(&id);
            }
        }
        Owner::App(Action::Timeline(TimelineAction::ToggleSnapping)) => {
            timeline_input(shared, PointerEvent::ToggleSnapping);
        }
        Owner::App(Action::Window(action)) => {
            return shared.window_hook.send(Forward::Window(action))
        }
        Owner::Panel(_) => return shared.window_hook.send(Forward::Panel(spec.id)),
    }
    Ok(())
}

/// ウィンドウのフックへ渡すもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forward {
    /// パネルが持ち主のコマンドの ID。WebView の Shell が持ち主のパネルのタブを開き、パネルの `requests` を進める。
    Panel(&'static str),
    Window(WindowAction),
    /// WebView で押した、キーの表に無い Alt+英字の字（小文字）。ウィンドウがその字をアクセスキーとしてメニューを開く。
    MenuKey(char),
}

/// ウィンドウのフックの中身。
type ForwardFn = Box<dyn Fn(Forward) + Send + Sync>;

/// パネルとウィンドウのコマンドを、Tauri の `AppHandle` と `Window` で実行するフック。メニューバーと同じく、
/// ウィンドウを作るときに設定する。
#[derive(Default)]
pub struct WindowHook(OnceLock<ForwardFn>);

impl WindowHook {
    /// 渡す先を設定する。2 回目からは無視する。
    pub fn set(&self, forward: impl Fn(Forward) + Send + Sync + 'static) {
        let _ = self.0.set(Box::new(forward));
    }

    /// 設定した先へ渡す。ウィンドウを作る前は渡す先が無いので拒否する。
    pub fn send(&self, forward: Forward) -> Result<(), String> {
        let f = self
            .0
            .get()
            .ok_or_else(|| "The window isn't ready yet".to_owned())?;
        f(forward);
        Ok(())
    }
}

/// Source を最後にクリックしていて素材を開いていれば Source の、そうでなければ Program の再生へ送る。
fn play(shared: &Shared, command: Command) {
    if shared.source.is_active() && shared.source.handle().is_some() {
        shared.source.send(command);
    } else {
        shared.send(command);
    }
}

#[cfg(test)]
mod tests;
