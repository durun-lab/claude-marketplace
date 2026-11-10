use mm_doc::{Asset, Cut, Layer, Node, Sequence, SEQUENCE_KIND, SEQUENCE_PARAM};
use mm_types::{ColorSpace, Id, ParamValue, Rational, RationalTime};

use super::*;

fn secs(s: i64) -> RationalTime {
    RationalTime::seconds(s)
}

fn comp(layers: Vec<Layer>) -> Composition {
    Composition::new(Id::new("main"), 64, 36, Rational::integer(30), secs(10)).with_layers(layers)
}

fn solid(id: &str) -> Layer {
    Layer::new(id, Node::new("solid", 1), secs(0), secs(0), secs(5))
}

/// シーケンス `s` を置いた sequence のレイヤー。
fn sequence_layer() -> Layer {
    let node = Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s".into()));
    Layer::new("seq", node, secs(0), secs(0), secs(5))
}

/// カット `cuts` を並べたシーケンス `s` を持つプロジェクト。
fn project_with(cuts: Vec<Cut>) -> Project {
    let mut p = Project::new();
    p.sequences.push(Sequence {
        id: Id::new("s"),
        cuts,
        origin: None,
    });
    p
}

fn footage_cut(id: &str) -> Cut {
    Cut::new(id, Node::footage(&Id::new("a")), secs(0), secs(2))
}

fn with_asset(mut p: Project) -> Project {
    p.assets.push(Asset {
        id: Id::new("a"),
        path: "a.mp4".into(),
        content_hash: "hash-a".into(),
        color_space: ColorSpace::bt709(),
        duration: secs(20),
        fps: Some(Rational::integer(30)),
        timebase_offset: RationalTime::ZERO,
    });
    p
}

const PROGRAM_ONLY: EmptyState = EmptyState {
    program: true,
    timeline: false,
};
const BOTH: EmptyState = EmptyState {
    program: true,
    timeline: true,
};
const NONE: EmptyState = EmptyState {
    program: false,
    timeline: false,
};

#[test]
fn レイヤーも素材も無いと_program_に取り込みの案内だけを出す() {
    assert_eq!(
        empty_state(&Project::new(), &comp(Vec::new())),
        PROGRAM_ONLY
    );
}

#[test]
fn カットの無い_sequence_のレイヤーには_timeline_に_insert_の手順も出す() {
    let state = empty_state(&project_with(Vec::new()), &comp(vec![sequence_layer()]));
    assert_eq!(state, BOTH);
}

#[test]
fn 素材を取り込んだら_program_の取り込みの案内は消す() {
    let p = with_asset(project_with(Vec::new()));
    assert_eq!(
        empty_state(&p, &comp(vec![sequence_layer()])),
        EmptyState {
            program: false,
            timeline: true,
        }
    );
    assert_eq!(empty_state(&p, &comp(Vec::new())), NONE);
}

#[test]
fn カットか_sequence_でないレイヤーがあれば中身があるので出さない() {
    let p = project_with(vec![footage_cut("k1")]);
    assert_eq!(empty_state(&p, &comp(vec![sequence_layer()])), NONE);
    let p = project_with(Vec::new());
    assert_eq!(
        empty_state(&p, &comp(vec![sequence_layer(), solid("bg")])),
        NONE
    );
}

#[test]
fn 無効のレイヤーと無効のカットも中身に数える() {
    let mut off = solid("bg");
    off.enabled = false;
    assert_eq!(empty_state(&Project::new(), &comp(vec![off])), NONE);
    let mut cut = footage_cut("k1");
    cut.enabled = false;
    let p = project_with(vec![cut]);
    assert_eq!(empty_state(&p, &comp(vec![sequence_layer()])), NONE);
}
