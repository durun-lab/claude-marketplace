use std::sync::Arc;

use mm_doc::{Layer, Node, Project};
use mm_types::{Id, RationalTime};
use mm_ui_layout::PointerEvent;
use windows::core::w;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP,
};

use super::*;
use crate::win::commands::{self, table, TABLE};
use crate::win::menu::{context_groups, lines};
use crate::win::sequence_edit::tests::sample_project;

/// 項目が表に無くても並びに置いてある、後の PR のコマンド（Track Height）。
const PENDING: &[&str] = &["timeline.track_height"];

const PLACES: [Place; 5] = [
    Place::Cut,
    Place::Layer,
    Place::Header,
    Place::Ruler,
    Place::Empty,
];

/// シーケンスのレイヤー `l-seq`（最背面）の上に文字のレイヤー `l-a` を積んだプロジェクトを開いた共有状態。
fn open_shared() -> Arc<Shared> {
    let mut p: Project = sample_project();
    let node = Node::new("text", 1);
    let secs = RationalTime::seconds;
    p.compositions[0]
        .layers
        .push(Layer::new("l-a", node, secs(0), secs(0), secs(5)));
    let shared = Arc::new(Shared::new());
    shared.publish_doc(Arc::new(p), Id::new("main"));
    shared
}

fn texts(shared: &Shared, place: Place) -> Vec<String> {
    describe(&lines(context_groups(place), &table(shared)))
}

#[test]
fn 並びの_id_は表にあるか_後の_pr_のコマンドで_グループは空でない() {
    for place in PLACES {
        for group in context_groups(place) {
            assert!(!group.is_empty(), "{place:?}");
            for id in *group {
                let known = TABLE.iter().any(|s| s.id == *id) || PENDING.contains(id);
                assert!(known, "{place:?}: {id}");
            }
        }
    }
}

#[test]
fn カットのメニューは_split_clip_here_lift_extract_を区切り線で分けて出す() {
    let shared = open_shared();
    shared.select_layer(Some("l-seq".to_owned()));
    shared.edit.select_cut(Some("k1".to_owned()));
    shared.context.set_time(1.5);
    assert_eq!(
        texts(&shared, Place::Cut),
        [
            "Split Clip Here",
            "-",
            "Lift  Delete",
            "Extract  Backspace",
            "-",
            "Show Keyframe Editor  U"
        ]
    );
}

#[test]
fn 右クリックの前は_split_clip_here_を押せず_カットを選ぶまで_lift_と_extract_を押せない() {
    let shared = open_shared();
    assert_eq!(
        texts(&shared, Place::Cut),
        [
            "Split Clip Here (disabled)",
            "-",
            "Lift  Delete (disabled)",
            "Extract  Backspace (disabled)",
            "-",
            "Show Keyframe Editor  U"
        ]
    );
}

#[test]
fn レイヤーのメニューは有効とロックと_shy_を上に_重なりをその下に出し_見出しも同じ並び() {
    let shared = open_shared();
    shared.select_layer(Some("l-a".to_owned()));
    let expected = [
        "Enable Layer (checked)",
        "Lock Layer",
        "Shy Selected Layers",
        "-",
        "Bring Forward (disabled)",
        "Send Backward",
        "-",
        "Show Keyframe Editor  U",
    ];
    assert_eq!(texts(&shared, Place::Layer), expected);
    assert_eq!(texts(&shared, Place::Header), expected);
}

#[test]
fn 目盛りと空きのメニュー() {
    let shared = open_shared();
    assert_eq!(texts(&shared, Place::Ruler), ["Full Extent Zoom  Shift+Z"]);
    assert_eq!(
        texts(&shared, Place::Empty),
        ["Full Extent Zoom  Shift+Z", "-", "Snapping  N (checked)"]
    );
}

#[test]
fn 選んだ項目はメニューバーと同じ道でコマンドとして実行する() {
    let shared = open_shared();
    menu::run(&shared, "timeline.toggle_snapping");
    menu::run(&shared, "timeline.full_extent_zoom");
    use mm_ui_layout::TimelineCommand::{FullExtentZoom, ToggleSnapping};
    assert_eq!(
        *shared.input.lock().unwrap(),
        [
            PointerEvent::Command(ToggleSnapping),
            PointerEvent::Command(FullExtentZoom)
        ]
    );
}

#[test]
fn 右クリックしていなければ_split_clip_here_を拒否する() {
    let shared = open_shared();
    assert_eq!(
        commands::run(&shared, "clip.split_here"),
        Err("Split Clip Here isn't available now".to_owned())
    );
    shared.context.set_time(1.0);
    assert!(commands::run(&shared, "clip.split_here").is_ok());
    shared.context.clear_time();
    assert!(commands::run(&shared, "clip.split_here").is_err());
}

#[test]
fn 描画スレッドの右クリックは時刻を置いてウィンドウのフックへ点を丸めて渡す() {
    let shared = open_shared();
    let cut = Hit::Cut {
        item: 0,
        cut: 1,
        part: mm_ui_layout::Part::Body,
    };
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    shared
        .window_hook
        .set(move |f| sink.lock().unwrap().push(f));
    on_action(&shared, cut, 2.5, (10.4, 20.6));
    assert_eq!(shared.context.time(), Some(2.5));
    assert_eq!(*seen.lock().unwrap(), [Forward::ContextMenu(cut, (10, 21))]);
}

#[test]
fn メニューキーは選んでいるものの位置_マウスは画面の座標をクライアントの座標にして入力にする() {
    assert_eq!(
        requested(HWND::default(), LPARAM(-1)),
        PointerEvent::ContextMenuKey
    );
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("ctx"),
            WS_POPUP,
            100,
            200,
            50,
            50,
            None,
            None,
            None,
            None,
        )
    }
    .expect("試験の HWND");
    let screen = LPARAM(((230 & 0xFFFF) << 16) | (120 & 0xFFFF));
    let ev = requested(window, screen);
    let _ = unsafe { DestroyWindow(window) };
    assert_eq!(ev, PointerEvent::ContextMenu { x: 20.0, y: 30.0 });
}
