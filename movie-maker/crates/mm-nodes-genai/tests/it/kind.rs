//! `gen.ai` の種類: 契約、性質、長さ、描き方、音。

use std::sync::Arc;

use mm_node_api::{NoInputs, NodeKind};
use mm_nodes_genai::GenAiKind;
use mm_testkit::decode::{DecoderFrameSource, SampleDecoderFactory, SampleMedia};
use mm_testkit::node::{contract, render_audio, Harness, NodeFixture};
use mm_types::{ParamValue, Params, RationalTime, Rect, Size, TimeRange};

use mm_decode_api::DecoderFactory;

fn sample_source(media: &SampleMedia) -> Box<dyn mm_decode_api::FrameSource> {
    let f = SampleDecoderFactory {
        media: media.clone(),
    };
    Box::new(DecoderFrameSource::new(
        f.open(std::path::Path::new("gen.y4m"), None).unwrap(),
    ))
}

fn with(name: &str, v: ParamValue) -> Params {
    let mut p = Params::new();
    p.set(name, v);
    p
}

#[test]
fn 契約を満たす() {
    contract(|| Arc::new(GenAiKind), &NodeFixture::default());
}

#[test]
fn 素材を渡しても契約を満たす() {
    let fx = NodeFixture {
        footage: Some(SampleMedia::default()),
        ..NodeFixture::default()
    };
    contract(|| Arc::new(GenAiKind), &fx);
}

#[test]
fn offline_で音を持つ() {
    let t = GenAiKind.traits();
    assert!(t.offline && t.audio && t.pixel);
}

#[test]
fn 長さは動画と音だけ() {
    let k = GenAiKind;
    assert_eq!(k.duration(&Params::new()), None, "既定は画像");
    let video = with("media", ParamValue::Choice("video".into()));
    assert_eq!(k.duration(&video), Some(RationalTime::seconds(5)));
    let mut audio = with("media", ParamValue::Choice("audio".into()));
    audio.set("duration", ParamValue::Number(2.5));
    assert_eq!(k.duration(&audio), Some(RationalTime::new(5, 2).unwrap()));
}

#[test]
fn 素材が無ければ透明() {
    let mut h = Harness::new(None);
    let size = Size::new(16, 9);
    let (img, _) = h
        .render(
            &GenAiKind,
            &GenAiKind.params().defaults(),
            RationalTime::ZERO,
            size,
            1.0,
            &mut NoInputs,
        )
        .unwrap();
    assert_eq!(
        img.max_diff(&mm_types::RgbaImage::transparent(size)),
        Some(0.0)
    );
}

#[test]
fn 縦横比の違う素材は画布に収めて中央に置く() {
    let media = SampleMedia {
        size: Size::new(100, 100),
        ..SampleMedia::default()
    };
    let mut h = Harness::new(Some(sample_source(&media)));
    let (img, rod) = h
        .render(
            &GenAiKind,
            &GenAiKind.params().defaults(),
            RationalTime::ZERO,
            Size::new(160, 90),
            1.0,
            &mut NoInputs,
        )
        .unwrap();
    assert_eq!(img.size(), Size::new(160, 90), "CPU の画像は画布の大きさ");
    assert_eq!(rod, Rect::new(35.0, 0.0, 90.0, 90.0));
    assert_eq!(img.pixel(10, 45).a, 0.0, "左右の余白は透明");
    assert_eq!(img.pixel(80, 45).a, 1.0);
}

#[test]
fn 音は素材から出し_素材が無ければ無音() {
    let params = GenAiKind.params().defaults();
    let range = TimeRange::new(RationalTime::ZERO, RationalTime::new(1, 10).unwrap()).unwrap();
    let silent = render_audio(&GenAiKind, &params, range, None).unwrap();
    assert!(silent.is_silent());
    let mut src = sample_source(&SampleMedia::default());
    let tone = render_audio(&GenAiKind, &params, range, Some(src.as_mut())).unwrap();
    assert!(!tone.is_silent());
}
