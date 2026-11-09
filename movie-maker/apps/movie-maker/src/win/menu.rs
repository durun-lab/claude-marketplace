//! メニューバー。メニューごとに、コマンドの ID の並びを区切りで分けたグループとして持ち、項目の名前、キーの見た目、
//! 押せるかは UI のコマンドの表から作る。VS Code の `menus` と KDE の ui.rc と同じく、コマンドの表とは別に並びだけを持つ。
//! 右クリックのメニューも、当たった所ごとに同じ形の並びを置けば `items` で作れる。

use std::sync::Arc;

use serde::Serialize;
use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu as NativeMenu, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::{App, AppHandle, Emitter, Manager, Window, Wry};

use crate::win::commands::{self, CommandView, Forward, State, WindowAction, PANEL_COMMAND_EVENT};
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

/// 区切りで分けたコマンドの ID のグループの並び。
pub type Groups = &'static [&'static [&'static str]];

/// メニューバーのメニューと中の並び。表に無い ID は飛ばすので、後の PR が表にコマンドを足すとメニューに出る。
pub static BAR: &[(Menu, Groups)] = &[
    (
        Menu::File,
        &[&["media.import", "export.start"], &["app.exit"]],
    ),
    (
        Menu::Edit,
        &[
            &["edit.undo", "edit.redo"],
            &["clip.split", "clip.lift", "clip.extract"],
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
            &["timeline.show_keyframe_editor", "timeline.hide_shy_layers"],
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
            &["playback.step_back", "playback.step_forward"],
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
        &[&[
            "timeline.selection_tool",
            "timeline.trim_tool",
            "timeline.blade_tool",
        ]],
    ),
    (
        Menu::Help,
        &[&["app.show_keyboard_shortcuts"], &["app.show_about"]],
    ),
];

/// メニューの 1 行。
#[derive(Debug, PartialEq)]
pub enum Line<'a> {
    Command(&'a CommandView),
    Separator,
}

/// グループの並びを、表にあるコマンドの行にする。空でないグループの間にだけ区切りを入れる（VS Code の menus の group と同じ）。
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

/// 項目の文字。キーがあれば `\t` の後ろにキーの見た目を置き、Windows のメニューが右のキーの列に出す。
/// キーはアクセラレーターに登録しない。Tauri はキーを受けたウィンドウに `WM_COMMAND` を送るので、サーフェスに
/// フォーカスがあるとメニューのイベントにならずにキーが消える。キーは今どおり `keys::dispatch` が受ける。
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

/// メニューの 1 行の Tauri の項目。押せるかとチェックの状態を後から置き直すため、コマンドの ID を持つ。
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

/// 押せるかとチェックの状態を置ける項目。
trait Reflect {
    /// 項目のコマンドの ID。区切りは `None`。
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

/// 表の押せるかとチェックの状態を、同じ ID の項目に置く。
fn apply<T: Reflect>(items: &[T], states: &[(&'static str, State)]) {
    for item in items {
        let Some(id) = item.command() else { continue };
        if let Some((_, state)) = states.iter().find(|(i, _)| *i == id) {
            item.set_state(*state);
        }
    }
}

/// グループの並びから Tauri のメニューの項目を作る。右クリックのメニューも同じ形の並びから作る。
pub fn items(app: &AppHandle, groups: Groups, table: &[CommandView]) -> tauri::Result<Vec<Item>> {
    lines(groups, table)
        .iter()
        .map(|line| Item::new(app, line))
        .collect()
}

/// メニューバーの項目。押せるかとチェックの状態を置き直すときに引く。
pub struct MenuBar(Vec<Item>);

/// メニューバーを作る。項目の押せるかとチェックの状態は `table` のもの。
pub fn build(app: &AppHandle, table: &[CommandView]) -> tauri::Result<(NativeMenu<Wry>, MenuBar)> {
    let bar = NativeMenu::new(app)?;
    let mut all = Vec::new();
    for (menu, groups) in BAR {
        let items = items(app, groups, table)?;
        let natives: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(Item::native).collect();
        // 項目のまだ無いメニューも、メニューバーの形を見せるため押せない見た目で出す。
        bar.append(&Submenu::with_items(
            app,
            menu.title(),
            !items.is_empty(),
            &natives,
        )?)?;
        all.extend(items);
    }
    Ok((bar, MenuBar(all)))
}

/// メニューのイベントでコマンドを実行し、パネルとウィンドウのコマンドを受けるウィンドウのフックを置く。
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
    });
}

/// 押せるかとチェックの状態をメニューバーに置く。同期のスレッドが表の変化を WebView へ送るときに呼ぶ。
/// 項目はメインスレッドで置き直し、同期のスレッドはそれを待たない。
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
