//! CEF の要らないテスト。モックのホスト（`SampleLauncher`）で文字の契約を回し、ページに渡す値を確かめる。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mm_host_api::{HostLauncher, HostProcess, HostRequest, HostResponse, HostSpec};
use mm_testkit::host::SampleLauncher;
use mm_testkit::text::{contract, SampleShaper};
use mm_text_api::{
    FontFile, LineBreak, ShapedText, TextRasterizer, TextShaper, TextStroke, TextStyle, RUBY_SCALE,
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

/// モックのホストに任せ、`Prepare` の `params` を覚えるランチャー。
#[derive(Default)]
struct Recording {
    prepared: Arc<Mutex<Vec<serde_json::Value>>>,
}

struct RecordingHost {
    inner: Box<dyn HostProcess>,
    prepared: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl HostProcess for RecordingHost {
    fn is_alive(&self) -> bool {
        self.inner.is_alive()
    }
    fn pid(&self) -> Option<u32> {
        self.inner.pid()
    }
    fn request(&mut self, req: &HostRequest, timeout: Duration) -> mm_types::Result<HostResponse> {
        if let HostRequest::Prepare { params } = req {
            self.prepared.lock().unwrap().push(params.clone());
        }
        self.inner.request(req, timeout)
    }
    fn kill(&mut self) -> mm_types::Result<()> {
        self.inner.kill()
    }
    fn restart(&mut self) -> mm_types::Result<()> {
        self.inner.restart()
    }
    fn open_handles(&self) -> usize {
        self.inner.open_handles()
    }
}

impl HostLauncher for Recording {
    fn id(&self) -> &'static str {
        "recording"
    }
    fn spawn(&self, spec: &HostSpec) -> mm_types::Result<Box<dyn HostProcess>> {
        Ok(Box::new(RecordingHost {
            inner: SampleLauncher.spawn(spec)?,
            prepared: self.prepared.clone(),
        }))
    }
}

#[test]
fn 書体の組をページと一緒に渡し_組が変わったら読み直す() {
    let launcher = Arc::new(Recording::default());
    let raster = CefRasterizer::new(launcher.clone(), "sample".into());
    let style = TextStyle::default();
    let shaped = SampleShaper.shape("abc", &style).unwrap();
    let font = FontFile::from_asset("p/fonts/A B/a.ttf", "h1");
    raster.use_fonts(std::slice::from_ref(&font)).unwrap();
    raster.rasterize(&shaped, &style, 1.0).unwrap();
    raster.use_fonts(std::slice::from_ref(&font)).unwrap();
    raster.rasterize(&shaped, &style, 1.0).unwrap();
    raster.use_fonts(&[]).unwrap();
    raster.rasterize(&shaped, &style, 1.0).unwrap();
    let prepared = launcher.prepared.lock().unwrap().clone();
    assert_eq!(prepared.len(), 2, "同じ組ならホストを起こし直さない");
    assert_eq!(prepared[0]["fonts"][0]["family"], "A B");
    assert_eq!(prepared[1]["fonts"], serde_json::json!([]));
}

#[test]
fn 描画のページは書体をホストが配る起点からだけ読む() {
    let page = include_str!("../../assets/text.html");
    assert!(page.contains(&format!("font-src {}\"", mm_host_api::FONT_ORIGIN)));
}
