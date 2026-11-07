//! 頼みの鍵と偽の提供元の出力。

use mm_node_api::NodeKind;
use mm_nodes_genai::{
    FakeProvider, GenAiKind, GenRequest, Media, Provider, CHANNELS, FAKE_FPS, SAMPLE_RATE,
};
use mm_types::{Id, ParamValue, Params};

const REV: &str = "r1";

fn request(extra: &[(&str, ParamValue)]) -> GenRequest {
    let mut p = Params::new();
    for (k, v) in extra {
        p.set(*k, v.clone());
    }
    GenRequest::from_params(&p.with_defaults(&GenAiKind.params())).unwrap()
}

fn media(name: &str) -> (&'static str, ParamValue) {
    ("media", ParamValue::Choice(name.into()))
}

#[test]
fn 同じ頼みは同じ鍵で_プロンプトとシードと版で変わる() {
    let base = request(&[]);
    assert_eq!(base.key(REV), request(&[]).key(REV));
    let prompt = request(&[("prompt", ParamValue::Text("x".into()))]);
    let seed = request(&[("seed", ParamValue::Number(1.0))]);
    assert_ne!(base.key(REV), prompt.key(REV));
    assert_ne!(base.key(REV), seed.key(REV));
    assert_ne!(base.key(REV), base.key("r2"));
}

#[test]
fn 種類に関係しない値と素材の参照は鍵に入らない() {
    let image = request(&[]);
    let longer = request(&[("duration", ParamValue::Number(9.0))]);
    let placed = request(&[("asset", ParamValue::AssetRef(Id::new("gen_x")))]);
    assert_eq!(image.key(REV), longer.key(REV), "画像の長さ");
    assert_eq!(image.key(REV), placed.key(REV), "生成が書く asset");
    let audio = request(&[media("audio")]);
    let wide = request(&[media("audio"), ("width", ParamValue::Number(64.0))]);
    assert_eq!(audio.key(REV), wide.key(REV), "音の幅");
}

#[test]
fn 整数でない大きさと長さ_0_は断る() {
    let mut p = GenAiKind.params().defaults();
    p.set("width", ParamValue::Number(10.5));
    assert!(GenRequest::from_params(&p).is_err());
    let mut p = GenAiKind.params().defaults();
    p.set("media", ParamValue::Choice("video".into()));
    p.set("duration", ParamValue::Number(0.0));
    assert!(GenRequest::from_params(&p).is_err());
}

#[test]
fn 偽の画像は頼んだ大きさの_png_で決定的() {
    let req = request(&[
        ("width", ParamValue::Number(7.0)),
        ("height", ParamValue::Number(5.0)),
    ]);
    let fake = FakeProvider::new();
    let a = fake.generate(&req).unwrap();
    assert_eq!(a, fake.generate(&req).unwrap());
    assert_eq!(a.extension, "png");
    let info = png::Decoder::new(std::io::Cursor::new(&a.bytes))
        .read_info()
        .unwrap()
        .info()
        .clone();
    assert_eq!((info.width, info.height), (7, 5));
}

#[test]
fn 偽の音と動画は頼んだ長さ() {
    let fake = FakeProvider::new();
    let audio = request(&[media("audio"), ("duration", ParamValue::Number(0.5))]);
    let wav = fake.generate(&audio).unwrap();
    let header = 44;
    let frames = (wav.bytes.len() - header) / (usize::from(CHANNELS) * 2);
    assert_eq!(frames, SAMPLE_RATE as usize / 2);
    let video = request(&[
        media("video"),
        ("duration", ParamValue::Number(1.0)),
        ("width", ParamValue::Number(4.0)),
        ("height", ParamValue::Number(2.0)),
    ]);
    let y4m = fake.generate(&video).unwrap();
    let text = String::from_utf8_lossy(&y4m.bytes);
    assert_eq!(text.matches("FRAME\n").count(), FAKE_FPS as usize);
    assert_eq!(video.media, Media::Video);
}

#[test]
fn 偽の見積もりは料金表から() {
    let fake = FakeProvider::new();
    let video = request(&[media("video"), ("duration", ParamValue::Number(3.0))]);
    let e = fake.estimate(&video).unwrap();
    assert!((e.cost_usd - 0.30).abs() < 1e-9, "{e:?}");
}
