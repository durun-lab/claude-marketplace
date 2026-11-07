//! ルビの注記、文節の切れ目の折り返し、縁取りの分の広がり。

use mm_text_api::{LineBreak, ShapedText, TextShaper, TextStroke, TextStyle};
use mm_text_glyphon::CosmicShaper;
use mm_types::Rgba;

/// 行ごとの元の文字列。グリフの位置から引く。
fn line_texts(shaped: &ShapedText, text: &str) -> Vec<String> {
    shaped
        .lines
        .iter()
        .map(|l| {
            l.glyphs
                .iter()
                .map(|g| &text[g.byte_offset..g.byte_offset + g.byte_len])
                .collect()
        })
        .collect()
}

fn style(max_width: Option<f32>, line_break: LineBreak) -> TextStyle {
    TextStyle {
        size_px: 40.0,
        max_width,
        line_break,
        ..TextStyle::default()
    }
}

#[test]
fn 文節の切れ目でだけ折り返す() {
    let shaper = CosmicShaper::new();
    let text = "このつまみを回すと空気感がガラッと変わります";
    let width = Some(40.0 * 10.0);
    let anywhere = shaper
        .shape(text, &style(width, LineBreak::Anywhere))
        .unwrap();
    let phrase = shaper
        .shape(text, &style(width, LineBreak::Phrase))
        .unwrap();
    assert_eq!(anywhere.text, text, "元の文字列を持つ");
    let lines = line_texts(&phrase, text);
    assert!(lines.len() >= 2, "{lines:?}");
    assert_eq!(lines.concat(), text, "注記の無い文は全部の文字が並ぶ");
    let first = &line_texts(&anywhere, text)[0];
    assert!(
        lines[0].len() <= first.len(),
        "文節で折ると 1 行目は短いか同じ: {lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|l| l.starts_with("す") || l.starts_with("感")),
        "文節の途中で折らない: {lines:?}"
    );
}

#[test]
fn ルビは親文字だけを組み_ふりがなの分だけ上に広げる() {
    let shaper = CosmicShaper::new();
    let plain = shaper
        .shape("東京へ行く", &style(None, LineBreak::Anywhere))
        .unwrap();
    let text = "｜東京《とうきょう》へ行く";
    let ruby = shaper
        .shape(text, &style(None, LineBreak::Anywhere))
        .unwrap();
    assert_eq!(line_texts(&ruby, text), ["東京へ行く"]);
    assert!(
        (ruby.bounds.width - plain.bounds.width).abs() < 0.5,
        "幅は親文字だけ"
    );
    let grown = ruby.bounds.height - plain.bounds.height;
    assert!(
        (grown - 20.0).abs() < 0.5,
        "字の半分だけ上に広がる: {grown}"
    );
    assert!(ruby.bounds.y < plain.bounds.y);
}

#[test]
fn 二行目以降のルビは行送りにふりがなの分を足す() {
    let shaper = CosmicShaper::new();
    let st = style(None, LineBreak::Anywhere);
    let plain = shaper.shape("一行目\n二行目\n三行目", &st).unwrap();
    let text = "一行目\n｜二行目《にぎょうめ》\n三行目";
    let ruby = shaper.shape(text, &st).unwrap();
    assert_eq!(line_texts(&ruby, text), ["一行目", "二行目", "三行目"]);
    let ruby_h = st.size_px * 0.5;
    let dy = |i: usize| ruby.lines[i].rect.y - plain.lines[i].rect.y;
    assert_eq!(dy(0), 0.0, "ルビの無い 1 行目は動かない");
    assert!(
        (dy(1) - ruby_h).abs() < 1e-3,
        "ルビの行から下がる: {}",
        dy(1)
    );
    assert!(
        (dy(2) - ruby_h).abs() < 1e-3,
        "後の行も同じだけ下がる: {}",
        dy(2)
    );
    let glyph = &ruby.lines[1].glyphs[0];
    assert!((glyph.origin[1] - plain.lines[1].glyphs[0].origin[1] - ruby_h).abs() < 1e-3);
    assert_eq!(
        ruby.bounds.y, plain.bounds.y,
        "1 行目にルビが無ければ上に広げない"
    );
    assert!((ruby.bounds.height - plain.bounds.height - ruby_h).abs() < 1e-3);
}

#[test]
fn ルビの行ごとにふりがなの分を重ねて足す() {
    let shaper = CosmicShaper::new();
    let st = style(None, LineBreak::Anywhere);
    let plain = shaper.shape("東京\nへ\n行く", &st).unwrap();
    let ruby = shaper
        .shape("東京《とうきょう》\nへ\n行《い》く", &st)
        .unwrap();
    let ruby_h = st.size_px * 0.5;
    let dy = |i: usize| ruby.lines[i].rect.y - plain.lines[i].rect.y;
    assert_eq!((dy(0), dy(1)), (0.0, 0.0), "1 行目の場所は上に取る");
    assert!((dy(2) - ruby_h).abs() < 1e-3, "{}", dy(2));
    assert!((plain.bounds.y - ruby.bounds.y - ruby_h).abs() < 1e-3);
    let grown = ruby.bounds.height - plain.bounds.height;
    assert!((grown - 2.0 * ruby_h).abs() < 1e-3, "{grown}");
}

#[test]
fn 縁取りの分だけ四方に広げる() {
    let shaper = CosmicShaper::new();
    let base = style(None, LineBreak::Anywhere);
    let stroked = TextStyle {
        stroke: Some(TextStroke {
            color: Rgba::BLACK,
            width_px: 6.0,
        }),
        ..base.clone()
    };
    let a = shaper.shape("Hello", &base).unwrap().bounds;
    let b = shaper.shape("Hello", &stroked).unwrap().bounds;
    assert!((b.width - a.width - 12.0).abs() < 1e-3 && (b.x - a.x + 6.0).abs() < 1e-3);
    assert!((b.height - a.height - 12.0).abs() < 1e-3);
}

#[test]
fn 縁の太さは上限までに丸め_有限でなければ縁なし() {
    let shaper = CosmicShaper::new();
    let base = style(None, LineBreak::Anywhere);
    let with = |width_px| TextStyle {
        stroke: Some(TextStroke {
            color: Rgba::BLACK,
            width_px,
        }),
        ..base.clone()
    };
    let a = shaper.shape("Hello", &base).unwrap().bounds;
    let huge = shaper.shape("Hello", &with(1e9)).unwrap().bounds;
    let max = TextStroke::MAX_WIDTH_PX;
    assert!((huge.width - a.width - 2.0 * max).abs() < 1e-3, "{huge:?}");
    let nan = shaper.shape("Hello", &with(f32::NAN)).unwrap().bounds;
    assert_eq!(nan, a);
}

#[test]
fn 親文字にならない注記の記号は文字のまま組む() {
    let shaper = CosmicShaper::new();
    let cases = [
        ("見て《ほしい》", "見て《ほしい》"),
        ("A｜B", "A｜B"),
        ("｜東京《とうきょう》都《と》へ", "東京都へ"),
    ];
    for (text, expected) in cases {
        let shaped = shaper
            .shape(text, &style(None, LineBreak::Anywhere))
            .unwrap();
        assert_eq!(line_texts(&shaped, text).concat(), expected, "{text}");
    }
}
