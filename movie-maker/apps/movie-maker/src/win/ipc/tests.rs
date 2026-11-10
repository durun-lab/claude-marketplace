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
    let sel = selection_of(&doc, None);
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
    let sel = selection_of(&open(Vec::new()), None);
    assert_eq!(sel.composition.map(|c| c.layer_count), Some(0));
}

#[test]
fn 選んだレイヤーがあればそれも送る() {
    let sel = selection_of(&open(vec![solid("a")]), Some("a"));
    assert_eq!(sel.layer.map(|l| l.id.to_string()), Some("a".into()));
}
