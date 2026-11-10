use std::cell::RefCell;
use std::sync::mpsc::channel;
use std::sync::Mutex;

use mm_types::Id;

use super::*;
use crate::win::commands::{table, TABLE};
use crate::win::playback::Command;
use crate::win::sequence_edit::tests::sample_project;

/// メニューバーに出さないコマンド。Stop は Play / Stop と J/K/L のキーで足り、Choose Render Target… は
/// Deliver のパネルの中の操作なので Tasks の項目から呼ぶ。
const NOT_IN_MENUS: &[&str] = &["export.choose_target", "playback.stop"];

/// 項目と区切り線を、項目の文字と `-` にする。
fn texts(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| match line {
            Line::Command(c) => label(c),
            Line::Separator => "-".to_owned(),
        })
        .collect()
}

fn menu_texts(menu: Menu, table: &[CommandView]) -> Vec<String> {
    let (_, groups) = BAR.iter().find(|(m, _)| *m == menu).expect("メニュー");
    texts(&lines(groups, table))
}

#[test]
fn メニューバーは_file_edit_view_playback_timeline_help_の順() {
    let titles: Vec<&str> = BAR.iter().map(|(m, _)| m.title()).collect();
    assert_eq!(
        titles,
        ["&File", "&Edit", "&View", "&Playback", "&Timeline", "&Help"]
    );
}

#[test]
fn 項目は並びの順にコマンドの表の名前とキーの見た目で出る() {
    let table = table(&Shared::new());
    assert_eq!(
        menu_texts(Menu::File, &table),
        ["Import Media…\tCtrl+I", "Export…", "-", "Exit"]
    );
    assert_eq!(
        menu_texts(Menu::Edit, &table),
        [
            "Undo\tCtrl+Z",
            "Redo\tCtrl+Y",
            "-",
            "Lift\tDelete",
            "Extract\tBackspace",
            "-",
            "Mark In\tI",
            "Mark Out\tO",
            "Clear In and Out\tAlt+X",
            "-",
            "Insert\t,",
            "Overwrite\t.",
        ]
    );
    assert_eq!(
        menu_texts(Menu::Playback, &table),
        [
            "Play / Stop\tSpace",
            "Play Reverse\tJ",
            "Play Forward\tL",
            "-",
            "Step One Frame Back\t←",
            "Step One Frame Forward\t→",
            "Step One Second Back\tShift+←",
            "Step One Second Forward\tShift+→",
            "-",
            "Go to Start\tHome",
            "Go to End\tEnd",
            "Previous Edit\t↑",
            "Next Edit\t↓",
        ]
    );
    assert_eq!(
        menu_texts(Menu::Timeline, &table),
        ["Split Clip\tB", "Add Transition…"],
        "ツールは後の PR が表に足す"
    );
    assert_eq!(
        menu_texts(Menu::Help, &table),
        ["Keyboard Shortcuts…	?", "-", "About"]
    );
    assert_eq!(
        menu_texts(Menu::View, &table),
        [
            "Zoom In\t=",
            "Zoom Out\t-",
            "Full Extent Zoom\tShift+Z",
            "-",
            "Snapping\tN",
            "-",
            "Shy Selected Layers",
            "Hide Shy Layers"
        ]
    );
}

#[test]
fn 項目の無いメニューはメニューバーに出さない() {
    let full = table(&Shared::new());
    let menus: Vec<Menu> = shown(&full).into_iter().map(|(m, _)| m).collect();
    assert_eq!(menus.len(), BAR.len(), "どのメニューにも項目がある");
    let without_view: Vec<CommandView> = full
        .into_iter()
        .filter(|c| menu_of(c.id) != Some(Menu::View))
        .collect();
    let menus: Vec<Menu> = shown(&without_view).into_iter().map(|(m, _)| m).collect();
    assert!(!menus.contains(&Menu::View), "{menus:?}");
    assert!(shown(&without_view)
        .iter()
        .all(|(_, lines)| !lines.is_empty()));
}

#[test]
fn 表のコマンドはメニューバーに一度だけ出るか_not_in_menus_に入る() {
    let ids: Vec<&str> = BAR
        .iter()
        .flat_map(|(_, groups)| groups.iter().flat_map(|g| g.iter().copied()))
        .collect();
    for id in &ids {
        assert_eq!(ids.iter().filter(|i| *i == id).count(), 1, "{id}");
        let (category, verb) = id.split_once('.').expect(id);
        let well_formed =
            |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c == '_');
        assert!(
            well_formed(category) && well_formed(verb),
            "{id} は <分類>.<動詞>"
        );
    }
    for spec in TABLE {
        let placed = ids.contains(&spec.id);
        let kept_out = NOT_IN_MENUS.contains(&spec.id);
        assert!(
            placed != kept_out,
            "{} はメニューの並びか NOT_IN_MENUS のどちらか一方に置く",
            spec.id
        );
    }
}

#[test]
fn 表に無い_id_はスキップし_空のグループは区切り線ごと消える() {
    let table = table(&Shared::new());
    let groups: Groups = &[
        &["test.absent_one"],
        &["edit.undo", "test.absent_two"],
        &["test.absent_three"],
        &["app.exit"],
        &["test.absent_four"],
    ];
    assert_eq!(texts(&lines(groups, &table)), ["Undo\tCtrl+Z", "-", "Exit"]);
    assert!(lines(&[&["test.absent_one"]], &table).is_empty());
}

