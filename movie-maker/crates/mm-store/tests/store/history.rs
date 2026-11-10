//! 取り消しとやり直しが記録と競合の判定に見えること、最後のカットを抜いた空の sequence でも往復できること、
//! 壊れた DB から `import` で戻れること。

use std::fs;

use mm_doc::{Cut, Layer, Node, Project, Sequence, SEQUENCE_KIND, SEQUENCE_PARAM};
use mm_ops::{Extract, Insert, LockLayer, Op, OpContext, OpError, Transaction};
use mm_store::Store;
use mm_types::{Id, LayerId, ParamValue, Rational, RationalTime};

use crate::common::{actor, project_with, NOW};

fn lock(i: usize, lock: bool) -> Op {
    LockLayer {
        layer: LayerId::new(format!("l{i}")),
        lock,
    }
    .into()
}

fn apply(store: &mut Store, name: &str, ops: Vec<Op>, base: Option<u64>) -> mm_ops::OpResult<i64> {
    let mut txn = Transaction::new(name, ops);
    txn.base_version = base;
    store
        .apply_txn(txn, &OpContext::human(), actor(), NOW, None)
        .map(|t| t.doc_version)
}

#[test]
fn 取り消しの後も古いバージョンを元にした操作は競合で拒否される() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Store::create(tmp.path(), &project_with(2), NOW).unwrap();
    assert_eq!(
        apply(&mut s, "ロック", vec![lock(0, true)], None).unwrap(),
        1
    );
    s.undo_last(&OpContext::human(), actor(), NOW)
        .unwrap()
        .unwrap();
    assert_eq!(s.doc_version(), 2, "取り消しもバージョンを進める");

    // バージョン 1 を元にした l0 への操作は、バージョン 2 の取り消しと触る所が重なる。
    let err = apply(&mut s, "古い", vec![lock(0, true)], Some(1)).unwrap_err();
    assert!(matches!(err, OpError::Conflict { .. }), "{err:?}");

    s.redo_last(&OpContext::human(), actor(), NOW)
        .unwrap()
        .unwrap();
    assert_eq!(s.doc_version(), 3);
    let err = apply(&mut s, "古い", vec![lock(0, false)], Some(2)).unwrap_err();
    assert!(matches!(err, OpError::Conflict { .. }), "{err:?}");
    // l1 は誰も触っていない。
    assert_eq!(
        apply(&mut s, "別", vec![lock(1, true)], Some(1)).unwrap(),
        4
    );
}

#[test]
fn 適用したバージョンは取り消しても変わらず_diff_は取り消しと対で集約する() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Store::create(tmp.path(), &project_with(2), NOW).unwrap();
    apply(&mut s, "A", vec![lock(0, true)], None).unwrap();
    s.undo_last(&OpContext::human(), actor(), NOW)
        .unwrap()
        .unwrap();

    assert_eq!(
        s.txns_between(0, 1).unwrap().len(),
        1,
        "バージョン 1 には A が適用されている"
    );
    assert!(
        s.txns_between(0, 2).unwrap().is_empty(),
        "A とその取り消しは集約する"
    );
    let undo_only = s.txns_between(1, 2).unwrap();
    assert_eq!(undo_only.len(), 1);
    assert_eq!(
        undo_only[0].ops[0].record.op["lock"], false,
        "取り消しの行は逆操作"
    );

    s.redo_last(&OpContext::human(), actor(), NOW)
        .unwrap()
        .unwrap();
    assert_eq!(
        s.txns_between(0, 3).unwrap().len(),
        1,
        "やり直しは取り消しと畳んで A が残る"
    );
    assert_eq!(
        s.txns_between(2, 3).unwrap()[0].ops[0].record.op["lock"],
        true
    );
    assert_eq!(s.doc_version_at(NOW).unwrap(), 3);
    let log = s.txn_log().unwrap();
    assert_eq!(log.len(), 3);
    assert_eq!(log[0].doc_version, 1, "A のバージョンは書き換えない");
}

