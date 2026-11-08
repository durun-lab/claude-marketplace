//! プロジェクトのデータの不変条件（`Project::validate`）のテスト。

use mm_types::{ParamValue, Rational, RationalTime};

use mm_doc::{
    Composition, Cut, Interpolation, Keyframe, Layer, Marker, Mask, MaskRect, MatteKind, Node,
    Project, RenderTarget, Sequence, TimeKeyframe, TimeRemap, TrackMatte, NESTED_KIND,
    NESTED_PARAM,
};
use mm_types::{Id, MmError, Result};

fn comp(id: &str, layers: Vec<Layer>) -> Composition {
    Composition::new(
        Id::new(id),
        16,
        9,
        Rational::integer(30),
        RationalTime::seconds(2),
    )
    .with_layers(layers)
}

fn layer(id: &str, node: Node) -> Layer {
    Layer::new(
        id,
        node,
        RationalTime::ZERO,
        RationalTime::ZERO,
        RationalTime::seconds(1),
    )
}

fn nested(target: &str) -> Node {
    Node::new(NESTED_KIND, 1).with_param(NESTED_PARAM, ParamValue::Text(target.into()))
}

fn project(comps: Vec<Composition>) -> Project {
    Project {
        compositions: comps,
        ..Project::new()
    }
}

fn is_invalid(r: Result<()>) -> bool {
    matches!(r, Err(MmError::InvalidArgument(_)))
}

#[test]
fn キャッシュの外を指すコンテンツハッシュを拒否する() {
    let asset = |hash: &str| mm_doc::Asset {
        id: Id::new("a"),
        path: "a.mp4".into(),
        content_hash: hash.into(),
        color_space: mm_types::ColorSpace::bt709(),
        duration: RationalTime::seconds(1),
        fps: None,
        timebase_offset: RationalTime::ZERO,
    };
    for bad in ["..\\..\\secret", "../x", "a/b", "a.b"] {
        let mut p = project(Vec::new());
        p.assets.push(asset(bad));
        assert!(is_invalid(p.validate()), "{bad} は拒否する");
    }
    for good in ["0123abcdef", "stage0-cfr", ""] {
        let mut p = project(Vec::new());
        p.assets.push(asset(good));
        assert!(p.validate().is_ok(), "{good} は受ける");
    }
}

#[test]
fn 入れ子の循環を拒否する() {
    let p = project(vec![
        comp("a", vec![layer("la", nested("b"))]),
        comp("b", vec![layer("lb", nested("a"))]),
    ]);
    assert!(is_invalid(p.validate()));
    let ok = project(vec![
        comp("a", vec![layer("la", nested("b"))]),
        comp("b", vec![layer("lb", Node::new("solid", 1))]),
    ]);
    ok.validate().unwrap();
}

#[test]
fn 無い入れ子と無いレンダーターゲットの先を拒否する() {
    let p = project(vec![comp("a", vec![layer("la", nested("zzz"))])]);
    assert!(is_invalid(p.validate()));
    let mut p = project(vec![comp("a", vec![])]);
    p.render_targets.push(RenderTarget {
        id: Id::new("t"),
        composition: Id::new("nope"),
        width: 16,
        height: 9,
        fps: Rational::integer(30),
        codec: "sample".into(),
        output_path: "x".into(),
        loudness_lufs: None,
        delivery: Default::default(),
    });
    assert!(is_invalid(p.validate()));
}

#[test]
fn id_の重複を拒否する() {
    let p = project(vec![
        comp("a", vec![layer("l", Node::new("solid", 1))]),
        comp("b", vec![layer("l", Node::new("solid", 1))]),
    ]);
    assert!(is_invalid(p.validate()));
}

#[test]
fn in_が_out_以上なら拒否する() {
    let mut l = layer("l", Node::new("solid", 1));
    l.out_point = RationalTime::ZERO;
    assert!(is_invalid(project(vec![comp("a", vec![l])]).validate()));
}

