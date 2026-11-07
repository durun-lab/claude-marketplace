//! CEF の要らないテスト。モックのホスト（`SampleLauncher`）で文字の契約を回し、ページに渡す値を確かめる。

use std::sync::Arc;

use mm_testkit::host::SampleLauncher;
use mm_testkit::text::{contract, SampleShaper};
use mm_text_api::{
    LineBreak, ShapedText, TextRasterizer, TextShaper, TextStroke, TextStyle, RUBY_SCALE,
};
use mm_text_cef::{page_params, CefRasterizer};
use mm_text_glyphon::TextEngine;
use mm_types::Rgba;

fn sample_rasterizer() -> Arc<CefRasterizer> {
    Arc::new(CefRasterizer::new(
        Arc::new(SampleLauncher),
        "sample".into(),
    ))
}

#[test]
fn モックのホストで文字の契約を満たす() {
    contract(|| Arc::new(SampleShaper), || sample_rasterizer());
    let engine = TextEngine::new();
    contract(|| engine.shaper(), || sample_rasterizer());
}

#[test]
fn 元の文字列が無い行組みの結果は描けない() {
    let mut shaped = SampleShaper.shape("abc", &TextStyle::default()).unwrap();
    shaped.text.clear();
    let err = sample_rasterizer()
        .rasterize(&shaped, &TextStyle::default(), 1.0)
        .unwrap_err();
    assert!(matches!(err, mm_types::MmError::Unsupported(_)), "{err:?}");
}

#[test]
fn ページには行ごとの文字とルビと縁取りを渡す() {
    let style = TextStyle {
        size_px: 40.0,
        max_width: Some(240.0),
        line_break: LineBreak::Phrase,
        stroke: Some(TextStroke {
            color: Rgba::BLACK,
            width_px: 4.0,
        }),
        font_family: "M PLUS Rounded 1c".into(),
        ..TextStyle::default()
    };
    let text = "｜東京《とうきょう》で曲を作っています";
    let shaped: ShapedText = TextEngine::new().shaper().shape(text, &style).unwrap();
    let p = page_params(&shaped, &style, 2.0);
    let lines = p["lines"].as_array().unwrap();
    assert!(lines.len() >= 2, "{p}");
    assert_eq!(lines[0]["segments"][0]["text"], "東京");
    assert_eq!(lines[0]["segments"][0]["ruby"], "とうきょう");
    let all: String = lines
        .iter()
        .flat_map(|l| l["segments"].as_array().unwrap().iter())
        .map(|s| s["text"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(all, "東京で曲を作っています", "注記を除いた文字が全部並ぶ");
    assert_eq!(p["style"]["stroke"]["width"], 4.0);
    assert_eq!(p["style"]["font"], "\"M PLUS Rounded 1c\", sans-serif");
    assert_eq!(p["scale"], 2.0);
    assert!(
        lines[0]["y"].as_f64().unwrap() > 4.0,
        "ふりがなと縁の分だけ下がる"
    );
    assert_eq!(p["style"]["ruby"], 20.0, "ふりがなの字の大きさは字の半分");
}

#[test]
fn 二行目のルビの行はふりがなの分だけ下がった位置を渡す() {
    let style = TextStyle {
        size_px: 40.0,
        ..TextStyle::default()
    };
    let shaper = TextEngine::new().shaper();
    let plain = page_params(
        &shaper.shape("一行目\n二行目", &style).unwrap(),
        &style,
        1.0,
    );
    let text = "一行目\n｜二行目《にぎょうめ》";
    let ruby = page_params(&shaper.shape(text, &style).unwrap(), &style, 1.0);
    let y = |p: &serde_json::Value, i: usize| p["lines"][i]["y"].as_f64().unwrap();
    assert_eq!(y(&ruby, 0), y(&plain, 0), "1 行目は動かない");
    let ruby_h = f64::from(style.size_px * RUBY_SCALE);
    assert!((y(&ruby, 1) - y(&plain, 1) - ruby_h).abs() < 1e-3);
    assert_eq!(ruby["lines"][1]["segments"][0]["ruby"], "にぎょうめ");
}
