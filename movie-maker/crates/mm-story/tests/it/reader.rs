use std::fs;
use std::path::Path;

use mm_doc::{
    Asset, Composition, Cut, Layer, Node, Project, Sequence, WordEdit, SEQUENCE_KIND,
    SEQUENCE_PARAM,
};
use mm_store::Store;
use mm_story::{range_end, StoryReader, StoryWord};
use mm_types::{ColorSpace, Id, ParamValue, Rational, RationalTime};

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
fn setup(dir: &Path, edit: impl FnOnce(&mut Project)) -> (StoryReader, Project) {
    let cache = dir.join("cache");
    let sidecar = cache.join("ingest").join(format!("{HASH}.transcript.json"));
    fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
    fs::copy(fixture(), sidecar).unwrap();
    let mut project = Project::new();
    project.assets.push(asset());
    edit(&mut project);
    (StoryReader::new(&dir.join("project.json"), cache), project)
}

fn word_ids(words: &[StoryWord]) -> Vec<String> {
    words.iter().map(|w| w.id.clone()).collect()
}

fn find<'a>(words: &'a [StoryWord], text: &str) -> &'a StoryWord {
    words.iter().find(|w| w.text == text).unwrap()
}

fn edit_of(word: &str, text: Option<&str>, deleted: bool) -> WordEdit {
    WordEdit {
        asset: Id::new(ASSET),
        word: Id::new(word),
        text: text.map(Into::into),
        deleted,
    }
}

/// 付属ファイルと同じ単語を DB の表に入れた `Store`。
fn db_with_words(dir: &Path, reader: &StoryReader, one_segment: bool) -> Store {
    let mut project = Project::new();
    project.assets.push(asset());
    let mut store = Store::create(&dir.join("db"), &project, 0).unwrap();
    let t = mm_ingest::sidecar::load_transcript(reader.cache(), HASH)
        .unwrap()
        .unwrap();
    let segments = if one_segment {
        &t.segments[..1]
    } else {
        &t.segments[..]
    };
    let input = mm_store::TranscriptInput {
        producer: "test".into(),
        language: t.language.clone(),
        words: segments
            .iter()
            .flat_map(|s| {
                s.words.iter().map(|w| mm_store::WordInput {
                    segment: 0,
                    start: w.start,
                    end: w.end,
                    text: w.text.clone(),
                })
            })
            .collect(),
    };
    store.put_transcript(&Id::new(ASSET), HASH, &input).unwrap();
    store
}

#[test]
fn 付属ファイルの単語は_db_と同じ_id_になる() {
    let dir = tempfile::tempdir().unwrap();
    let (story, project) = setup(dir.path(), |_| {});
    let from_sidecar = story.words(&project, ASSET).unwrap();
    let store = db_with_words(dir.path(), &story, false);
    let from_db: Vec<String> = store
        .transcript_words(&Id::new(ASSET))
        .unwrap()
        .iter()
        .map(|w| w.word.to_string())
        .collect();
    assert_eq!(word_ids(&from_sidecar), from_db);
}

#[test]
fn 消した単語と直した文字はいまのプロジェクトのデータから重なる() {
    let dir = tempfile::tempdir().unwrap();
    let (probe, project) = setup(dir.path(), |_| {});
    let words = probe.words(&project, ASSET).unwrap();
    let (del, fix) = (find(&words, "ソ").id.clone(), find(&words, "編").id.clone());
    let (story, project) = setup(dir.path(), |p| {
        p.transcript_edits = vec![edit_of(&del, None, true), edit_of(&fix, Some("偏"), false)];
    });
    let words = story.words(&project, ASSET).unwrap();
    assert!(find(&words, "ソ").deleted);
    assert_eq!(find(&words, "編").fixed.as_deref(), Some("偏"));
    assert!(!find(&words, "フ").deleted);
}

#[test]
fn 範囲は先後どちらから指しても同じ時刻() {
    let dir = tempfile::tempdir().unwrap();
    let (story, project) = setup(dir.path(), |_| {});
    let words = story.words(&project, ASSET).unwrap();
    let (a, b) = (find(&words, "動画"), find(&words, "ト"));
    let forward = story.range(&project, ASSET, &a.id, &b.id).unwrap();
    assert_eq!(forward, story.range(&project, ASSET, &b.id, &a.id).unwrap());
    assert_eq!((forward.start, forward.end), (a.start, b.end));
    assert!(story.range(&project, ASSET, "w_none", &b.id).is_err());
}

