//! プレビューの字幕の焼き込み。書き出しと同じ関数の結果を描くことを、描くプロジェクトのデータで確かめる。

use std::sync::Arc;

use mm_doc::{
    Cut, Layer, Node, Project, Sequence, FOOTAGE_KIND, FOOTAGE_PARAM, SEQUENCE_KIND, SEQUENCE_PARAM,
};
use mm_runtime::RuntimeConfig;
use mm_store::{Store, TranscriptInput, WordInput};
use mm_testkit::decode::SampleMedia;
use mm_testkit::fixtures::vertical_project;
use mm_types::{CompositionId, Id, ParamValue, RationalTime};

use super::player::{Player, PlayerOptions};
use crate::win::state::Shared;

fn seconds(s: i64) -> RationalTime {
    RationalTime::seconds(s)
}

/// 素材 clip の 3〜5 秒を切る sequence を置いたコンポジションと、clip の文字起こしを持つ `project.mmdb`。
fn project_with_store(dir: &std::path::Path, burn_in: bool) -> (Project, CompositionId) {
    let (mut project, comp, _) = vertical_project(&SampleMedia::default(), &dir.join("a.json"));
    let clip = ParamValue::AssetRef(Id::new("clip"));
    let footage = Node::new(FOOTAGE_KIND, 1).with_param(FOOTAGE_PARAM, clip);
    project.sequences.push(Sequence {
        id: Id::new("s"),
        cuts: vec![Cut::new("k1", footage, seconds(3), seconds(5))],
        origin: None,
    });
    let node = Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s".into()));
    project.compositions[0].layers =
        vec![Layer::new("seq", node, seconds(0), seconds(0), seconds(2))];
    project.render_targets[0].delivery.burn_in_captions = burn_in;
    let mut store = Store::create(&dir.join("store"), &project, 0).unwrap();
    let words = [("こんにちは。", 3), ("世界。", 4)].map(|(text, at)| WordInput {
        segment: 0,
        start: seconds(at),
        end: RationalTime::new(i128::from(at) * 1000 + 800, 1000).unwrap(),
        text: text.into(),
    });
    let input = TranscriptInput {
        producer: "test".into(),
        language: "ja".into(),
        words: words.to_vec(),
    };
    let hash = project.assets[0].content_hash.clone();
    store
        .put_transcript(&Id::new("clip"), &hash, &input)
        .unwrap();
    (project, comp)
}

fn player(project: &Project, comp: CompositionId, dir: &std::path::Path) -> Player {
    player_in(Arc::new(Shared::new()), project, comp, dir)
}

/// `shared` を共有する再生。アプリと同じく `Core` を置いた `Shared` を渡せる。
fn player_in(
    shared: Arc<Shared>,
    project: &Project,
    comp: CompositionId,
    dir: &std::path::Path,
) -> Player {
    let options = PlayerOptions {
        no_audio: true,
        config: Some(RuntimeConfig::sample()),
        store_dir: Some(dir.join("store")),
    };
    Player::new(shared, Arc::new(project.clone()), comp, options)
        .expect("再生を始める")
        .0
}

fn text_layers(project: &Project, comp: &CompositionId) -> usize {
    let layers = &project.composition(comp).unwrap().layers;
    layers.iter().filter(|l| l.source.kind == "text").count()
}

#[test]
fn 焼き込むレンダーターゲットがあるとプレビューは書き出しと同じ字幕のレイヤーを描く() {
    let guard = tempfile::Builder::new()
        .prefix("mm_preview_burn_in-")
        .tempdir()
        .unwrap();
    let dir = guard.path();
    let (project, comp) = project_with_store(dir, true);
    let mut player = player(&project, comp.clone(), dir);
    let shown = player.shown_project();
    assert!(text_layers(&shown, &comp) >= 2, "字幕の影と本体が足される");
    let words = crate::win::deliver::transcript_words(Some(&dir.join("store")), &project);
    let engine = mm_runtime::assemble(&RuntimeConfig::sample()).unwrap();
    let exported = mm_export::burn_in_captions(
        &project,
        &comp,
        &words,
        engine.services().text_shaper.as_ref(),
    )
    .unwrap();
    assert_eq!(*shown, exported, "書き出しの描くプロジェクトのデータと同じ");
    assert!(
        Arc::ptr_eq(&shown, &player.shown_project()),
        "元が同じ間は作り直さない"
    );
}

/// 書き手として `project.mmdb` を開いた `Core` を置いた `Shared`（アプリと同じ形）。
fn shared_with_core(dir: &std::path::Path) -> (Arc<Shared>, Arc<mm_serve::Core>) {
    let shared = Arc::new(Shared::new());
    let core = mm_serve::Core::new(Store::open(&dir.join("store"), 0).unwrap(), None);
    let _ = shared.core.set(core.clone());
    (shared, core)
}

#[test]
fn 文字起こしを取り込み直すとプロジェクトのデータが同じでも字幕を作り直す() {
    let guard = tempfile::Builder::new()
        .prefix("mm_preview_retranscribe-")
        .tempdir()
        .unwrap();
    let dir = guard.path();
    let (project, comp) = project_with_store(dir, true);
    let (shared, core) = shared_with_core(dir);
    let mut player = player_in(shared, &project, comp, dir);
    let before = player.shown_project();
    let words = [("さようなら。", 3)].map(|(text, at)| WordInput {
        segment: 0,
        start: seconds(at),
        end: RationalTime::new(i128::from(at) * 1000 + 800, 1000).unwrap(),
        text: text.into(),
    });
    let input = TranscriptInput {
        producer: "test".into(),
        language: "ja".into(),
        words: words.to_vec(),
    };
    let hash = project.assets[0].content_hash.clone();
    core.put_transcript(&Id::new("clip"), &hash, &input)
        .unwrap();
    let after = player.shown_project();
    assert!(!Arc::ptr_eq(&before, &after), "単語が変わったので作り直す");
    assert_ne!(before, after);
}

#[test]
fn 文字起こしに関係の無い保存では字幕を作り直さない() {
    let guard = tempfile::Builder::new()
        .prefix("mm_preview_unrelated-")
        .tempdir()
        .unwrap();
    let dir = guard.path();
    let (project, comp) = project_with_store(dir, true);
    let (shared, core) = shared_with_core(dir);
    let mut player = player_in(shared, &project, comp.clone(), dir);
    let before = player.shown_project();
    let comment = mm_store::NewComment {
        comp: comp.to_string(),
        client_id: None,
        at: Some(seconds(1)),
        range: None,
        text: "ここ".into(),
        drawing: Vec::new(),
    };
    core.add_comment(&comment, &mm_serve::Caller::webview())
        .unwrap();
    assert!(
        Arc::ptr_eq(&before, &player.shown_project()),
        "コメントの保存では使い回す"
    );
}

#[test]
fn 焼き込まない設定のプレビューは字幕を足さない() {
    let guard = tempfile::Builder::new()
        .prefix("mm_preview_plain-")
        .tempdir()
        .unwrap();
    let dir = guard.path();
    let (project, comp) = project_with_store(dir, false);
    let mut player = player(&project, comp.clone(), dir);
    assert_eq!(*player.shown_project(), project);
}
