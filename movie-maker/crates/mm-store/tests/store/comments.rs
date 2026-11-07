//! コメントの読み書き、状態の移り変わり、対応のトランザクションとの結びつき、`review.json` への書き出し。

use std::fs;

use mm_ops::{Op, OpContext, Transaction};
use mm_store::{
    CommentFilter, CommentRange, CommentState, NewComment, Shape, ShapeKind, Store, VERSIONS_DIR,
};
use mm_types::RationalTime;

use crate::common::{project_with, NOW};
use crate::proposals::{claude, human, opacity};

fn at(secs: i64) -> NewComment {
    NewComment {
        comp: "c".into(),
        client_id: None,
        at: Some(RationalTime::seconds(secs)),
        range: None,
        text: format!("{secs} 秒の指摘"),
        drawing: vec![],
    }
}

fn range(a: i64, b: i64) -> NewComment {
    NewComment {
        comp: "c".into(),
        client_id: None,
        at: None,
        range: Some(CommentRange {
            start: RationalTime::seconds(a),
            out: RationalTime::seconds(b),
        }),
        text: format!("{a} から {b} 秒"),
        drawing: vec![],
    }
}

fn store(dir: &std::path::Path) -> Store {
    Store::create(dir, &project_with(3), NOW).unwrap()
}

fn respond(s: &mut Store, to: &str, ops: Vec<Op>) -> mm_store::StoredTxn {
    let txn = Transaction::new("Claude の対応", ops);
    s.apply_txn(
        txn,
        &OpContext::claude(),
        claude(),
        NOW + 5,
        Some(to.into()),
    )
    .unwrap()
}

#[test]
fn 作る_返信_状態_一覧の絞り込み() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    let a = s.add_comment(&at(2), &human(), NOW).unwrap();
    let b = s.add_comment(&range(4, 6), &human(), NOW + 1).unwrap();
    assert_eq!((a.id.as_str(), b.id.as_str()), ("c-1", "c-2"));
    assert_eq!(a.state, CommentState::Open);

    let a = s
        .reply_comment("c-1", &claude(), "直します", NOW + 2)
        .unwrap();
    assert_eq!(a.replies.len(), 1);
    assert_eq!(a.replies[0].id, "r-1");
    assert_eq!(a.state, CommentState::Open, "返信は状態を変えない");

    s.set_comment_state("c-2", CommentState::Done).unwrap();
    let open = CommentFilter {
        state: Some(CommentState::Open),
        ..Default::default()
    };
    let ids = |f: &CommentFilter| -> Vec<String> {
        s.list_comments(f)
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect()
    };
    assert_eq!(ids(&open), ["c-1"]);
    assert_eq!(ids(&CommentFilter::default()), ["c-1", "c-2"]);

    let overlap = |x, y| CommentFilter {
        overlaps: Some((RationalTime::seconds(x), RationalTime::seconds(y))),
        ..Default::default()
    };
    assert_eq!(ids(&overlap(1, 3)), ["c-1"], "時刻は点として");
    assert_eq!(ids(&overlap(5, 9)), ["c-2"], "範囲は区間として重なれば");
    assert!(ids(&overlap(7, 9)).is_empty());
    let other = CommentFilter {
        comp: Some("zz".into()),
        ..Default::default()
    };
    assert!(ids(&other).is_empty());
}

#[test]
fn 端末の_id_が同じ再送は二重に書かない() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    let new = NewComment {
        client_id: Some("dev-1".into()),
        ..at(1)
    };
    let first = s.add_comment(&new, &human(), NOW).unwrap();
    let again = s.add_comment(&new, &human(), NOW + 9).unwrap();
    assert_eq!(first, again);
    assert_eq!(s.list_comments(&CommentFilter::default()).unwrap().len(), 1);
}

#[test]
fn id_は_10_を超えても書いた順に並ぶ() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    for i in 0..11 {
        s.add_comment(&at(1), &human(), NOW + i).unwrap();
    }
    let ids: Vec<_> = s
        .list_comments(&CommentFilter::default())
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids.first().unwrap(), "c-1");
    assert_eq!(ids.last().unwrap(), "c-11");
}

