use super::*;
use mm_doc::{Crop, Interpolation};
use serde_json::json;

fn media() -> HashMap<String, Media> {
    let m = |w, h| Media {
        size: Some([w, h]),
        duration: RationalTime::seconds(100),
        fps: None,
        still: true,
    };
    HashMap::from([
        ("cap".to_owned(), m(3840, 2160)),
        ("m0".to_owned(), m(1920, 1080)),
    ])
}

/// 4K の収録を半分に縮めて (100, 50) に置き、左を 400 画素切り抜いたレイヤー。
fn target() -> Layer {
    let node =
        Node::new("footage", 1).with_param(ASSET_PARAM, ParamValue::AssetRef(Id::new("cap")));
    let mut l = Layer::new(
        "v-0",
        node,
        RationalTime::seconds(2),
        RationalTime::ZERO,
        RationalTime::seconds(1),
    );
    l.transform.position = Param::constant(ParamValue::Vec2([100.0, 50.0]));
    l.transform.scale = Param::constant(ParamValue::Vec2([0.5, 0.5]));
    l.crop = Crop {
        left: Some(Param::constant(ParamValue::Number(400.0))),
        ..Crop::default()
    };
    l
}

fn run(clip: Value, layer: Layer) -> (Vec<Layer>, Report) {
    let mut report = Report::default();
    let out = split(&media(), &mut report, &clip, layer).unwrap();
    (out, report)
}

fn vec2(p: &Param) -> [f64; 2] {
    p.value.as_vec2().unwrap()
}

#[test]
fn 画像のマスクは輝度のトラックマットと_クリップの矩形に重なる画像のレイヤーにする() {
    let (out, report) = run(json!({"mask": {"asset": "m0", "invert": true}}), target());
    assert!(report.to_json().as_array().unwrap().is_empty());
    let [l, m] = &out[..] else {
        panic!("2 枚のはず: {out:?}")
    };
    let tm = l.track_matte.as_ref().unwrap();
    assert_eq!(tm.layer.as_str(), "v-0-matte");
    assert_eq!((tm.kind, tm.inverted), (MatteKind::Luma, true));
    assert_eq!(m.id.as_str(), "v-0-matte");
    assert_eq!(
        (m.start, m.duration().unwrap()),
        (l.start, l.duration().unwrap())
    );
    assert_eq!(m.source.text_param(FIT_PARAM), Some(FIT_NONE));
    // 矩形は x 400..3840（幅 3440）、y 0..2160。画像の 1920x1080 を引き伸ばし、レイヤーの変形を掛ける。
    assert_eq!(vec2(&m.transform.position), [100.0 + 0.5 * 400.0, 50.0]);
    assert_eq!(
        vec2(&m.transform.scale),
        [0.5 * 3440.0 / 1920.0, 0.5 * 2160.0 / 1080.0]
    );
}

#[test]
fn 動く変形はキーフレームごとに変換する() {
    let mut l = target();
    let key = |t: i64, v: [f64; 2]| {
        Keyframe::new(
            RationalTime::seconds(t),
            ParamValue::Vec2(v),
            Interpolation::Linear,
        )
    };
    l.transform.position.keyframes = vec![key(2, [0.0, 0.0]), key(3, [10.0, 20.0])];
    l.transform.scale.keyframes = vec![key(2, [1.0, 1.0]), key(3, [2.0, 2.0])];
    let (out, _) = run(json!({"mask": {"asset": "m0"}}), l);
    let p = &out[1].transform.position.keyframes;
    assert_eq!(p[1].value.as_vec2(), Some([10.0 + 2.0 * 400.0, 20.0]));
    let s = &out[1].transform.scale.keyframes;
    assert_eq!(s[0].value.as_vec2(), Some([3440.0 / 1920.0, 2.0]));
}

#[test]
fn 大きさの分からない画像のマスクは除いて数える() {
    let (out, report) = run(json!({"mask": {"asset": "missing"}}), target());
    assert_eq!(out.len(), 1);
    assert!(out[0].track_matte.is_none());
    assert_eq!(report.to_json()[0]["what"], "mask");
}

#[test]
fn 形のマスクとマスクの無いクリップはそのまま() {
    for clip in [json!({}), json!({"mask": {"radius": 10}})] {
        let (out, report) = run(clip, target());
        assert_eq!(out.len(), 1);
        assert!(report.to_json().as_array().unwrap().is_empty());
    }
}
