//! 読み出し（単語、検索、範囲）の試験は `mm-story`。ここは再生位置への変換と、アプリの最新のデータを渡す層。

use std::fs;

use mm_doc::{Asset, Cut, Node, Sequence, Topic, WordEdit, SEQUENCE_KIND, SEQUENCE_PARAM};
use mm_types::{ColorSpace, Id, ParamValue, RationalTime};

use super::*;

const HASH: &str = "ca152c0602faf08d897d38f301d5883036d18f9155cac34ae0cc582653c33f75";
const ASSET: &str = "a1";

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ingest/transcript-ja.json")
}

fn asset() -> Asset {
    Asset {
        id: Id::new(ASSET),
        path: "C:/media/speech-ja.wav".into(),
        content_hash: HASH.into(),
        color_space: ColorSpace::bt709(),
        duration: RationalTime::seconds(5),
        fps: None,
        timebase_offset: RationalTime::zero(),
    }
}

fn secs(n: i64) -> RationalTime {
    RationalTime::seconds(n)
}

/// 付属ファイルだけの素材 1 つのプロジェクト。`cache/ingest/` に日本語の文字起こしのフィクスチャを置く。
fn state(dir: &Path, edit: impl FnOnce(&mut Project)) -> StoryState {
    let cache = dir.join("cache");
    let sidecar = cache.join("ingest").join(format!("{HASH}.transcript.json"));
    fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
    fs::copy(fixture(), sidecar).unwrap();
    let mut project = Project::new();
    project.assets.push(asset());
    edit(&mut project);
    let shared = Arc::new(Shared::new());
    shared.publish_doc(Arc::new(project), Id::new("main"));
    StoryState::new(shared, &dir.join("project.json"), cache)
}

#[test]
fn 単語にはアプリの最新の修正が重なる() {
    let dir = tempfile::tempdir().unwrap();
    let probe = state(dir.path(), |_| {});
    let words = probe.transcript(ASSET).unwrap().words;
    let del = words.iter().find(|w| w.text == "ソ").unwrap().id.clone();
    let story = state(dir.path(), |p| {
        p.transcript_edits = vec![WordEdit {
            asset: Id::new(ASSET),
            word: Id::new(&del),
            text: None,
            deleted: true,
        }];
    });
    let words = story.transcript(ASSET).unwrap().words;
    assert!(words.iter().find(|w| w.text == "ソ").unwrap().deleted);
    assert!(story.transcript("nothing").is_err());
}

fn sequence_layer(seq: &str) -> mm_doc::Layer {
    let node = Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text(seq.into()));
    mm_doc::Layer::new("ls", node, secs(0), secs(0), secs(60))
}

fn cut(id: &str, from: i64, to: i64) -> Cut {
    Cut::new(id, Node::footage(&Id::new(ASSET)), secs(from), secs(to))
}

#[test]
fn 素材の時刻は_sequence_のカットの位置に変わる() {
    let dir = tempfile::tempdir().unwrap();
    let story = state(dir.path(), |p| {
        p.sequences.push(Sequence {
            id: Id::new("s1"),
            cuts: vec![cut("k1", 10, 14), cut("k2", 30, 33)],
            origin: None,
        });
        p.compositions.push(
            mm_doc::Composition::new(
                Id::new("main"),
                320,
                180,
                mm_types::Rational::integer(30),
                secs(60),
            )
            .with_layers(vec![sequence_layer("s1")]),
        );
    });
    assert_eq!(story.timeline_time(ASSET, 31.0).unwrap(), Some(5.0));
    assert_eq!(story.timeline_time(ASSET, 10.0).unwrap(), Some(0.0));
    assert_eq!(story.timeline_time(ASSET, 20.0).unwrap(), None);
    let overview = story.overview().unwrap();
    assert_eq!(overview.sequences[0].duration, 7.0);
    assert_eq!(overview.open_sequence.as_deref(), Some("s1"));
    assert!(overview.assets[0].has_transcript);
}

#[test]
fn 撮影素材のレイヤーでも素材の時刻が再生位置に変わる() {
    let dir = tempfile::tempdir().unwrap();
    let story = state(dir.path(), |p| {
        let layer = mm_doc::Layer::new(
            "lf",
            Node::footage(&Id::new(ASSET)),
            secs(2),
            secs(10),
            secs(20),
        );
        p.compositions.push(
            mm_doc::Composition::new(
                Id::new("main"),
                320,
                180,
                mm_types::Rational::integer(30),
                secs(30),
            )
            .with_layers(vec![layer]),
        );
    });
    assert_eq!(story.timeline_time(ASSET, 13.0).unwrap(), Some(5.0));
    assert_eq!(story.timeline_time(ASSET, 20.0).unwrap(), None);
    assert_eq!(story.timeline_time("other", 13.0).unwrap(), None);
}

/// 素材 1 つと話題 2 つ（素材の 1-3 秒と 4-5 秒）と、レイヤーの無いコンポジション `main` を書き手で開いた Story。
fn saved_story(dir: &Path) -> (StoryState, Arc<mm_serve::Core>) {
    let mut project = Project::new();
    project.assets.push(asset());
    for (id, from, to) in [("t1", 1, 3), ("t2", 4, 5)] {
        project.bins.topics.push(Topic {
            id: Id::new(id),
            asset: Id::new(ASSET),
            in_point: secs(from),
            out_point: secs(to),
            title: id.into(),
            rating: Default::default(),
            tags: Vec::new(),
        });
    }
    let fps = mm_types::Rational::integer(30);
    let comp = mm_doc::Composition::new(Id::new("main"), 320, 180, fps, secs(10));
    project.compositions.push(comp);
    let store = mm_store::Store::create(dir, &project, 0).expect("project.mmdb を作る");
    let core = mm_serve::Core::new(store, None);
    let shared = Arc::new(Shared::new());
    let _ = shared.core.set(core.clone());
    let version = core.version().expect("バージョン");
    shared.publish_saved(version, Arc::new(project), Id::new("main"));
    let story = StoryState::new(shared, &dir.join("project.mmdb"), dir.join("cache"));
    (story, core)
}

#[test]
fn 並べ先の_sequence_が無ければ_開いているコンポジションに_sequence_とレイヤーを作って並べ_取り消し_1_回で戻る(
) {
    let dir = tempfile::tempdir().unwrap();
    let (story, core) = saved_story(dir.path());
    let before = core.project().unwrap().project;
    let ids = |v: [&str; 2]| v.map(String::from);
    story
        .arrange_new(&ids(["t2", "t1"]), &ids(["k2", "k1"]))
        .expect("並べる");
    let after = core.project().unwrap().project;
    let seq = after.sequence(&Id::new("main-seq")).unwrap();
    let cuts: Vec<&str> = seq.cuts.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(cuts, ["k2", "k1"], "話題の順");
    let (_, layer) = after.layer(&Id::new("main-seq-layer")).unwrap();
    assert_eq!(layer.out_point, secs(3), "話題の長さの和");
    let overview = story.overview().unwrap();
    assert_eq!(overview.open_sequence.as_deref(), Some("main-seq"));

    core.undo(&mm_serve::Caller::webview()).expect("取り消す");
    assert_eq!(core.project().unwrap().project, before);
}
