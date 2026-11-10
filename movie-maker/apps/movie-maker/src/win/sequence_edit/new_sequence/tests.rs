//! sequence のレイヤーが無いコンポジションへの Insert と Overwrite（sequence の自動作成）。

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

/// `empty_project` のコンポジションに、長さ `len` 秒の単色のレイヤー `bg`（sequence のレイヤーでない）を置いたもの。
fn with_background(len: i64) -> Project {
    let mut p = empty_project(len);
    let bg = Layer::new("bg", Node::new("solid", 1), secs(0), secs(0), secs(len));
    p.compositions[0].layers.push(bg);
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
        "Store が適用の前に通す with_fitted_layers は fitLayer を重ねて足さない"
    );
    let outcome = apply(&mut p, &planned.ops);

    let seq = p.sequence(&Id::new("Sequence 1")).unwrap();
    let cut = &seq.cuts[..];
    assert_eq!(cut.len(), 1);
    assert_eq!(
        (cut[0].id.as_str(), cut[0].in_point, cut[0].out_point),
        ("new", secs(5), secs(6))
    );
    let (comp, layer) = p.layer(&Id::new("Sequence 1 layer")).unwrap();
    assert_eq!(layer.source.text_param(SEQUENCE_PARAM), Some("Sequence 1"));
    assert_eq!(
        (layer.start, layer.in_point, layer.out_point),
        (secs(0), secs(0), secs(1)),
        "レイヤーはカットの長さ"
    );
    assert_eq!(
        comp.duration,
        secs(1),
        "レイヤーの無いコンポジションはカットの長さにして、後ろに黒い画面を残さない"
    );

    apply(&mut p, &outcome.inverse_ops().unwrap());
    assert_eq!(p, before, "sequence もレイヤーもカットも消え、長さも戻る");
}

#[test]
fn 途中の操作が拒否されたら_足した_sequence_とレイヤーも残さない() {
    let mut p = empty_project(10);
    let before = p.clone();
    let missing = SourceRange {
        asset: Id::new("missing"),
        ..range(5, 6)
    };
    let planned = plan_insert(&ctx(&p), &missing, Id::new("new")).unwrap();
    let refused = apply_transaction(&mut p, &planned.ops, &OpContext::new(Entry::WebView));
    assert!(refused.is_err(), "無い素材のカットは insert で拒否される");
    assert_eq!(p, before, "先に適用した addSequence と addLayer も戻る");
}

#[test]
fn overwrite_も置き換えるカットが無いので_insert_と同じく_sequence_から作る() {
    let p = empty_project(10);
    let insert = plan_insert(&ctx(&p), &range(5, 6), Id::new("new")).unwrap();
    let overwrite = plan_overwrite(&ctx(&p), &range(5, 6), Id::new("new")).unwrap();
    assert_eq!(overwrite, insert);
}

#[test]
fn ほかのレイヤーがあるコンポジションは_カットが長ければ伸ばし_短ければ長さを変えない() {
    let mut short = with_background(10);
    let planned = plan_insert(&ctx(&short), &range(5, 6), Id::new("new")).unwrap();
    apply(&mut short, &planned.ops);
    assert_eq!(short.compositions[0].duration, secs(10));
    let (comp, _) = short.layer(&Id::new("Sequence 1 layer")).unwrap();
    assert_eq!(
        comp.layers.last().unwrap().id.as_str(),
        "Sequence 1 layer",
        "一番上"
    );

    let mut long = with_background(2);
    let planned = plan_insert(&ctx(&long), &range(1, 6), Id::new("new")).unwrap();
    apply(&mut long, &planned.ops);
    assert_eq!(long.compositions[0].duration, secs(5));
}

#[test]
fn 作った_sequence_のレイヤーは次の_insert_の対象になり_レイヤーもコンポジションも長さが追従する() {
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
    let (comp, layer) = p.layer(&Id::new("Sequence 1 layer")).unwrap();
    assert_eq!((layer.out_point, comp.duration), (secs(3), secs(3)));
}

#[test]
fn 最後のカットを抜いた空の_sequence_へは_insert_も_overwrite_も先頭に入り_レイヤーが追従する() {
    let mut p = empty_project(10);
    let first = plan_insert(&ctx(&p), &range(5, 6), Id::new("k1")).unwrap();
    apply(&mut p, &first.ops);
    let extract = with_fitted_layers(&p, &[mm_ops::Extract { cut: Id::new("k1") }.into()]);
    apply(&mut p, &extract);
    let (comp, layer) = p.layer(&Id::new("Sequence 1 layer")).unwrap();
    let frame = RationalTime::from_frames(1, comp.fps).unwrap();
    assert_eq!((layer.duration().unwrap(), comp.duration), (frame, frame));

    // 再生位置の 4 秒は 1 フレームのレイヤーの外だが、空の sequence の先頭に入る。
    let insert = plan_insert(&ctx(&p), &range(0, 2), Id::new("k2")).unwrap();
    let overwrite = plan_overwrite(&ctx(&p), &range(0, 2), Id::new("k2")).unwrap();
    assert_eq!(
        overwrite, insert,
        "置き換えるカットが無いので Insert と同じ"
    );
    assert_eq!(
        names(&insert),
        ["insert"],
        "残した sequence とレイヤーに入れる"
    );
    let ops = with_fitted_layers(&p, &insert.ops);
    assert_eq!(ops.len(), 2, "1 フレームのレイヤーは追従している");
    apply(&mut p, &ops);
    let (comp, layer) = p.layer(&Id::new("Sequence 1 layer")).unwrap();
    assert_eq!((layer.out_point, comp.duration), (secs(2), secs(2)));
}

#[test]
fn id_はほかの種類の_id_とも重複しない最初の番号にする() {
    let mut p = with_background(10);
    p.sequences.push(Sequence {
        id: Id::new("Sequence 1"),
        cuts: Vec::new(),
        origin: None,
    });
    p.compositions[0].layers[0].id = Id::new("Sequence 2 layer");
    let new = NewSequence::ids(&p);
    assert_eq!(
        (new.sequence.as_str(), new.layer.as_str()),
        ("Sequence 3", "Sequence 3 layer")
    );
}
