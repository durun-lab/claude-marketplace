use mm_doc::{Cut, CutReframe, Node, Sequence, Transform, SEQUENCE_KIND, SEQUENCE_PARAM};
use mm_ops::OpContext;
use mm_types::{Rational, RationalTime};

use super::*;

fn secs(s: i64) -> RationalTime {
    RationalTime::seconds(s)
}

fn key(t: RationalTime, v: f64) -> Keyframe {
    Keyframe::new(t, ParamValue::Number(v), Interpolation::Linear)
}

/// title（不透明度のキーフレームが 1 秒と 4 秒）と、sequence `s`（k1: 素材の 10〜13 秒、k2: 素材の 20〜22 秒）を
/// 1 秒から置いた seq（k1 だけ再フレーミングの位置のキーフレームが素材の 11 秒）。k1 はコンポジションの 1〜4 秒に映る。
fn project() -> Project {
    let mut title = Layer::new("title", Node::new("text", 1), secs(0), secs(0), secs(8));
    title.opacity.keyframes = vec![key(secs(1), 0.0), key(secs(4), 1.0)];
    let mut seq = Layer::new(
        "seq",
        Node::new(SEQUENCE_KIND, 1),
        secs(1),
        secs(0),
        secs(5),
    );
    seq.source =
        Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s".into()));
    let mut reframe = Transform::default();
    reframe.position.keyframes = vec![Keyframe::new(
        secs(11),
        ParamValue::Vec2([0.0, 0.0]),
        Interpolation::Linear,
    )];
    seq.framing.cuts = vec![CutReframe {
        cut: Id::new("k1"),
        transform: reframe,
    }];
    let cut =
        |id: &str, a: i64, b: i64| Cut::new(id, Node::footage(&Id::new("a")), secs(a), secs(b));
    let mut p = Project::new();
    p.assets.push(mm_doc::Asset {
        id: Id::new("a"),
        path: "a.mp4".into(),
        content_hash: "hash-a".into(),
        color_space: mm_types::ColorSpace::bt709(),
        duration: secs(30),
        fps: Some(Rational::integer(30)),
        timebase_offset: RationalTime::ZERO,
    });
    p.sequences.push(Sequence {
        id: Id::new("s"),
        cuts: vec![cut("k1", 10, 13), cut("k2", 20, 22)],
        origin: None,
    });
    p.compositions.push(
        Composition::new(Id::new("c"), 64, 36, Rational::integer(30), secs(10))
            .with_layers(vec![seq, title]),
    );
    p
}

fn comp(p: &Project) -> &Composition {
    &p.compositions[0]
}

fn opacity_times(p: &Project) -> Vec<RationalTime> {
    let (_, l) = p.layer(&Id::new("title")).unwrap();
    l.opacity.keyframes.iter().map(|k| k.time).collect()
}

fn apply(p: &mut Project, op: Op) {
    op.apply(p, &OpContext::human()).unwrap();
}

#[test]
fn ドラッグを離した時刻へフレームの先頭にそろえて動かす() {
    let mut p = project();
    let op = move_op(&p, comp(&p), "title", (0, 0), 2.51).unwrap();
    let Op::MoveKeyframe(m) = &op else {
        panic!("{op:?}");
    };
    assert_eq!((m.target.path.as_str(), m.from), ("opacity", secs(1)));
    assert_eq!(
        m.keyframe.time,
        RationalTime::new(5, 2).unwrap(),
        "2.51 秒はいちばん近いフレームの 2.5 秒"
    );
    assert_eq!(
        m.keyframe.value,
        ParamValue::Number(0.0),
        "値と区間の形はそのまま"
    );
    apply(&mut p, op);
    assert_eq!(
        opacity_times(&p),
        [RationalTime::new(5, 2).unwrap(), secs(4)]
    );
    assert!(
        move_op(&p, comp(&p), "title", (0, 1), 4.0).is_none(),
        "時刻が変わらなければ操作にしない"
    );
}

#[test]
fn delete_は選んだキーフレームを外す() {
    let mut p = project();
    let op = remove_op(&p, comp(&p), "title", (0, 1)).unwrap();
    apply(&mut p, op);
    assert_eq!(opacity_times(&p), [secs(1)]);
    assert!(remove_op(&p, comp(&p), "title", (0, 5)).is_none());
}

#[test]
fn alt_クリックはその時刻の値で打ち_補間は直前のキーフレームを引き継ぐ() {
    let mut p = project();
    let op = add_op(&p, comp(&p), "title", 0, 2.0).unwrap();
    let Op::SetKeyframe(s) = &op else {
        panic!("{op:?}");
    };
    assert_eq!(s.keyframe.time, secs(2));
    let v = s.keyframe.value.as_number().unwrap();
    assert!(
        (v - 1.0 / 3.0).abs() < 1e-9,
        "1 秒の 0 から 4 秒の 1 への 1/3: {v}"
    );
    assert_eq!(s.keyframe.interpolation, Interpolation::Linear);
    apply(&mut p, op);
    assert_eq!(opacity_times(&p), [secs(1), secs(2), secs(4)]);
    assert!(
        add_op(&p, comp(&p), "title", 0, 4.0).is_none(),
        "キーフレームのある時刻には打たない"
    );
}

#[test]
fn 再フレーミングの行はカットを通して素材の時刻で動かし_打つ() {
    let mut p = project();
    let rows = keyframe_rows(&p, comp(&p), p.layer(&Id::new("seq")).unwrap().1);
    assert_eq!(rows[0].name, "Reframe › Position");
    assert_eq!(row_path(&p, comp(&p), "seq", 0), None, "Inspector に無い");
    assert_eq!(
        row_path(&p, comp(&p), "title", 0).as_deref(),
        Some("opacity")
    );
    let op = add_op(&p, comp(&p), "seq", 0, 3.0).unwrap();
    let Op::SetKeyframe(s) = &op else {
        panic!("{op:?}");
    };
    assert_eq!(s.target.path, "reframe.k1.position");
    assert_eq!(
        s.keyframe.time,
        secs(12),
        "コンポジションの 3 秒は k1 の素材の 12 秒"
    );
    apply(&mut p, op);
    assert!(
        add_op(&p, comp(&p), "seq", 0, 5.0).is_none(),
        "k2 には再フレーミングが無い"
    );
    let op = move_op(&p, comp(&p), "seq", (0, 0), 1.5).unwrap();
    let Op::MoveKeyframe(m) = &op else {
        panic!("{op:?}");
    };
    assert_eq!(
        (m.from, m.keyframe.time),
        (secs(11), RationalTime::new(21, 2).unwrap())
    );
}