#[test]
fn 壊れた_db_でも_import_の置き換えで戻れる() {
    let tmp = tempfile::tempdir().unwrap();
    {
        let mut s = Store::create(tmp.path(), &project_with(2), NOW).unwrap();
        apply(&mut s, "A", vec![lock(0, true)], None).unwrap();
        s.export_versions().unwrap();
        s.close(NOW).unwrap();
    }
    let db = tmp.path().join("project.mmdb");
    let mut bytes = fs::read(&db).unwrap();
    for b in bytes.iter_mut().skip(100) {
        *b = 0xA5;
    }
    fs::write(&db, bytes).unwrap();
    assert!(Store::open(tmp.path(), NOW).is_err());

    let versions = tmp.path().join("versions");
    let restored = Store::import_versions(tmp.path(), &versions, true, NOW + 1000).unwrap();
    assert!(restored.project().compositions[0].layers[0].lock);
    let kept: Vec<String> = fs::read_dir(tmp.path().join("backups"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        kept.iter().any(|n| n.contains("broken")),
        "壊れたファイルは backups/ に残す: {kept:?}"
    );
}

/// 素材 a1 の 0 秒から 2 秒のカット k1 だけの sequence s1 と、それに追従している sequence のレイヤー ls を、
/// 長さ 2 秒のコンポジション c に置いたプロジェクト。
fn one_cut_project() -> Project {
    let mut p = project_with(0);
    let cut = Cut::new(
        "k1",
        Node::footage(&Id::new("a1")),
        RationalTime::ZERO,
        RationalTime::seconds(2),
    );
    p.sequences.push(Sequence {
        id: Id::new("s1"),
        cuts: vec![cut],
        origin: None,
    });
    let node =
        Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s1".into()));
    let zero = RationalTime::ZERO;
    let ls = Layer::new("ls", node, zero, zero, RationalTime::seconds(2));
    let comp = &mut p.compositions[0];
    comp.layers.push(ls);
    comp.duration = RationalTime::seconds(2);
    p
}

/// レイヤー ls とコンポジション c の長さ。
fn lengths(s: &Store) -> (RationalTime, RationalTime) {
    let (comp, layer) = s.project().layer(&Id::new("ls")).unwrap();
    (layer.duration().unwrap(), comp.duration)
}

#[test]
fn 最後のカットを抜いた空の_sequence_でも取り消しとやり直しを往復できる() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = Store::create(tmp.path(), &one_cut_project(), NOW).unwrap();
    let ctx = OpContext::human();
    let frame = RationalTime::from_frames(1, Rational::integer(30)).unwrap();
    let two = RationalTime::seconds(2);
    let extract = Extract { cut: Id::new("k1") };
    apply(&mut s, "extract", vec![extract.into()], None).unwrap();
    assert_eq!(
        lengths(&s),
        (frame, frame),
        "Store が足す fitLayer で 1 フレーム"
    );
    s.undo_last(&ctx, actor(), NOW).unwrap().unwrap();
    assert_eq!(lengths(&s), (two, two));
    s.redo_last(&ctx, actor(), NOW).unwrap().unwrap();
    assert!(s.project().sequences[0].cuts.is_empty());
    assert_eq!(
        lengths(&s),
        (frame, frame),
        "やり直しも空の sequence に戻す"
    );

    let three = RationalTime::seconds(3);
    let cut = Cut::new(
        "k2",
        Node::footage(&Id::new("a1")),
        RationalTime::ZERO,
        three,
    );
    let insert = Insert {
        sequence: Id::new("s1"),
        index: 0,
        cut,
    };
    apply(&mut s, "insert", vec![insert.into()], None).unwrap();
    assert_eq!(
        lengths(&s),
        (three, three),
        "空の sequence への insert に追従する"
    );
    s.undo_last(&ctx, actor(), NOW).unwrap().unwrap();
    assert_eq!(lengths(&s), (frame, frame));
    s.redo_last(&ctx, actor(), NOW).unwrap().unwrap();
    assert_eq!(lengths(&s), (three, three));
}
