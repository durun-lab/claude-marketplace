//! メニューバー。メニューの中の並びを、区切り線で分けたコマンドの ID のグループとして表とは別に持ち（VS Code の `menus`、
//! KDE の ui.rc と同じ）、項目の名前、キーの見た目、押せるかは UI のコマンドの表から作る。右クリックのメニューも同じ形。

use std::sync::Arc;

use mm_ui_layout::HitKind;
use serde::{Serialize, Serializer};
use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu as NativeMenu, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::{App, AppHandle, Emitter, Manager, Window, Wry};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SC_KEYMENU, WM_SYSCOMMAND};

use crate::win::commands::{self, CommandView, Forward, State, WindowAction, PANEL_COMMAND_EVENT};
use crate::win::context_menu;
use crate::win::shortcuts_window;
use crate::win::state::Shared;

/// メニューバーのメニュー。WebView へは画面の名前（`Edit`）で送り、Keyboard Shortcuts のウィンドウが区画の見出しに使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    File,
    Edit,
    View,
    Playback,
    Timeline,
    Help,
}

impl Menu {
    /// メニューバーに出す名前。`&` の次の字がアクセスキー（Alt+F、F10 の後の F）。
    fn title(self) -> &'static str {
        match self {
            Menu::File => "&File",
            Menu::Edit => "&Edit",
            Menu::View => "&View",
            Menu::Playback => "&Playback",
            Menu::Timeline => "&Timeline",
            Menu::Help => "&Help",
        }
    }

    /// 画面の名前（`title` からアクセスキーを示す `&` を除いたもの）。
    pub fn name(self) -> String {
        self.title().replace('&', "")
    }
}

