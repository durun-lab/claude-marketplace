use super::*;

const WINDOW: f64 = 1600.0;

fn cols(left: f64, right: f64) -> Columns {
    Columns { left, right }
}

#[test]
fn 既定は_2_列とも開いて_幅は_theme_の左右の列の幅で_timeline_は_45_パーセント() {
    let w = Workspace::default();
    assert_eq!(w.columns(WINDOW), cols(260.0, 400.0));
    assert_eq!(w.columns(WINDOW).web(), theme::webview_width());
    assert!(w.left_open && w.right_open);
    assert_eq!(w.timeline_ratio, TIMELINE_RATIO);
}

#[test]
fn 縦の仕切りを左へ動かすと左の列が広がり_右の列は動かない() {
    let mut w = Workspace::default();
    w.drag_web(900.0, WINDOW);
    assert_eq!(w.columns(WINDOW), cols(500.0, 400.0));
}

#[test]
fn 縦の仕切りを右へ動かすと左の列が下限まで縮み_その先は右の列が縮む() {
    let mut w = Workspace::default();
    w.drag_web(600.0, WINDOW);
    assert_eq!(w.columns(WINDOW), cols(200.0, 400.0), "左が下限");
    w.drag_web(520.0, WINDOW);
    assert_eq!(w.columns(WINDOW), cols(200.0, 320.0), "右が縮む");
    w.drag_web(100.0, WINDOW);
    assert_eq!(w.columns(WINDOW), cols(200.0, 300.0), "両方が下限");
    w.drag_web(900.0, WINDOW);
    assert_eq!(w.columns(WINDOW), cols(500.0, 400.0), "戻すと右も広がる");
}

#[test]
fn 縦の仕切りを左へ動かしても_サーフェスの下限を割らない() {
    let mut w = Workspace::default();
    w.drag_web(5000.0, WINDOW);
    let web = w.columns(WINDOW).web();
    assert_eq!(WINDOW - web, MIN_SURFACE_W + f64::from(SPLITTER_W));
}

#[test]
fn ウィンドウの下限の幅でも_サーフェスは下限を割らず_既定の幅が入る() {
    let min_window = theme::webview_width() + MIN_SURFACE_W + f64::from(SPLITTER_W);
    let w = Workspace::default();
    assert_eq!(w.columns(min_window).web(), theme::webview_width());
    // 保存した幅より狭いウィンドウで開いたら、左の列から縮め、その先は右の列を縮める。
    let mut wide = Workspace::default();
    wide.drag_web(900.0, 2000.0);
    assert_eq!(wide.columns(1200.0), cols(396.0, 400.0));
    assert_eq!(wide.columns(1000.0), cols(200.0, 396.0));
}

#[test]
fn 列を閉じるとその分は_webview_から外れ_開いている列は自分の幅を保つ() {
    let mut w = Workspace {
        left_open: false,
        ..Workspace::default()
    };
    assert_eq!(w.columns(WINDOW), cols(0.0, 400.0));
    w.left_open = true;
    w.right_open = false;
    assert_eq!(w.columns(WINDOW), cols(260.0, 0.0));
    w.left_open = false;
    assert_eq!(w.columns(WINDOW), cols(0.0, 0.0));
    assert!(!w.has_web());
}

#[test]
fn 列が_1_つのときの仕切りは_その列の幅を動かし_2_列のときの幅は変えない() {
    let mut w = Workspace {
        left_open: false,
        ..Workspace::default()
    };
    w.drag_web(350.0, WINDOW);
    assert_eq!(w.columns(WINDOW), cols(0.0, 350.0));
    w.drag_web(900.0, WINDOW);
    assert_eq!(w.columns(WINDOW).right, 400.0, "右の列は上限まで");
    w.left_open = true;
    assert_eq!(w.columns(WINDOW), cols(260.0, 400.0));
    w.right_open = false;
    w.drag_web(380.0, WINDOW);
    assert_eq!(w.columns(WINDOW), cols(380.0, 0.0));
}

#[test]
fn 横の仕切りは_timeline_の高さを割合で保つ() {
    let mut w = Workspace::default();
    w.drag_timeline(400.0, 800.0);
    assert_eq!(w.timeline_ratio, 0.5);
    assert_eq!(
        mm_ui_layout::timeline_height(1000.0, w.timeline_ratio),
        500.0
    );
}

#[test]
fn 保存した割り付けを読み直すと同じになり_ファイルは版を持つ() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("movie-maker").join(FILE);
    let mut w = Workspace::default();
    w.drag_web(800.0, WINDOW);
    w.drag_timeline(450.0, 900.0);
    w.right_open = false;
    write(&path, &w).unwrap();
    assert_eq!(read(&path), Some(w));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"version\": 1"), "{text}");
}

#[test]
fn 無い_壊れた_知らない版の設定は既定で始める() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(FILE);
    assert_eq!(read(&path), None);
    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(read(&path), None);
    std::fs::write(&path, r#"{"version": 99, "left_open": false}"#).unwrap();
    assert_eq!(read(&path), None);
}

#[test]
fn 足りない項目は既定で埋め_壊れた数は既定に直す() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(FILE);
    std::fs::write(&path, r#"{"version": 1, "left_open": false, "both_w": -5}"#).unwrap();
    let w = read(&path).unwrap();
    assert!(!w.left_open && w.right_open);
    assert_eq!(w.both_w, Workspace::default().both_w);
}

#[test]
fn 設定の場所は環境変数が先で_次にローカルのアプリデータ() {
    let local = Some(PathBuf::from("C:/Users/u/AppData/Local"));
    assert_eq!(
        path_from(Some(PathBuf::from("x.json")), local.clone()),
        Some(PathBuf::from("x.json"))
    );
    assert_eq!(
        path_from(None, local),
        Some(PathBuf::from(
            "C:/Users/u/AppData/Local/movie-maker/workspace.json"
        ))
    );
    assert_eq!(path_from(None, None), None);
}

#[test]
fn 置き場所は更新を反映し_開いていなければ書かない() {
    let slot = WorkspaceSlot::default();
    slot.update(|w| w.left_open = false);
    assert!(!slot.get().left_open);
    // 設定のファイルを決めていない（`--play-test`）ので書かない。
    slot.save();
}

#[test]
fn 置き場所は開いた設定のファイルを読み_保存で書く() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(FILE);
    let mut saved = Workspace::default();
    saved.drag_web(700.0, WINDOW);
    write(&path, &saved).unwrap();
    let slot = WorkspaceSlot::default();
    slot.open(path.clone());
    assert_eq!(slot.get(), saved);
    slot.update(|w| w.right_open = false);
    slot.save();
    assert_eq!(read(&path).map(|w| w.right_open), Some(false));
}
