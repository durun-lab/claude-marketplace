use super::*;
use mm_doc::{
    CutReframe, Effect, Interpolation, Keyframe, Mask, MaskRect, Node, SEQUENCE_KIND,
    SEQUENCE_PARAM,
};
use mm_types::{Id, ParamValue, Rational};

fn secs(s: i64) -> RationalTime {
    RationalTime::seconds(s)
}

fn key(t: RationalTime) -> Keyframe {
    Keyframe::new(t, ParamValue::Number(1.0), Interpolation::Linear)
}

fn comp(layers: Vec<Layer>) -> Composition {
    Composition::new(Id::new("c"), 64, 36, Rational::integer(30), secs(10)).with_layers(layers)
}

fn text(id: &str) -> Layer {
    Layer::new(id, Node::new("text", 1), secs(0), secs(0), secs(8))
}

fn names(rows: &[KeyframeRowSource]) -> Vec<String> {
    rows.iter().map(|r| r.name.clone()).collect()
}

#[test]
fn キーフレームのあるパラメータだけを_inspector_の区画の順に並べる() {
    let mut l = text("title");
    l.transform.position.keyframes = vec![key(secs(2))];
    l.opacity.keyframes = vec![key(secs(1)), key(secs(4))];
    l.source.params.insert(
        "size".into(),
        Param {
            keyframes: vec![key(secs(3))],
            ..Param::constant(ParamValue::Number(10.0))
        },
    );
    let mut blur = Node::new("blur", 1).with_param("radius", ParamValue::Number(2.0));
    blur.params.get_mut("radius").unwrap().keyframes = vec![key(secs(5))];
    l.effects = vec![Effect {
        id: Id::new("fx"),
        node: blur,
        enabled: true,
    }];
    let mut mask = Mask::rect("m1", MaskRect::new([0.0, 0.0], [8.0, 8.0], 0.0));
    mask.feather.keyframes = vec![key(secs(6))];
    l.masks = vec![mask];
    let c = comp(vec![l.clone()]);
    let project = mm_doc::Project::new();
    let rows = keyframe_rows(&project, &c, &l);
    assert_eq!(
        names(&rows),
        [
            "Opacity",
            "Transform › Position",
            "Source › size",
            "Effect: blur › radius",
            "Mask: m1 › Feather"
        ]
    );
    let opacity = &rows[0];
    assert_eq!(opacity.spans.len(), 1);
    assert_eq!(opacity.spans[0].path, "opacity");
    assert_eq!((opacity.spans[0].start, opacity.spans[0].end), (0.0, 10.0));
    let comps: Vec<f64> = opacity.keyframes.iter().map(|k| k.comp).collect();
    assert_eq!(comps, [1.0, 4.0]);
    assert_eq!(rows[4].spans[0].path, "masks.m1.feather");
}

#[test]
fn キーフレームの無いレイヤーは行を持たない() {
    let l = text("plain");
    let c = comp(vec![l.clone()]);
    assert!(keyframe_rows(&mm_doc::Project::new(), &c, &l).is_empty());
}

/// sequence `s`（k1: 素材の 10〜13 秒、k2: 素材の 20〜22 秒）と、それを 1 秒から置いた sequence のレイヤー。
/// k1 はコンポジションの 1〜4 秒、k2 は 4〜6 秒に映る。
fn reframed() -> (mm_doc::Project, Composition) {
    let cut = |id: &str, a: i64, b: i64| {
        mm_doc::Cut::new(id, Node::footage(&Id::new("a")), secs(a), secs(b))
    };
    let mut project = mm_doc::Project::new();
    project.sequences.push(Sequence {
        id: Id::new("s"),
        cuts: vec![cut("k1", 10, 13), cut("k2", 20, 22)],
        origin: None,
    });
    let mut seq = Layer::new(
        "seq",
        Node::new(SEQUENCE_KIND, 1),
        secs(1),
        secs(0),
        secs(5),
    );
    seq.source =
        Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s".into()));
    let mut first = Transform::default();
    // 素材の 9 秒はカットの範囲（10〜13 秒）の外なので、行に出さない。
    first.position.keyframes = vec![
        key(secs(9)),
        key(secs(11)),
        key(RationalTime::new(25, 2).unwrap()),
    ];
    let mut second = Transform::default();
    second.position.keyframes = vec![key(secs(21))];
    seq.framing.cuts = vec![
        CutReframe {
            cut: Id::new("k1"),
            transform: first,
        },
        CutReframe {
            cut: Id::new("k2"),
            transform: second,
        },
    ];
    let c = comp(vec![seq]);
    (project, c)
}

#[test]
fn 再フレーミングはカットを通してコンポジションの時刻に置き_全部のカットを_1_本にまとめる() {
    let (project, c) = reframed();
    let layer = &c.layers[0];
    let rows = keyframe_rows(&project, &c, layer);
    assert_eq!(names(&rows), ["Reframe › Position"]);
    let row = &rows[0];
    let paths: Vec<&str> = row.spans.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, ["reframe.k1.position", "reframe.k2.position"]);
    let spans: Vec<(f64, f64)> = row.spans.iter().map(|s| (s.start, s.end)).collect();
    assert_eq!(spans, [(1.0, 4.0), (4.0, 6.0)]);
    let keys: Vec<(usize, usize, f64)> = row
        .keyframes
        .iter()
        .map(|k| (k.span, k.index, k.comp))
        .collect();
    assert_eq!(
        keys,
        [(0, 1, 2.0), (0, 2, 3.5), (1, 0, 5.0)],
        "素材の 9 秒は外す"
    );
    assert_eq!(
        row.spans[0].param_time(layer, secs(3)).unwrap(),
        secs(12),
        "コンポジションの 3 秒は素材の 12 秒"
    );
    assert_eq!(row.spans[1].param_time(layer, secs(5)).unwrap(), secs(21));
}

#[test]
fn 動かせる範囲はパラメータの範囲の最後のフレームまで_マーカーは再フレーミングも含む() {
    let (project, c) = reframed();
    let layer = &c.layers[0];
    let rows = keyframe_rows(&project, &c, layer);
    let layout = layout_rows(&rows, 30.0);
    let k = &layout[0].keyframes;
    assert_eq!((k[0].min, k[0].max), (1.0, 4.0 - 1.0 / 30.0));
    assert_eq!((k[2].min, k[2].max), (4.0, 6.0 - 1.0 / 30.0));
    assert_eq!(&*marker_times(layer, &rows), [2.0, 3.5, 5.0]);
}

#[test]
fn タイムリマップのある_sequence_のレイヤーは再フレーミングの行を出さない() {
    let (project, mut c) = reframed();
    c.layers[0].time_remap = Some(mm_doc::TimeRemap {
        keyframes: vec![
            mm_doc::TimeKeyframe {
                time: secs(0),
                source: secs(0),
                interpolation: Interpolation::Linear,
            },
            mm_doc::TimeKeyframe {
                time: secs(4),
                source: secs(4),
                interpolation: Interpolation::Linear,
            },
        ],
    });
    assert!(keyframe_rows(&project, &c, &c.layers[0]).is_empty());
}

#[test]
fn timeline_のモデルはキーフレームの行とマーカーを持つ() {
    let (project, c) = reframed();
    let m = super::super::timeline_model(&project, &c);
    let item = &m.items[0];
    assert_eq!(item.keyframe_rows.len(), 1);
    assert_eq!(&*item.keyframe_rows[0].name, "Reframe › Position");
    assert_eq!(&*item.keyframes, [2.0, 3.5, 5.0]);
}
