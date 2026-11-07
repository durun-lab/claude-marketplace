//! `text` の種類のパラメータ（縁取り、折り返し）が `TextStyle` に渡ること。縁とルビで字の位置が動かないこと。

use std::sync::Arc;

use mm_node_api::{FrameCx, NoInputs, NodeKind, Quality, Services};
use mm_nodes_basic::TextKind;
use mm_testkit::fixtures::sample_services;
use mm_testkit::gpu::CpuGpu;
use mm_testkit::text::SampleShaper;
use mm_text_api::{
    has_ruby, parse_ruby, LineBreak, ShapedText, TextShaper, TextStroke, TextStyle, RUBY_SCALE,
};
use mm_types::{ParamValue, Params, RationalTime, Rect, Result, Rgba, Size};

fn defaults() -> Params {
    TextKind::default().params().defaults()
}

#[test]
fn 既定のパラメータは_text_style_の既定と同じ() {
    assert_eq!(TextKind::style(&defaults()), TextStyle::default());
}

#[test]
fn 縁取りの色と幅が渡る() {
    let mut params = defaults();
    let red = Rgba::new(1.0, 0.0, 0.0, 1.0);
    params.set("stroke_color", ParamValue::Color(red));
    params.set("stroke_width", ParamValue::Number(3.0));
    assert_eq!(
        TextKind::style(&params).stroke,
        Some(TextStroke {
            color: red,
            width_px: 3.0
        })
    );
}

#[test]
fn 縁の幅が_0_なら縁なし() {
    let mut params = defaults();
    params.set("stroke_color", ParamValue::Color(Rgba::WHITE));
    assert_eq!(TextKind::style(&params).stroke, None);
}

#[test]
fn 折り返しの選択肢が_line_break_に渡る() {
    let mut params = defaults();
    params.set("max_width", ParamValue::Number(200.0));
    let style = TextKind::style(&params);
    assert_eq!(style.line_break, LineBreak::Anywhere);
    assert_eq!(style.max_width, Some(200.0));

    params.set("line_break", ParamValue::Choice("phrase".into()));
    let style = TextKind::style(&params);
    assert_eq!(style.line_break, LineBreak::Phrase);
    assert_eq!(style.max_width, Some(200.0));

    params.set("line_break", ParamValue::Choice("none".into()));
    assert_eq!(TextKind::style(&params).max_width, None, "折り返さない");
}

#[test]
fn 縁の太さは上限までに丸め_有限でなければ縁なし() {
    let mut params = defaults();
    params.set("stroke_width", ParamValue::Number(1e9));
    let stroke = TextKind::style(&params).stroke.expect("縁あり");
    assert_eq!(stroke.width_px, TextStroke::MAX_WIDTH_PX);
    params.set("stroke_width", ParamValue::Number(f64::NAN));
    assert_eq!(TextKind::style(&params).stroke, None);
}

/// 契約どおりに縁とふりがなの分だけ `bounds` を広げるシェーパー（モックのシェーパーは広げない）。
struct Widening;

impl TextShaper for Widening {
    fn id(&self) -> &'static str {
        "widening"
    }

    fn shape(&self, text: &str, style: &TextStyle) -> Result<ShapedText> {
        let mut shaped = SampleShaper.shape(text, style)?;
        let stroke = style.stroke.map_or(0.0, |s| s.clamped_width());
        let ruby = if has_ruby(&parse_ruby(text)) {
            style.size_px * RUBY_SCALE
        } else {
            0.0
        };
        let b = shaped.bounds;
        shaped.bounds = Rect::new(
            b.x - stroke,
            b.y - stroke - ruby,
            b.width + 2.0 * stroke,
            b.height + 2.0 * stroke + ruby,
        );
        Ok(shaped)
    }
}

/// `position` が (10, 10) の画像の領域。
fn region(params: &mut Params) -> Rect {
    params.set("position", ParamValue::Vec2([10.0, 10.0]));
    let size = Size::new(128, 72);
    let (mut inputs, mut gpu) = (NoInputs, CpuGpu::new());
    let services = Services {
        text_shaper: Arc::new(Widening),
        ..sample_services()
    };
    let cx = FrameCx {
        time: RationalTime::ZERO,
        params,
        size,
        region: size.to_rect(),
        scale: 1.0,
        quality: Quality::Full,
        inputs: &mut inputs,
        footage: None,
        source_time: RationalTime::ZERO,
        gpu: &mut gpu,
        services: &services,
    };
    TextKind::default().region_of_definition(&cx)
}

#[test]
fn 縁を付けても字の位置は動かない() {
    let mut params = defaults();
    let plain = region(&mut params);
    params.set("stroke_width", ParamValue::Number(4.0));
    let stroked = region(&mut params);
    assert_eq!(stroked.x, plain.x - 4.0, "縁の分だけ左へ広がる");
    assert_eq!(stroked.y, plain.y - 4.0, "縁の分だけ上へ広がる");
}

#[test]
fn ルビを付けても字の位置は動かない() {
    let mut params = defaults();
    params.set("text", ParamValue::Text("漢字".into()));
    let plain = region(&mut params);
    params.set("text", ParamValue::Text("漢字《かんじ》".into()));
    let ruby = region(&mut params);
    assert_eq!(ruby.x, plain.x);
    let size = TextKind::style(&params).size_px;
    assert_eq!(
        ruby.y,
        plain.y - size * RUBY_SCALE,
        "ふりがなの分だけ上へ広がる"
    );
}
