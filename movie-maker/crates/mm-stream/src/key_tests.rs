//! 配信の区間のキーが、編集で変わる区間だけ変わること。

use mm_doc::{
    Asset, Composition, Cut, Layer, MatteKind, Node, Project, Sequence, TrackMatte, Transition,
    SEQUENCE_KIND, SEQUENCE_PARAM,
};
use mm_runtime::Registry;
use mm_types::color::ColorSpace;
use mm_types::{Id, ParamValue, Rational, RationalTime};

use crate::hls::{HlsRequest, Plan};
use crate::key::{segment_key, KeyEnv, SegmentKey};
use crate::quality::HlsQuality;

const FPS: u32 = 30;

fn s(v: i64) -> RationalTime {
    RationalTime::seconds(v)
}

fn solid(color: &str) -> Node {
    Node::new("solid", 1).with_param("color", ParamValue::Text(color.into()))
}

/// 3 秒のカット 4 つ（a〜d）を並べたシーケンス `s` を置いた、12 秒のコンポジション `main`。
/// 区間は 0〜4、4〜8、8〜12 秒の 3 つ。
fn project() -> Project {
    let cuts = ["a", "b", "c", "d"]
        .iter()
        .map(|id| Cut::new(*id, solid(id), s(0), s(3)))
        .collect();
    let mut p = Project::new();
    p.sequences.push(Sequence {
        id: Id::new("s"),
        cuts,
        origin: None,
    });
    let seq = Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s".into()));
    let fps = Rational::integer(FPS);
    p.compositions.push(
        Composition::new(Id::new("main"), 1920, 1080, fps, s(12)).with_layers(vec![Layer::new(
            "seq",
            seq,
            s(0),
            s(0),
            s(12),
        )]),
    );
    p
}

fn keys_with(p: &Project, version: u64, quality: HlsQuality) -> Vec<SegmentKey> {
    keys_on(p, version, quality, "test")
}

fn keys_on(p: &Project, version: u64, quality: HlsQuality, backends: &str) -> Vec<SegmentKey> {
    let registry = Registry::new();
    let env = KeyEnv {
        project: p,
        nodes: &registry,
        backends,
        version,
    };
    let req = HlsRequest {
        composition: "main".into(),
        quality,
    };
    let plan = Plan::new(p, &req).unwrap();
    (0..plan.count())
        .map(|i| segment_key(&env, &plan, i).unwrap())
        .collect()
}

fn keys(p: &Project) -> Vec<SegmentKey> {
    keys_with(p, 1, HlsQuality::P360)
}

/// 区間ごとに、キーが同じか。
fn same(a: &[SegmentKey], b: &[SegmentKey]) -> Vec<bool> {
    a.iter().zip(b).map(|(x, y)| x == y).collect()
}

#[test]
fn 区間のキーは名前と往復し中身と範囲で決まる() {
    let k = keys(&project());
    assert_eq!(k.len(), 3);
    assert_ne!(k[0], k[1]);
    assert_eq!(k[0].file_name().parse::<SegmentKey>().unwrap(), k[0]);
    assert_eq!(k[0].to_string().parse::<SegmentKey>().unwrap(), k[0]);
    assert!("xyz.ts".parse::<SegmentKey>().is_err());
    assert_eq!(keys(&project()), k, "同じ中身なら同じキー");
    assert_eq!(
        keys_with(&project(), 9, HlsQuality::P360),
        k,
        "バージョンに依らない"
    );
    assert_ne!(keys_with(&project(), 1, HlsQuality::P540)[0], k[0]);
}

#[test]
fn デコーダーの画素の作り方のバージョンが上がると全区間のキーが変わる() {
    for (name, before_ids, after_ids) in [
        ("auto", ["d3d12va@1", "ffmpeg@1"], ["d3d12va@2", "ffmpeg@1"]),
        ("ffmpeg", ["ffmpeg", "ffmpeg@1"], ["ffmpeg", "ffmpeg@2"]),
    ] {
        let keys = |ids: &[&str]| {
            let label = crate::streamer::backend_label("decode", name, ids);
            keys_on(&project(), 1, HlsQuality::P360, &label)
        };
        let before = keys(&before_ids);
        assert_eq!(keys(&before_ids), before, "{name}");
        assert_eq!(
            same(&before, &keys(&after_ids)),
            [false, false, false],
            "{name}"
        );
    }
}

