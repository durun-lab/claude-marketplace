use super::*;
use mm_types::Id;

fn state(px: f64, scroll: f32) -> ViewState {
    let mut layers = BTreeMap::new();
    layers.insert("title".to_owned(), RowHeight::Small);
    ViewState {
        time: TimeView {
            origin: -0.5,
            px_per_sec: px,
        },
        scroll_y: scroll,
        heights: RowHeights {
            all: Some(RowHeight::Medium),
            layers,
        },
        snapping: true,
    }
}

/// `project.json` のあるフォルダーを作り、そのプロジェクトのパスを返す。
fn json_project(dir: &Path) -> PathBuf {
    let project = dir.join("project.json");
    std::fs::write(&project, "{}").unwrap();
    project
}

fn opened(project: &Path, comp: &str) -> Arc<Shared> {
    let shared = Arc::new(Shared::new());
    open(&shared, project, &Id::new(comp), false);
    shared
}

#[test]
fn 書いた表示の状態を開き直すと同じ値で戻る() {
    let tmp = tempfile::tempdir().unwrap();
    let project = json_project(tmp.path());
    let shared = opened(&project, "main");
    assert_eq!(shared.view.current(), None, "ファイルが無ければ既定");
    shared.view.publish(state(42.0, 280.0));
    shared.view.flush();
    let path = tmp.path().join(".mm").join("view.json");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"pxPerSec\": 42.0"), "{text}");
    assert!(text.contains("\"trackHeight\": \"medium\""), "{text}");
    let again = opened(&project, "main");
    assert_eq!(again.view.current(), Some(state(42.0, 280.0)));
}

#[test]
fn スナップの入_切を書いて戻し_書いていないファイルは入() {
    let tmp = tempfile::tempdir().unwrap();
    let project = json_project(tmp.path());
    let shared = opened(&project, "main");
    assert!(shared.view.snapping(), "表示の状態が無ければ入");
    let off = ViewState {
        snapping: false,
        ..state(42.0, 0.0)
    };
    shared.view.publish(off.clone());
    shared.view.flush();
    let path = tmp.path().join(".mm").join("view.json");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"snapping\": false"), "{text}");
    let again = opened(&project, "main");
    assert_eq!(again.view.current(), Some(off));
    assert!(!again.view.snapping());
    let older =
        r#"{"version": 1, "timelines": {"main": {"origin": 0, "pxPerSec": 50, "scrollY": 0}}}"#;
    std::fs::write(&path, older).unwrap();
    assert!(
        opened(&project, "main").view.snapping(),
        "入/切の無いファイルは入"
    );
}

#[test]
fn 読むだけで開いたときは表示の状態を戻すが書かない() {
    let tmp = tempfile::tempdir().unwrap();
    let project = json_project(tmp.path());
    let writer = opened(&project, "main");
    writer.view.publish(state(42.0, 280.0));
    writer.view.flush();
    let path = tmp.path().join(".mm").join("view.json");
    let before = std::fs::read_to_string(&path).unwrap();
    let reader = Arc::new(Shared::new());
    open(&reader, &project, &Id::new("main"), true);
    assert_eq!(reader.view.current(), Some(state(42.0, 280.0)), "戻す");
    reader.view.publish(state(10.0, 0.0));
    reader.view.flush();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before, "書かない");
}

#[test]
fn 長いコンポジションの全体表示の倍率は_min_px_per_sec_より細かくても戻す() {
    let tmp = tempfile::tempdir().unwrap();
    let project = json_project(tmp.path());
    let shared = opened(&project, "main");
    shared.view.publish(state(1.3, 0.0));
    shared.view.flush();
    let again = opened(&project, "main").view.current().expect("読める");
    assert_eq!(again.time.px_per_sec, 1.3);
}

#[test]
fn ほかのコンポジションの表示の状態は残す() {
    let tmp = tempfile::tempdir().unwrap();
    let project = json_project(tmp.path());
    let a = opened(&project, "a");
    a.view.publish(state(10.0, 0.0));
    a.view.flush();
    let b = opened(&project, "b");
    b.view.publish(state(20.0, 5.0));
    b.view.flush();
    assert_eq!(opened(&project, "a").view.current(), Some(state(10.0, 0.0)));
    assert_eq!(opened(&project, "b").view.current(), Some(state(20.0, 5.0)));
}

#[test]
fn 同じ状態を置き直しても書かない() {
    let shared = Shared::new();
    shared.view.publish(state(10.0, 0.0));
    shared.view.inner.lock().unwrap().dirty = false;
    shared.view.publish(state(10.0, 0.0));
    assert!(!shared.view.inner.lock().unwrap().dirty);
    shared.view.publish(state(10.0, 1.0));
    assert!(shared.view.inner.lock().unwrap().dirty);
}

#[test]
fn 読めないファイルと知らないバージョンは既定に戻し_書き換えた値は範囲に収める() {
    let tmp = tempfile::tempdir().unwrap();
    let project = json_project(tmp.path());
    let path = tmp.path().join(".mm").join("view.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not json").unwrap();
    assert_eq!(opened(&project, "main").view.current(), None);
    std::fs::write(&path, r#"{"version": 99, "timelines": {}}"#).unwrap();
    assert_eq!(opened(&project, "main").view.current(), None);
    let edited =
        r#"{"version": 1, "timelines": {"main": {"origin": 0, "pxPerSec": 1e9, "scrollY": -40}}}"#;
    std::fs::write(&path, edited).unwrap();
    let s = opened(&project, "main").view.current().expect("読める");
    assert_eq!((s.time.px_per_sec, s.scroll_y), (MAX_PX_PER_SEC, 0.0));
    assert_eq!(s.heights, RowHeights::default(), "無い行の高さは既定");
}

#[test]
fn 置き場所は_project_mmdb_のフォルダーかプロジェクトの_json_のあるフォルダーの_mm() {
    let tmp = tempfile::tempdir().unwrap();
    let project = json_project(tmp.path());
    assert_eq!(
        view_path(&project),
        tmp.path().join(".mm").join("view.json")
    );
    let store = tmp.path().join("store");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(store.join("project.mmdb"), "").unwrap();
    assert_eq!(view_path(&store), store.join(".mm").join("view.json"));
}