#[test]
fn キーの見た目はタブの後ろに置き_名前のアンパサンドは二重にする() {
    let table = table(&Shared::new());
    let row = |id: &str| table.iter().find(|c| c.id == id).expect(id);
    assert_eq!(label(row("edit.redo")), "Redo\tCtrl+Y", "先頭のキーだけ");
    assert_eq!(
        label(row("export.start")),
        "Export…",
        "キーが無ければ名前だけ"
    );
    let named = CommandView {
        name: "Fit & Fill",
        ..row("clip.split").clone()
    };
    assert_eq!(label(&named), "Fit && Fill\tB", "& はアクセスキーにしない");
}

#[test]
fn webview_に送る表のメニューはメニューの並びから引く() {
    assert_eq!(menu_of("clip.lift"), Some(Menu::Edit));
    assert_eq!(menu_of("clip.split"), Some(Menu::Timeline));
    assert_eq!(menu_of("app.exit"), Some(Menu::File));
    assert_eq!(menu_of("app.show_about"), Some(Menu::Help));
    assert_eq!(menu_of("playback.stop"), None);
    let json = serde_json::to_value(table(&Shared::new())).expect("JSON にする");
    let about = json
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["id"] == "app.show_about"))
        .expect("About");
    assert_eq!(about["menu"], "help");
}

/// 反映された押せるかとチェックの状態を記録する項目。
struct Fake {
    id: Option<&'static str>,
    got: RefCell<Vec<State>>,
}

impl Fake {
    fn new(id: Option<&'static str>) -> Self {
        Self {
            id,
            got: RefCell::new(Vec::new()),
        }
    }

    fn last(&self) -> Option<State> {
        self.got.borrow().last().copied()
    }
}

impl Reflect for Fake {
    fn command(&self) -> Option<&'static str> {
        self.id
    }

    fn set_state(&self, state: State) {
        self.got.borrow_mut().push(state);
    }
}

fn states(table: &[CommandView]) -> Vec<(&'static str, State)> {
    table.iter().map(|c| (c.id, c.state)).collect()
}

#[test]
fn 押せるかが変わった表を同じ_id_の項目に反映する() {
    let shared = Shared::new();
    let items = [
        Fake::new(Some("clip.split")),
        Fake::new(None),
        Fake::new(Some("playback.toggle")),
        Fake::new(Some("test.absent")),
    ];
    apply(&items, &states(&table(&shared)));
    let enabled = |i: usize| items[i].last().map(|s| s.enabled);
    assert_eq!(
        enabled(0),
        Some(false),
        "シーケンスが無ければ split は押せない"
    );
    assert_eq!(enabled(2), Some(true));
    assert!(items[1].got.borrow().is_empty(), "区切り線には反映しない");
    assert!(items[3].got.borrow().is_empty(), "表に無い ID は触らない");

    shared.publish_doc(Arc::new(sample_project()), Id::new("main"));
    apply(&items, &states(&table(&shared)));
    assert_eq!(enabled(0), Some(true), "シーケンスを開くと押せる");
}

#[test]
fn チェックの状態も項目に反映する() {
    let item = [Fake::new(Some("timeline.toggle_snapping"))];
    let on = State {
        enabled: true,
        checked: Some(true),
    };
    apply(&item, &[("timeline.toggle_snapping", on)]);
    assert_eq!(item[0].last(), Some(on));
}

/// ウィンドウのフックに渡ったコマンドを記録する。
fn record_forwards(shared: &Shared) -> Arc<Mutex<Vec<Forward>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    shared
        .window_hook
        .set(move |f| sink.lock().unwrap().push(f));
    seen
}

#[test]
fn メニューのイベントは表のコマンドを実行し_パネルとウィンドウのコマンドはフックへ渡す() {
    let shared = Arc::new(Shared::new());
    let (tx, rx) = channel();
    *shared.commands.lock().unwrap() = Some(tx);
    let seen = record_forwards(&shared);
    run(&shared, "playback.toggle");
    run(&shared, "playback.go_to_end");
    assert_eq!(rx.try_recv(), Ok(Command::Toggle));
    assert_eq!(rx.try_recv(), Ok(Command::Seek(f64::MAX)));
    for id in [
        "media.import",
        "export.start",
        "app.exit",
        "app.show_about",
        "app.show_keyboard_shortcuts",
    ] {
        run(&shared, id);
    }
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Forward::Panel("media.import"),
            Forward::Panel("export.start"),
            Forward::Window(WindowAction::Exit),
            Forward::Window(WindowAction::About),
            Forward::Window(WindowAction::ShowKeyboardShortcuts),
        ]
    );
}

#[test]
fn 押せない項目と表に無い_id_のイベントは何もしない() {
    let shared = Arc::new(Shared::new());
    let seen = record_forwards(&shared);
    run(&shared, "clip.split");
    run(&shared, "timeline.zoom_in");
    assert_eq!(shared.edit.message(), "", "編集を積まない");
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn メニューの中の位置はグループをまたいだ通し番号() {
    assert_eq!(order_of("timeline.zoom_in"), Some(0));
    assert_eq!(order_of("timeline.full_extent_zoom"), Some(2));
    assert_eq!(order_of("timeline.toggle_snapping"), Some(3));
    assert_eq!(order_of("playback.stop"), None, "メニューに無いコマンド");
}