#[test]
fn 矩形のマスクの型を確かめる() {
    let with_rect = |edit: &dyn Fn(&mut MaskRect)| {
        let mut l = layer("l", Node::new("solid", 1));
        let mut rect = MaskRect::new([0.0, 0.0], [10.0, 10.0], 0.0);
        edit(&mut rect);
        l.masks.push(Mask::rect("m", rect));
        project(vec![comp("a", vec![l])]).validate()
    };
    assert!(with_rect(&|_| {}).is_ok());
    assert!(is_invalid(with_rect(&|r| {
        r.size.value = ParamValue::Number(10.0)
    })));
    assert!(is_invalid(with_rect(&|r| {
        r.position.value = ParamValue::Number(1.0)
    })));
    assert!(is_invalid(with_rect(&|r| {
        r.radius.value = ParamValue::Vec2([1.0, 1.0])
    })));
    assert!(is_invalid(with_rect(&|r| {
        r.size.keyframes = vec![Keyframe::new(
            RationalTime::ZERO,
            ParamValue::Number(1.0),
            Interpolation::Linear,
        )]
    })));
}

#[test]
fn キーフレームの時刻は昇順() {
    let mut l = layer("l", Node::new("solid", 1));
    l.opacity.keyframes = vec![
        Keyframe::new(
            RationalTime::seconds(1),
            ParamValue::Number(1.0),
            Interpolation::Linear,
        ),
        Keyframe::new(
            RationalTime::seconds(1),
            ParamValue::Number(0.0),
            Interpolation::Linear,
        ),
    ];
    assert!(is_invalid(project(vec![comp("a", vec![l])]).validate()));
}

#[test]
fn 親の先が在り循環しない() {
    let mut a = layer("a", Node::new("solid", 1));
    let mut b = layer("b", Node::new("solid", 1));
    a.parent = Some(Id::new("b"));
    b.parent = Some(Id::new("a"));
    assert!(is_invalid(
        project(vec![comp("c", vec![a.clone(), b])]).validate()
    ));
    a.parent = Some(Id::new("nope"));
    assert!(is_invalid(project(vec![comp("c", vec![a])]).validate()));
}

fn matted(mut l: Layer, matte: &str) -> Layer {
    l.track_matte = Some(TrackMatte {
        layer: Id::new(matte),
        kind: MatteKind::Alpha,
        inverted: false,
    });
    l
}

#[test]
fn トラックマットの先が在り循環しない() {
    let plain = |id: &str| layer(id, Node::new("solid", 1));
    let check = |layers: Vec<Layer>| project(vec![comp("c", layers)]).validate();
    // 連鎖は循環でなければ通る。
    check(vec![
        matted(plain("a"), "b"),
        matted(plain("b"), "c"),
        plain("c"),
    ])
    .unwrap();
    let two = check(vec![matted(plain("a"), "b"), matted(plain("b"), "a")]);
    assert!(is_invalid(two));
    // 循環に外から入るレイヤーがあっても見つける。
    let tail = check(vec![
        matted(plain("x"), "a"),
        matted(plain("a"), "b"),
        matted(plain("b"), "a"),
    ]);
    assert!(is_invalid(tail));
    assert!(is_invalid(check(vec![matted(plain("a"), "a")])));
    assert!(is_invalid(check(vec![matted(plain("a"), "nope")])));
}

#[test]
fn 接続先のカットとタイムリマップを確かめる() {
    let mut l = layer("l", Node::new("solid", 1));
    l.attach_to = Some(Id::new("cut1"));
    let mut p = project(vec![comp("c", vec![l])]);
    assert!(is_invalid(p.validate()));
    p.sequences.push(Sequence {
        id: Id::new("s"),
        cuts: vec![Cut::new(
            "cut1",
            Node::new("solid", 1),
            RationalTime::ZERO,
            RationalTime::seconds(1),
        )],
        origin: None,
    });
    p.validate().unwrap();
    p.compositions[0].layers[0].time_remap = Some(TimeRemap {
        keyframes: vec![TimeKeyframe {
            time: RationalTime::ZERO,
            source: RationalTime::ZERO,
            interpolation: Interpolation::Bezier,
        }],
    });
    assert!(is_invalid(p.validate()));
}

#[test]
fn マーカーは昇順() {
    let mut c = comp("c", vec![]);
    c.markers = vec![
        Marker {
            id: Id::new("m1"),
            time: RationalTime::seconds(1),
            duration: RationalTime::ZERO,
            name: "a".into(),
        },
        Marker {
            id: Id::new("m2"),
            time: RationalTime::ZERO,
            duration: RationalTime::ZERO,
            name: "b".into(),
        },
    ];
    assert!(is_invalid(project(vec![c]).validate()));
}
