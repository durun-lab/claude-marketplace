//! 読み出し（単語、検索、範囲）の試験は `mm-story`。ここは再生位置への変換と、アプリの最新のデータを渡す層。

use std::fs;

use mm_doc::{Asset, Cut, Node, Sequence, WordEdit, SEQUENCE_KIND, SEQUENCE_PARAM};
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