/// `project()` に、無効のレイヤー `off`（シーケンス `s2` の画像）を足す。`as_matte` なら `seq` のマットに使う。
fn with_disabled_layer(as_matte: bool) -> Project {
    let mut p = project();
    p.sequences.push(Sequence {
        id: Id::new("s2"),
        cuts: vec![Cut::new("x", solid("x"), s(0), s(12))],
        origin: None,
    });
    let node =
        Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s2".into()));
    let mut off = Layer::new("off", node, s(0), s(0), s(12));
    off.enabled = false;
    p.compositions[0].layers.push(off);
    if as_matte {
        p.compositions[0].layers[0].track_matte = Some(TrackMatte {
            layer: Id::new("off"),
            kind: MatteKind::Alpha,
            inverted: false,
        });
    }
    p
}

#[test]
fn 無効のレイヤーの中身を直してもキーは変わらず_マットに使われていれば変わる() {
    for (as_matte, expected) in [(false, [true, true, true]), (true, [false, false, false])] {
        let p = with_disabled_layer(as_matte);
        let mut edited = p.clone();
        edited.sequences[1].cuts[0].source = solid("changed");
        assert_eq!(
            same(&keys(&p), &keys(&edited)),
            expected,
            "matte={as_matte}"
        );
    }
}

#[test]
fn カットを直すと掛かる区間だけ変わる() {
    let before = keys(&project());
    let mut p = project();
    p.sequences[0].cuts[1].source = solid("changed");
    assert_eq!(same(&before, &keys(&p)), [false, false, true]);
}

#[test]
fn 後ろのカットを消して詰めても前の区間は変わらない() {
    let before = keys(&project());
    let mut p = project();
    p.sequences[0].cuts.pop();
    p.compositions[0].duration = s(9);
    p.compositions[0].layers[0].out_point = s(9);
    let after = keys(&p);
    assert_eq!(after.len(), 3);
    assert_eq!(same(&before, &after), [true, true, false]);
}

#[test]
fn 区間の中だけのレイヤーを直すとその区間だけ変わる() {
    let mut p = project();
    let title = Layer::new(
        "title",
        solid("red"),
        RationalTime::new(17, 2).unwrap(),
        s(0),
        s(1),
    );
    p.compositions[0].layers.push(title);
    let before = keys(&p);
    assert_eq!(same(&keys(&project()), &before), [true, true, false]);
    p.compositions[0].layers[1].source = solid("blue");
    assert_eq!(same(&before, &keys(&p)), [true, true, false]);
}

#[test]
fn トランジションは編集点の近くの区間だけを変える() {
    let before = keys(&project());
    let mut p = project();
    // c の先頭（6 秒）の 1 秒のトランジションは 5.5〜6.5 秒に掛かる。
    p.sequences[0].cuts[2].transition_in = Some(Transition {
        node: Node::new("crossfade", 1),
        duration: s(1),
    });
    assert_eq!(same(&before, &keys(&p)), [true, false, true]);
}

#[test]
fn 素材の中身が変わると使う区間が変わり_取り込み前の素材はバージョンで分ける() {
    let asset = |hash: &str| Asset {
        id: Id::new("clip"),
        path: "clip.mp4".into(),
        content_hash: hash.into(),
        color_space: ColorSpace::bt709(),
        duration: s(10),
        fps: None,
        timebase_offset: RationalTime::ZERO,
    };
    let mut p = project();
    p.assets.push(asset("h1"));
    p.sequences[0].cuts[3].source =
        Node::new("footage", 1).with_param("asset", ParamValue::AssetRef(Id::new("clip")));
    let before = keys(&p);
    p.assets[0] = asset("h2");
    assert_eq!(same(&before, &keys(&p)), [true, true, false]);

    p.assets[0] = asset("");
    let v1 = keys_with(&p, 1, HlsQuality::P360);
    let v2 = keys_with(&p, 2, HlsQuality::P360);
    assert_eq!(same(&v1, &v2), [true, true, false]);
}

/// SDR の区間はプロジェクトの素材から決まるトーンマップの上限で符号化するので、どの区間にも置いていない
/// HDR の素材を足すと全区間のキーが変わる。SDR の素材なら変わらない。
#[test]
fn hdr_の素材を足すと区間に掛からなくても全区間のキーが変わる() {
    let unused = |color_space: ColorSpace| Asset {
        id: Id::new("unused"),
        path: "unused.mp4".into(),
        content_hash: "h".into(),
        color_space,
        duration: s(10),
        fps: None,
        timebase_offset: RationalTime::ZERO,
    };
    let before = keys(&project());
    let mut sdr = project();
    sdr.assets.push(unused(ColorSpace::bt709()));
    assert_eq!(same(&before, &keys(&sdr)), [true, true, true]);
    let mut hdr = project();
    hdr.assets.push(unused(ColorSpace::bt2100_pq()));
    assert_eq!(same(&before, &keys(&hdr)), [false, false, false]);
}