#[test]
fn 検索は単語の境をまたぎ_消した単語を外し_直した文字で探す() {
    let dir = tempfile::tempdir().unwrap();
    let (probe, project) = setup(dir.path(), |_| {});
    let words = probe.words(&project, ASSET).unwrap();
    let (fix, del) = (find(&words, "編").id.clone(), find(&words, "起").id.clone());
    let (story, project) = setup(dir.path(), |p| {
        p.transcript_edits = vec![edit_of(&fix, Some("偏"), false), edit_of(&del, None, true)];
    });
    let hit = &story.search(&project, "文字", 10).unwrap()[0];
    assert_eq!(hit.word, find(&words, "文").id);
    assert_eq!(hit.end, find(&words, "字").end);
    assert_eq!(story.search(&project, "動画偏集", 10).unwrap().len(), 1);
    assert!(story.search(&project, "動画編集", 10).unwrap().is_empty());
    assert_eq!(
        story.search(&project, "字こし", 10).unwrap().len(),
        1,
        "消した単語はスキップする"
    );
    assert!(story.search(&project, "", 10).is_err());
}

#[test]
fn 検索は_mm_search_と同じ件を返す() {
    let dir = tempfile::tempdir().unwrap();
    let (story, project) = setup(dir.path(), |_| {});
    let store = db_with_words(dir.path(), &story, true);
    for q in [
        "の",
        "こんにちは",
        "編集ソフト",
        "文字起こし",
        "です。",
        "試験です",
    ] {
        let want: Vec<_> = store
            .search_words(q, 50)
            .unwrap()
            .iter()
            .map(|h| (h.word.to_string(), h.start, h.end))
            .collect();
        let got: Vec<_> = story
            .search(&project, q, 50)
            .unwrap()
            .iter()
            .map(|h| (h.word.clone(), h.start, h.end))
            .collect();
        assert_eq!(got, want, "検索 {q}");
    }
}

#[test]
fn 概要は開いているシーケンスと素材の文字起こしの有無を返す() {
    let dir = tempfile::tempdir().unwrap();
    let node =
        Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s1".into()));
    let (story, project) = setup(dir.path(), |p| {
        p.sequences.push(Sequence {
            id: Id::new("s1"),
            cuts: vec![
                Cut::new("k1", Node::footage(&Id::new(ASSET)), secs(10), secs(14)),
                Cut::new("k2", Node::footage(&Id::new(ASSET)), secs(30), secs(33)),
            ],
            origin: None,
        });
        p.compositions.push(
            Composition::new(Id::new("main"), 320, 180, Rational::integer(30), secs(60))
                .with_layers(vec![Layer::new("ls", node, secs(0), secs(0), secs(60))]),
        );
    });
    let overview = story.overview(&project, &Id::new("main"));
    assert_eq!(overview.sequences[0].duration, 7.0);
    assert_eq!(overview.sequences[0].cuts, 2);
    assert_eq!(overview.open_sequence.as_deref(), Some("s1"));
    assert!(overview.assets[0].has_transcript);
    assert_eq!(overview.assets[0].name, "speech-ja.wav");
}

fn at(num: i64, den: u32) -> RationalTime {
    RationalTime::new(i128::from(num), den).unwrap()
}

fn timed(id: &str, start: RationalTime, end: RationalTime) -> StoryWord {
    StoryWord {
        id: id.into(),
        segment: 0,
        start,
        end,
        text: id.into(),
        fixed: None,
        deleted: false,
    }
}

#[test]
fn 範囲の終わりは次の単語の始まりを越えない() {
    let words = [
        timed("a", at(0, 1), at(3, 2)),
        timed("b", at(1, 1), at(2, 1)),
        timed("c", at(2, 1), at(5, 2)),
    ];
    assert_eq!(
        range_end(&words, 0),
        at(1, 1),
        "次の始まりより後に出る終わりは詰める"
    );
    assert_eq!(range_end(&words, 1), at(2, 1), "重ならなければそのまま");
    assert_eq!(range_end(&words, 2), at(5, 2), "最後の単語はそのまま");
}
