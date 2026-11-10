//! メニューバー。メニューの中の並びを、区切り線で分けたコマンドの ID のグループとして表とは別に持ち（VS Code の `menus`、
//! KDE の ui.rc と同じ）、項目の名前、キーの見た目、押せるかは UI のコマンドの表から作る。右クリックのメニューも同じ形。

use std::sync::Arc;

use serde::Serialize;
use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu as NativeMenu, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::{App, AppHandle, Emitter, Manager, Window, Wry};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SC_KEYMENU, WM_SYSCOMMAND};

use crate::win::commands::{self, CommandView, Forward, State, WindowAction, PANEL_COMMAND_EVENT};
use crate::win::layout;
use crate::win::state::Shared;

/// メニューバーのメニュー。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
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

/// コマンドが入っている最初のメニュー。
pub fn menu_of(id: &str) -> Option<Menu> {
    BAR.iter()
        .find(|(_, groups)| groups.iter().any(|g| g.contains(&id)))
        .map(|(menu, _)| *menu)
}

/// コマンドのメニューの中の位置（グループをまたいだ通し番号）。Keyboard Shortcuts のウィンドウがメニューと同じ順に並べる。
pub fn order_of(id: &str) -> Option<usize> {
    BAR.iter()
        .find(|(_, groups)| groups.iter().any(|g| g.contains(&id)))
        .and_then(|(_, groups)| groups.iter().flat_map(|g| g.iter()).position(|i| *i == id))
}

/// メニューの項目か区切り線の、Tauri の項目。押せるかとチェックの状態を後から反映するため、コマンドの ID を持つ。
pub enum Item {
    Separator(PredefinedMenuItem<Wry>),
    Plain(&'static str, MenuItem<Wry>),
    Check(&'static str, CheckMenuItem<Wry>),
}

impl Item {
    fn new(app: &AppHandle, line: &Line) -> tauri::Result<Self> {
        let Line::Command(c) = line else {
            return Ok(Item::Separator(PredefinedMenuItem::separator(app)?));
        };
        let (id, text, state) = (c.id, label(c), c.state);
        Ok(match state.checked {
            Some(on) => Item::Check(
                id,
                CheckMenuItem::with_id(app, id, text, state.enabled, on, None::<&str>)?,
            ),
            None => Item::Plain(
                id,
                MenuItem::with_id(app, id, text, state.enabled, None::<&str>)?,
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

/// 項目と区切り線の並びから Tauri のメニューの項目を作る。右クリックのメニューも同じ形の並びから作る。
pub fn items(app: &AppHandle, lines: &[Line]) -> tauri::Result<Vec<Item>> {
    lines.iter().map(|line| Item::new(app, line)).collect()
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
    shared.window_hook.set(move |forward| match forward {
        Forward::Panel(id) => {
            let _ = handle.emit(PANEL_COMMAND_EVENT, id);
        }
        Forward::Window(WindowAction::Exit) => {
            let _ = target.close();
        }
        Forward::Window(WindowAction::About) => about(&target),
        Forward::Window(WindowAction::ShowKeyboardShortcuts) => {
            layout::open_shortcuts(&handle, &target);
        }
        Forward::MenuKey(c) => open_by_access_key(&target, c),
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

/// メニューの項目のイベントからコマンドを実行する。項目の ID はコマンドの ID。
pub fn run(shared: &Arc<Shared>, id: &str) {
    let _span = tracing::info_span!("window.menu").entered();
    if let Err(e) = commands::run(shared, id) {
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