impl Serialize for Menu {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

/// 区切り線で分けたコマンドの ID のグループの並び。
pub type Groups = &'static [&'static [&'static str]];

/// メニューバーのメニューと中の並び。表に無い ID はスキップするので、後の PR が表にコマンドを足すとメニューに出る。
pub static BAR: &[(Menu, Groups)] = &[
    (
        Menu::File,
        &[&["media.import", "export.start"], &["app.exit"]],
    ),
    (
        Menu::Edit,
        &[
            &["edit.undo", "edit.redo"],
            &["clip.lift", "clip.extract"],
            &["source.mark_in", "source.mark_out", "source.clear_in_out"],
            &["clip.insert", "clip.overwrite"],
        ],
    ),
    (
        Menu::View,
        &[
            &[
                "timeline.zoom_in",
                "timeline.zoom_out",
                "timeline.full_extent_zoom",
            ],
            &["timeline.toggle_snapping"],
            &[
                "timeline.show_keyframe_editor",
                "timeline.shy_selected_layers",
                "timeline.hide_shy_layers",
            ],
            &[
                "view.show_tasks",
                "view.show_media",
                "view.show_story",
                "view.show_transition",
                "view.show_mixer",
                "view.show_deliver",
            ],
            &[
                "view.show_inspector",
                "view.show_review",
                "view.show_devices",
                "view.show_feedback",
            ],
            &["view.show_diagnostics"],
            &["view.toggle_left_column", "view.toggle_right_column"],
            &["view.reset_ui_layout"],
        ],
    ),
    (
        Menu::Playback,
        &[
            &[
                "playback.toggle",
                "playback.play_reverse",
                "playback.play_forward",
            ],
            &[
                "playback.step_back",
                "playback.step_forward",
                "playback.step_back_second",
                "playback.step_forward_second",
            ],
            &[
                "playback.go_to_start",
                "playback.go_to_end",
                "playback.previous_edit",
                "playback.next_edit",
            ],
            &[
                "playback.go_to_previous_keyframe",
                "playback.go_to_next_keyframe",
            ],
        ],
    ),
    (
        Menu::Timeline,
        &[
            &[
                "timeline.selection_tool",
                "timeline.trim_tool",
                "timeline.blade_tool",
            ],
            &["clip.split", "clip.add_transition"],
        ],
    ),
    (
        Menu::Help,
        &[&["app.show_keyboard_shortcuts"], &["app.show_about"]],
    ),
];

/// サーフェスの右クリックのメニューの並び。当たった所の種類ごとに、メニューバーと同じ形（区切り線で分けたコマンドの ID の
/// グループ）で持つ。表に無い ID はスキップするので、後の PR が表にコマンドを足すと出る（Track Height）。
pub fn context_groups(kind: HitKind) -> Groups {
    const KEYFRAME_STEPS: &[&str] = &[
        "playback.go_to_previous_keyframe",
        "playback.go_to_next_keyframe",
    ];
    match kind {
        HitKind::Cut => &[
            &["clip.split_here"],
            &["clip.lift", "clip.extract"],
            &["timeline.show_keyframe_editor"],
        ],
        HitKind::Layer | HitKind::Header => &[
            &[
                "layer.toggle_enabled",
                "layer.toggle_lock",
                "timeline.shy_selected_layers",
            ],
            &["layer.move_up", "layer.move_down"],
            &["timeline.track_height", "timeline.show_keyframe_editor"],
        ],
        HitKind::KeyframeRow => &[KEYFRAME_STEPS, &["timeline.show_keyframe_editor"]],
        HitKind::Keyframe => &[&["timeline.remove_keyframe"], KEYFRAME_STEPS],
        HitKind::Ruler => &[&["timeline.full_extent_zoom"]],
        HitKind::Empty => &[
            &["timeline.full_extent_zoom"],
            &["timeline.toggle_snapping"],
        ],
    }
}

/// メニューの項目か区切り線の 1 つ。
#[derive(Debug, PartialEq)]
pub enum Line<'a> {
    Command(&'a CommandView),
    Separator,
}

/// グループの並びを、表にあるコマンドの項目と区切り線の並びにする。空でないグループの間にだけ区切り線を入れる
/// （VS Code の menus の group と同じ）。
pub fn lines<'a>(groups: Groups, table: &'a [CommandView]) -> Vec<Line<'a>> {
    let mut out = Vec::new();
    for group in groups {
        let found: Vec<&CommandView> = group
            .iter()
            .filter_map(|id| table.iter().find(|c| c.id == *id))
            .collect();
        if found.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(Line::Separator);
        }
        out.extend(found.into_iter().map(Line::Command));
    }
    out
}

/// 項目の文字。キーの見た目は `\t` の後ろに書き、Windows のメニューが右の列に出す。アクセラレーターに登録すると Tauri が
/// キーを受けたウィンドウ（サーフェス）に `WM_COMMAND` を送り、キーが消えるので、キーは `keys::dispatch` が受ける。
pub fn label(command: &CommandView) -> String {
    let name = command.name.replace('&', "&&");
    match &command.key_label {
        Some(key) => format!("{name}\t{key}"),
        None => name,
    }
}

/// コマンドが入っている最初のメニューと、メニューバーの中の位置（メニューとグループをまたいだ通し番号）。
/// Keyboard Shortcuts のウィンドウは、この位置の順に並べてメニューごとの区画にする。
pub fn place_of(id: &str) -> Option<(Menu, usize)> {
    BAR.iter()
        .flat_map(|(menu, groups)| {
            groups
                .iter()
                .flat_map(|g| g.iter())
                .map(move |i| (*menu, *i))
        })
        .enumerate()
        .find(|(_, (_, i))| *i == id)
        .map(|(order, (menu, _))| (menu, order))
}