#[test]
fn 不正なコメントは拒否する() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    let both = NewComment {
        range: range(1, 2).range,
        ..at(1)
    };
    let none = NewComment { at: None, ..at(1) };
    let backwards = range(5, 1);
    let empty = NewComment {
        text: " ".into(),
        ..at(1)
    };
    let no_comp = NewComment {
        comp: "zz".into(),
        ..at(1)
    };
    let outside = NewComment {
        drawing: vec![Shape {
            kind: ShapeKind::Point,
            points: vec![[1.5, 0.5]],
        }],
        ..at(1)
    };
    let short_rect = NewComment {
        drawing: vec![Shape {
            kind: ShapeKind::Rect,
            points: vec![[0.1, 0.1]],
        }],
        ..at(1)
    };
    for bad in [both, none, backwards, empty, no_comp, outside, short_rect] {
        assert!(s.add_comment(&bad, &human(), NOW).is_err(), "{bad:?}");
    }
    assert!(s
        .list_comments(&CommentFilter::default())
        .unwrap()
        .is_empty());
    assert!(s.reply_comment("c-9", &claude(), "x", NOW).is_err());
    assert!(s.set_comment_state("c-9", CommentState::Done).is_err());
}

#[test]
fn fresh_は人が直接は決められない() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    s.add_comment(&at(1), &human(), NOW).unwrap();
    assert!(s.set_comment_state("c-1", CommentState::Fresh).is_err());
    assert_eq!(s.comment("c-1").unwrap().state, CommentState::Open);
}

#[test]
fn 描き込みは足される() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    s.add_comment(&at(1), &human(), NOW).unwrap();
    let shape = Shape {
        kind: ShapeKind::Stroke,
        points: vec![[0.1, 0.1], [0.5, 0.6], [0.9, 0.2]],
    };
    s.attach_drawing("c-1", std::slice::from_ref(&shape))
        .unwrap();
    let c = s
        .attach_drawing("c-1", std::slice::from_ref(&shape))
        .unwrap();
    assert_eq!(c.drawing, [shape.clone(), shape]);
}

#[test]
fn 対応のトランザクションがコメントを_fresh_にして結びつく() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    s.add_comment(&at(1), &human(), NOW).unwrap();
    let txn = respond(&mut s, "c-1", vec![opacity(0, 0.5), opacity(1, 0.5)]);
    let c = s.comment("c-1").unwrap();
    assert_eq!(c.state, CommentState::Fresh);
    assert_eq!(c.responded_txn, Some(txn.id));
    assert_eq!(txn.responds_to.as_deref(), Some("c-1"));

    s.set_comment_state("c-1", CommentState::Open).unwrap();
    let again = respond(&mut s, "c-1", vec![opacity(2, 0.5)]);
    let c = s.comment("c-1").unwrap();
    assert_eq!(
        (c.state, c.responded_txn),
        (CommentState::Fresh, Some(again.id))
    );
}

#[test]
fn 済みのコメントは対応で動かない_存在しない_id_は対応だけ書く() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    s.add_comment(&at(1), &human(), NOW).unwrap();
    s.set_comment_state("c-1", CommentState::Wontfix).unwrap();
    respond(&mut s, "c-1", vec![opacity(0, 0.5)]);
    assert_eq!(s.comment("c-1").unwrap().state, CommentState::Wontfix);
    let t = respond(&mut s, "c-77", vec![opacity(1, 0.5)]);
    assert_eq!(t.responds_to.as_deref(), Some("c-77"));
}

#[test]
fn 提案の承認でコメントが_fresh_になり_提案だけでは動かない() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    s.add_comment(&at(1), &human(), NOW).unwrap();
    let txn = Transaction::new("Claude の対応", vec![opacity(0, 0.5)]);
    let p = s
        .propose(txn, &OpContext::claude(), claude(), NOW, Some("c-1".into()))
        .unwrap();
    assert_eq!(s.comment("c-1").unwrap().state, CommentState::Open);
    let (_, stored) = s.approve_proposal(p.id, None, &human(), NOW + 1).unwrap();
    let c = s.comment("c-1").unwrap();
    assert_eq!(
        (c.state, c.responded_txn),
        (CommentState::Fresh, Some(stored.id))
    );
}

#[test]
fn 書き出した_review_json_に_状態と返信が出る() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = store(tmp.path());
    s.add_comment(&at(1), &human(), NOW).unwrap();
    s.reply_comment("c-1", &claude(), "了解", NOW).unwrap();
    respond(&mut s, "c-1", vec![opacity(0, 0.5)]);
    s.export_versions().unwrap();
    let text = fs::read_to_string(tmp.path().join(VERSIONS_DIR).join("review.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let c = &v["comments"][0];
    assert_eq!(c["state"], "fresh");
    assert_eq!(c["replies"][0]["text"], "了解");
    assert_eq!(c["responded_txn"], 1);
}
