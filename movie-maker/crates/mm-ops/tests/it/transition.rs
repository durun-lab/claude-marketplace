//! `setTransition`。カットの先頭のトランジションを置く・種類と長さを差し替える・外す。取り消し 1 回で元に戻る。

use mm_doc::{Node, Transition, NESTED_KIND, NESTED_PARAM};
use mm_ops::{Editor, Op, OpContext, SetTransition, Transaction};
use mm_types::{Id, ParamValue};

use crate::common::{canon, project, quarters};

fn transition(kind: &str, q: i64) -> Transition {
    Transition {
        node: Node::new(kind, 1).with_param("p0", ParamValue::Number(2.0)),
        duration: quarters(q),
    }
}

fn set(cut: &str, transition: Option<Transition>) -> Transaction {
    let op = SetTransition {
        cut: Id::new(cut),
        transition,
    };
    Transaction::new("トランジション", vec![op.into()])
}

fn transition_of(ed: &Editor, cut: &str) -> Option<Transition> {
    ed.project()
        .cut(&Id::new(cut))
        .unwrap()
        .1
        .transition_in
        .clone()
}

#[test]
fn 置く_差し替える_外すがそれぞれ取り消しで戻る() {
    let before = project();
    let ctx = OpContext::human();
    let mut ed = Editor::new(before.clone());
    let steps = [
        Some(transition("transition.dissolve", 4)),
        Some(transition("transition.wgsl", 3)),
        None,
    ];
    let mut history = vec![canon(ed.project())];
    for step in &steps {
        ed.apply(set("k2", step.clone()), &ctx).unwrap();
        assert_eq!(&transition_of(&ed, "k2"), step);
        history.push(canon(ed.project()));
    }
    for expected in history.iter().rev().skip(1) {
        ed.undo(&ctx).unwrap();
        assert_eq!(&canon(ed.project()), expected, "取り消すと 1 つ前に戻る");
    }
    assert_eq!(canon(ed.project()), canon(&before));
}

#[test]
fn 長さが負なら弾いて変えない() {
    let before = project();
    let mut ed = Editor::new(before.clone());
    let err = ed.apply(
        set("k2", Some(transition("transition.dissolve", -1))),
        &OpContext::human(),
    );
    assert!(err.is_err(), "負の長さは適用後の validate が弾く");
    assert_eq!(canon(ed.project()), canon(&before));
}

#[test]
fn 触る所に入れ子の先の先まで入る() {
    let mut p = project();
    // c1 の l1 が c2 を入れ子にするので、トランジションが c1 を入れ子にすると c2 まで読む。
    let nested = |target: &str| {
        Node::new(NESTED_KIND, 1).with_param(NESTED_PARAM, ParamValue::Text(target.into()))
    };
    p.compositions[0].layers[0].source = nested("c2");
    let op = SetTransition {
        cut: Id::new("k2"),
        transition: Some(Transition {
            node: nested("c1"),
            duration: quarters(2),
        }),
    };
    let t = Op::from(op).touches(&p).unwrap();
    assert!(t.contains("c1"), "直接の参照先");
    assert!(t.contains("c2"), "入れ子の先");
}

#[test]
fn 無いカットは弾く() {
    let mut ed = Editor::new(project());
    let err = ed.apply(set("nope", None), &OpContext::human());
    assert!(err.is_err());
}

#[test]
fn 操作の名前の一覧に入る() {
    assert!(mm_ops::Op::NAMES.contains(&"setTransition"));
}
