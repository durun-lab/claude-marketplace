use std::sync::Arc;

use mm_doc::{Layer, Node, Project};
use mm_types::{Id, Rational, RationalTime};

use super::*;

fn open(layers: Vec<Layer>) -> OpenDoc {
    let mut project = Project::new();
    project.compositions.push(
        Composition::new(
            Id::new("main"),
            1920,
            1080,
            Rational::integer(30),
            RationalTime::seconds(12),
        )
        .with_layers(layers),
    );
    OpenDoc {
        project: Arc::new(project),
        comp: Id::new("main"),
        version: None,
    }
}

fn solid(id: &str) -> Layer {
    let s = RationalTime::seconds;
    Layer::new(id, Node::new("solid", 1), s(0), s(0), s(5))
}

#[test]
fn 何も選んでいなくてもコンポジションの要約を送る() {
    let doc = open(vec![solid("a"), solid("b")]);
    let sel = selection_of(&doc, None, None);
    assert!(sel.layer.is_none());
    assert_eq!(
        sel.composition,
        Some(CompositionSummary {
            name: "main".into(),
            width: 1920,
            height: 1080,
            fps: 30.0,
            duration: 12.0,
            layer_count: 2,
        })
    );
}

#[test]
fn レイヤーが無いコンポジションはレイヤーの数が_0() {
    let sel = selection_of(&open(Vec::new()), None, None);
    assert_eq!(sel.composition.map(|c| c.layer_count), Some(0));
}

#[test]
fn 選んだレイヤーがあればそれも送る() {
    let sel = selection_of(&open(vec![solid("a")]), Some("a"), None);
    assert_eq!(sel.layer.map(|l| l.id.to_string()), Some("a".into()));
}

#[test]
fn キーフレームの行で選んだパラメータはレイヤーを選んでいるときだけ送る() {
    let param = || {
        Some(ParamSelection {
            path: "opacity".into(),
            serial: 3,
        })
    };
    let doc = open(vec![solid("a")]);
    let sel = selection_of(&doc, Some("a"), param());
    assert_eq!(sel.param, param());
    assert_eq!(selection_of(&doc, Some("gone"), param()).param, None);
}

#[test]
fn 選んだ行のパラメータは同じレイヤーを選んでいる間だけ引ける() {
    let shared = Shared::new();
    let row = |keyframe| crate::win::keyframe_edit::RowSelection {
        layer: "a".into(),
        path: Some("transform.position".into()),
        keyframe,
    };
    shared.keyframes.select(Some(row(true)));
    let first = shared.keyframes.param_for(Some("a"));
    assert_eq!(
        first.as_ref().map(|p| p.path.as_str()),
        Some("transform.position")
    );
    assert_eq!(shared.keyframes.param_for(Some("b")), None);
    assert!(shared.keyframes.keyframe_selected());
    shared.keyframes.select(Some(row(true)));
    let again = shared.keyframes.param_for(Some("a"));
    assert_ne!(
        first.map(|p| p.serial),
        again.map(|p| p.serial),
        "選び直すと通し番号が進む"
    );
    shared.keyframes.match_pick(Some(false));
    assert!(
        !shared.keyframes.keyframe_selected(),
        "キーフレームが無くなれば外す"
    );
    shared.keyframes.match_pick(None);
    assert_eq!(shared.keyframes.param_for(Some("a")), None);
}
