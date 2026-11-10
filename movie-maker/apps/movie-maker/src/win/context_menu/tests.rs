use std::sync::Arc;

use mm_doc::{Layer, Node, Project};
use mm_types::{Id, RationalTime};
use mm_ui_layout::PointerEvent;
use windows::core::w;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP,
};

use super::*;
use crate::win::commands::{self, context_table, TABLE};
use crate::win::keyframe_edit::RowSelection;
use crate::win::menu::{context_groups, lines};
use crate::win::sequence_edit::tests::sample_project;

/// 項目が表に無くても並びに置いてある、後の PR のコマンド（Track Height）。
const PENDING: &[&str] = &["timeline.track_height"];

const KINDS: [HitKind; 7] = [
    HitKind::Cut,
    HitKind::Layer,
    HitKind::Header,
    HitKind::KeyframeRow,
    HitKind::Keyframe,
    HitKind::Ruler,
    HitKind::Empty,
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

/// 割る時刻 `time` で出したメニューの項目。
fn texts(shared: &Shared, kind: HitKind, time: Option<f64>) -> Vec<String> {
    describe(&lines(context_groups(kind), &context_table(shared, time)))
}

#[test]
fn 並びの_id_は表にあるか_後の_pr_のコマンドで_グループは空でない() {
    for kind in KINDS {
        for group in context_groups(kind) {
            assert!(!group.is_empty(), "{kind:?}");
            for id in *group {
                let known = TABLE.iter().any(|s| s.id == *id) || PENDING.contains(id);
                assert!(known, "{kind:?}: {id}");
            }
        }
    }
}

#[test]
fn カットのメニューは_split_clip_here_lift_extract_show_keyframe_editor_を区切り線で分けて出す() {
    let shared = open_shared();
    shared.select_layer(Some("l-seq".to_owned()));
    shared.edit.select_cut(Some("k1".to_owned()));
    assert_eq!(
        texts(&shared, HitKind::Cut, Some(1.5)),
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
fn 割れない所のメニューでは_split_clip_here_を押せず_カットを選ぶまで_lift_と_extract_を押せない() {
    let shared = open_shared();
    assert_eq!(
        texts(&shared, HitKind::Cut, None),
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
        "Move Layer Up (disabled)",
        "Move Layer Down",
        "-",
        "Show Keyframe Editor  U",
    ];
    assert_eq!(texts(&shared, HitKind::Layer, None), expected);
    assert_eq!(texts(&shared, HitKind::Header, None), expected);
}

#[test]
fn ひし形のメニューは_remove_keyframe_と前後のキーフレームへの移動_行のメニューは移動と_show_keyframe_editor(
) {
    let shared = open_shared();
    shared.select_layer(Some("l-a".to_owned()));
    let steps = ["Go to Previous Keyframe  [", "Go to Next Keyframe  ]"];
    shared.keyframes.select(Some(RowSelection {
        layer: "l-a".to_owned(),
        path: None,
        keyframe: true,
    }));
    let keyframe = texts(&shared, HitKind::Keyframe, None);
    assert_eq!(keyframe[..2], ["Remove Keyframe  Delete", "-"]);
    assert_eq!(keyframe[2..], steps);
    let row = texts(&shared, HitKind::KeyframeRow, None);
    assert_eq!(row[..2], steps);
    assert_eq!(row[2..], ["-", "Show Keyframe Editor  U"]);
}

#[test]
fn 目盛りと行の無い所のメニュー() {
    let shared = open_shared();
    assert_eq!(
        texts(&shared, HitKind::Ruler, None),
        ["Full Extent Zoom  Shift+Z"]
    );
    assert_eq!(
        texts(&shared, HitKind::Empty, None),
        ["Full Extent Zoom  Shift+Z", "-", "Snapping  N (checked)"]
    );
}

#[test]
fn 項目の_id_はメニューバーと重ならず_選んだ項目はコマンドとして実行する() {
    let shared = open_shared();
    for command in ["timeline.toggle_snapping", "timeline.full_extent_zoom"] {
        let id = item_id(command);
        assert!(TABLE.iter().all(|s| s.id != id), "{id}");
        assert_eq!(command_of(&id), Some(command));
        menu::run(&shared, &id);
    }
    assert_eq!(command_of("timeline.full_extent_zoom"), None);
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
fn split_clip_here_は右クリックのメニューの項目からだけ押せ_選ばずに閉じた時刻をほかの呼び出し箇所が使わない(
) {
    let shared = open_shared();
    shared.select_layer(Some("l-seq".to_owned()));
    shared.context.set_time(Some(1.0));
    let refused = Err("Split Clip Here isn't available now".to_owned());
    assert_eq!(
        commands::run(&shared, "clip.split_here"),
        refused,
        "Tasks の項目"
    );
    let row = commands::table(&shared)
        .into_iter()
        .find(|c| c.id == "clip.split_here")
        .expect("表の行");
    assert!(
        !row.state.enabled,
        "WebView と Keyboard Shortcuts の表では押せない"
    );
    assert!(run(&shared, "clip.split_here").is_ok(), "メニューの項目");
    assert_eq!(shared.context.time(), None, "選んだ項目が時刻を使い切る");
    assert_eq!(run(&shared, "clip.split_here"), refused, "2 度目は割らない");
}

#[test]
fn 描画スレッドの右クリックは割る時刻を置いてウィンドウのフックへ点を丸めて渡す() {
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
    on_action(&shared, cut, Some(2.5), (10.4, 20.6));
    assert_eq!(shared.context.time(), Some(2.5));
    let ruler = Hit::Ruler;
    on_action(&shared, ruler, None, (30.0, 4.0));
    assert_eq!(
        shared.context.time(),
        None,
        "割れない所のメニューで置き直す"
    );
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Forward::ContextMenu(cut, (10, 21)),
            Forward::ContextMenu(ruler, (30, 4))
        ]
    );
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