/// メニューの項目か区切り線の、Tauri の項目。押せるかとチェックの状態を後から反映するため、コマンドの ID を持つ。
pub enum Item {
    Separator(PredefinedMenuItem<Wry>),
    Plain(&'static str, MenuItem<Wry>),
    Check(&'static str, CheckMenuItem<Wry>),
}

impl Item {
    /// 項目の ID は、メニューバーではコマンドの ID、右クリックのメニューでは `context_menu::item_id` のもの。
    fn new(app: &AppHandle, line: &Line, context: bool) -> tauri::Result<Self> {
        let Line::Command(c) = line else {
            return Ok(Item::Separator(PredefinedMenuItem::separator(app)?));
        };
        let (id, text, state) = (c.id, label(c), c.state);
        let native = if context {
            context_menu::item_id(id)
        } else {
            id.to_owned()
        };
        Ok(match state.checked {
            Some(on) => Item::Check(
                id,
                CheckMenuItem::with_id(app, native, text, state.enabled, on, None::<&str>)?,
            ),
            None => Item::Plain(
                id,
                MenuItem::with_id(app, native, text, state.enabled, None::<&str>)?,
            ),
        })
    }

    fn native(&self) -> &dyn IsMenuItem<Wry> {
        match self {
            Item::Separator(s) => s,
            Item::Plain(_, i) => i,
            Item::Check(_, i) => i,
        }
    }
}

/// 押せるかとチェックの状態を反映できる項目。
trait Reflect {
    /// 項目のコマンドの ID。区切り線は `None`。
    fn command(&self) -> Option<&'static str>;
    fn set_state(&self, state: State);
}

impl Reflect for Item {
    fn command(&self) -> Option<&'static str> {
        match self {
            Item::Separator(_) => None,
            Item::Plain(id, _) | Item::Check(id, _) => Some(id),
        }
    }

    fn set_state(&self, state: State) {
        match self {
            Item::Separator(_) => {}
            Item::Plain(_, i) => {
                let _ = i.set_enabled(state.enabled);
            }
            Item::Check(_, i) => {
                let _ = i.set_enabled(state.enabled);
                if let Some(on) = state.checked {
                    let _ = i.set_checked(on);
                }
            }
        }
    }
}

/// 表の押せるかとチェックの状態を、同じ ID の項目に反映する。
fn apply<T: Reflect>(items: &[T], states: &[(&'static str, State)]) {
    for item in items {
        let Some(id) = item.command() else { continue };
        if let Some((_, state)) = states.iter().find(|(i, _)| *i == id) {
            item.set_state(*state);
        }
    }
}

/// 項目と区切り線の並びからメニューバーの項目を作る。右クリックのメニューも同じ形の並びから作る（`context_menu`）。
pub fn items(app: &AppHandle, lines: &[Line]) -> tauri::Result<Vec<Item>> {
    lines
        .iter()
        .map(|line| Item::new(app, line, false))
        .collect()
}

/// 項目と区切り線の並びから右クリックのメニューを作る。出すたびに作り直すので、押せるかとチェックの状態は作るときの表のもの。
/// 項目の ID はメニューバーと分け、選んだ項目を `context_menu::run` が右クリックの時刻で実行する。
pub fn context_menu(app: &AppHandle, lines: &[Line]) -> tauri::Result<NativeMenu<Wry>> {
    let items = lines
        .iter()
        .map(|line| Item::new(app, line, true))
        .collect::<tauri::Result<Vec<_>>>()?;
    let natives: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(Item::native).collect();
    NativeMenu::with_items(app, &natives)
}

/// メニューバーに出すメニューと、その項目と区切り線。項目の無いメニューは開いても何もできないので出さず、
/// 後の PR が表にコマンドを足すと出る。
pub fn shown(table: &[CommandView]) -> Vec<(Menu, Vec<Line<'_>>)> {
    BAR.iter()
        .map(|(menu, groups)| (*menu, lines(groups, table)))
        .filter(|(_, lines)| !lines.is_empty())
        .collect()
}

/// メニューバーの項目。押せるかとチェックの状態を反映するときに引く。
pub struct MenuBar(Vec<Item>);

/// メニューバーを作る。項目の押せるかとチェックの状態は `table` のもの。
pub fn build(app: &AppHandle, table: &[CommandView]) -> tauri::Result<(NativeMenu<Wry>, MenuBar)> {
    let bar = NativeMenu::new(app)?;
    let mut all = Vec::new();
    for (menu, lines) in shown(table) {
        let items = items(app, &lines)?;
        let natives: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(Item::native).collect();
        bar.append(&Submenu::with_items(app, menu.title(), true, &natives)?)?;
        all.extend(items);
    }
    Ok((bar, MenuBar(all)))
}

/// メニューのイベントでコマンドを実行し、パネルとウィンドウのコマンドと WebView のアクセスキーを受けるウィンドウのフックを設定する。
pub fn connect(app: &App, window: &Window, shared: &Arc<Shared>, bar: MenuBar) {
    app.manage(bar);
    let on_menu = shared.clone();
    window.on_menu_event(move |_, event| run(&on_menu, event.id().as_ref()));
    let handle = app.handle().clone();
    let target = window.clone();
    let for_context = shared.clone();
    shared.window_hook.set(move |forward| match forward {
        Forward::Panel(id) => {
            let _ = handle.emit(PANEL_COMMAND_EVENT, id);
        }
        Forward::Window(WindowAction::Exit) => {
            let _ = target.close();
        }
        Forward::Window(WindowAction::About) => about(&target),
        Forward::Window(WindowAction::ShowKeyboardShortcuts) => {
            shortcuts_window::open(&handle, &target);
        }
        Forward::MenuKey(c) => open_by_access_key(&target, c),
        Forward::ContextMenu(hit, at) => {
            context_menu::show(&handle, &target, &for_context, hit, at);
        }
    });
}

/// Alt+英字と同じく、既定のウィンドウプロシージャにその字をアクセスキーとしてメニューを開かせる（合う字が無ければ警告音）。
fn open_by_access_key(window: &Window, c: char) {
    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = HWND(hwnd.0 as isize as *mut _);
    let _ = unsafe {
        PostMessageW(
            Some(hwnd),
            WM_SYSCOMMAND,
            WPARAM(SC_KEYMENU as usize),
            LPARAM(c as isize),
        )
    };
}

/// 押せるかとチェックの状態をメニューバーに反映する。同期のスレッドが表の変化を WebView へ送るときに呼ぶ。
/// 項目にはメインスレッドで反映し、同期のスレッドはそれを待たない。
pub fn reflect(app: &AppHandle, table: &[CommandView]) {
    let states: Vec<(&'static str, State)> = table.iter().map(|c| (c.id, c.state)).collect();
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(bar) = handle.try_state::<MenuBar>() {
            apply(&bar.0, &states);
        }
    });
}

/// メニューの項目のイベントからコマンドを実行する。メニューバーの項目の ID はコマンドの ID、右クリックのメニューの項目は
/// `context_menu::item_id` の ID で、メニューを出したときの右クリックの時刻で実行する。
pub fn run(shared: &Arc<Shared>, id: &str) {
    let _span = tracing::info_span!("window.menu").entered();
    let result = match context_menu::command_of(id) {
        Some(command) => context_menu::run(shared, command),
        None => commands::run(shared, id),
    };
    if let Err(e) = result {
        tracing::warn!(id, error = %e, "メニューのコマンドを実行できない");
    }
}

/// アプリの名前とバージョンを出す。閉じるまで返らないので、メインスレッドを止めないよう別のスレッドで出す。
fn about(window: &Window) {
    let parent = window.clone();
    let spawned = std::thread::Builder::new()
        .name("about".into())
        .spawn(move || {
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Info)
                .set_title("About movie-maker")
                .set_description(format!("movie-maker {}", env!("CARGO_PKG_VERSION")))
                .set_buttons(rfd::MessageButtons::Ok)
                .set_parent(&parent)
                .show();
        });
    if let Err(e) = spawned {
        tracing::warn!("About のダイアログを出せない: {e}");
    }
}

#[cfg(test)]
mod tests;
