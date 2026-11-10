//! sequence のレイヤーが無いコンポジションへの最初の Insert と Overwrite。

use mm_ops::{apply_transaction, with_fitted_layers, Entry, OpContext, TxnOutcome};
use mm_types::Rational;

use super::*;
use crate::win::sequence_edit::tests::sample_project;
use crate::win::sequence_edit::{plan_insert, plan_overwrite, Ctx, Planned, SourceRange};

fn secs(n: i64) -> RationalTime {
    RationalTime::seconds(n)
}

/// 素材 `a`（10 秒）と、レイヤーの無い長さ `len` 秒のコンポジション `main` のプロジェクト。`mm new` の後に `main` を足した形。
fn empty_project(len: i64) -> Project {
    let mut p = sample_project();
    p.sequences.clear();
    p.compositions = vec![Composition::new(
        Id::new("main"),
        64,
        36,
        Rational::integer(30),
        secs(len),
    )];
    p
}

/// 再生位置 4 秒の、何も選んでいない編集の入力。
fn ctx(p: &Project) -> Ctx<'_> {
    Ctx {
        project: p,
        comp: &p.compositions[0],
        layer: None,
        cut: None,
        playhead: 4.0,
    }
}

/// 素材 `a` の `from` 秒から `to` 秒。
fn range(from: i64, to: i64) -> SourceRange {
    SourceRange {
        asset: Id::new("a"),
        in_point: secs(from),
        out_point: secs(to),
    }
}

fn apply(p: &mut Project, ops: &[Op]) -> TxnOutcome {
    apply_transaction(p, ops, &OpContext::new(Entry::WebView)).expect("適用できる")
}

fn names(planned: &Planned) -> Vec<&'static str> {
    planned.ops.iter().map(Op::name).collect()
}

#[test]
fn insert_は_sequence_とレイヤーとカットを_1_つのトランザクションで作り_取り消し_1_回で全部戻る() {
    let mut p = empty_project(10);
    let before = p.clone();
    let planned = plan_insert(&ctx(&p), &range(5, 6), Id::new("new")).unwrap();
    assert_eq!(
        names(&planned),
        ["addSequence", "addLayer", "insert", "fitLayer"]
    );
    assert_eq!(planned.select, Some(Id::new("new")));
    assert_eq!(
        with_fitted_layers(&p, &planned.ops),
        planned.ops,
        "保存の層は fitLayer を重ねて足さない"
    );
    let outcome = apply(&mut p, &planned.ops);

    let seq = p.sequence(&Id::new("main-seq")).unwrap();
    let cut = &seq.cuts[..];
    assert_eq!(cut.len(), 1);
    assert_eq!(
        (cut[0].id.as_str(), cut[0].in_point, cut[0].out_point),
        ("new", secs(5), secs(6))
    );
    let (comp, layer) = p.layer(&Id::new("main-seq-layer")).unwrap();
    assert_eq!(layer.source.text_param(SEQUENCE_PARAM), Some("main-seq"));
    assert_eq!(
        (layer.start, layer.in_point, layer.out_point),
        (secs(0), secs(0), secs(1)),
        "レイヤーはカットの長さ"
    );
    assert_eq!(
        comp.duration,
        secs(10),
        "カットより長いコンポジションは縮めない"
    );

    apply(&mut p, &outcome.inverse_ops().unwrap());
    assert_eq!(p, before, "sequence もレイヤーもカットも消える");
}

#[test]
fn overwrite_も置き換えるカットが無いので_insert_と同じく_sequence_から作る() {
    let p = empty_project(10);
    let insert = plan_insert(&ctx(&p), &range(5, 6), Id::new("new")).unwrap();
    let overwrite = plan_overwrite(&ctx(&p), &range(5, 6), Id::new("new")).unwrap();
    assert_eq!(overwrite, insert);
}

#[test]
fn カットがコンポジションより長ければ_コンポジションをカットの長さまで伸ばす() {
    let mut p = empty_project(2);
    let planned = plan_insert(&ctx(&p), &range(1, 6), Id::new("new")).unwrap();
    apply(&mut p, &planned.ops);
    assert_eq!(p.compositions[0].duration, secs(5));
}

#[test]
fn 作った_sequence_のレイヤーは次の_insert_の入る先になり_長さが追従する() {
    let mut p = empty_project(10);
    let first = plan_insert(&ctx(&p), &range(5, 6), Id::new("k1")).unwrap();
    apply(&mut p, &first.ops);
    let second = plan_insert(&ctx(&p), &range(0, 2), Id::new("k2")).unwrap();
    assert_eq!(
        names(&second),
        ["insert"],
        "sequence のレイヤーがあれば今までどおり"
    );
    let ops = with_fitted_layers(&p, &second.ops);
    assert_eq!(ops.len(), 2, "レイヤーが sequence の長さに追従している");
    apply(&mut p, &ops);
    let (_, layer) = p.layer(&Id::new("main-seq-layer")).unwrap();
    assert_eq!(layer.out_point, secs(3));
}

#[test]
fn id_がプロジェクトの同じ種類の_id_と重なれば番号を付ける() {
    let mut p = empty_project(10);
    p.sequences.push(Sequence {
        id: Id::new("main-seq"),
        cuts: Vec::new(),
        origin: None,
    });
    let new = NewSequence::ids(&p, &p.compositions[0]);
    assert_eq!(
        (new.sequence.as_str(), new.layer.as_str()),
        ("main-seq-2", "main-seq-2-layer")
    );
}
