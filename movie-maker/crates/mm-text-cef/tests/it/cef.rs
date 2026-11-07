//! 実際の CEF で文字を描くテスト。`CEF_PATH` と `MM_CEF_HOST` が無ければスキップする（`MM_REQUIRE_CEF` があれば失敗）。
//! `MM_CEF_OUT` があればそこへ PNG を書く（目で確かめる）。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use mm_host_cef::CefLauncher;
use mm_text_api::{LineBreak, TextAlign, TextRasterizer, TextShaper, TextStroke, TextStyle};
use mm_text_cef::CefRasterizer;
use mm_text_glyphon::TextEngine;
use mm_types::{Rgba, RgbaImage};

fn host() -> Option<PathBuf> {
    let found = std::env::var_os("CEF_PATH").and(std::env::var_os("MM_CEF_HOST"));
    if found.is_none() {
        assert!(
            std::env::var_os("MM_REQUIRE_CEF").is_none(),
            "MM_REQUIRE_CEF があるのに CEF_PATH か MM_CEF_HOST が無い"
        );
        eprintln!("CEF_PATH か MM_CEF_HOST が無いので CEF のテストをスキップする");
    }
    found.map(PathBuf::from)
}

/// 透過が見えるよう、灰色の上に重ねて PNG に書く。
fn save(name: &str, image: &RgbaImage) {
    let Some(dir) = std::env::var_os("MM_CEF_OUT") else {
        return;
    };
    let path = PathBuf::from(dir).join(format!("{name}.png"));
    let mut board = RgbaImage::filled(image.size(), Rgba::new(0.3, 0.3, 0.3, 1.0));
    board.blit_over(image, 0, 0, 1.0);
    let rgb = mm_testkit::golden::to_srgb8(&board);
    let file = std::fs::File::create(&path).expect("PNG を作れる");
    let mut enc = png::Encoder::new(file, image.width(), image.height());
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&rgb))
        .expect("PNG を書ける");
    eprintln!("{}", path.display());
}

fn telop_style() -> TextStyle {
    TextStyle {
        font_family: "Yu Gothic UI".into(),
        size_px: 64.0,
        color: Rgba::from_straight(1.0, 0.83, 0.23, 1.0),
        max_width: Some(64.0 * 9.0),
        align: TextAlign::Center,
        line_break: LineBreak::Phrase,
        stroke: Some(TextStroke {
            color: Rgba::new(0.004, 0.006, 0.03, 1.0),
            width_px: 6.0,
        }),
        ..TextStyle::default()
    }
}

#[test]
fn 縁取りとルビと文節の折り返しを_css_で描く() {
    let Some(exe) = host() else { return };
    let shaper = TextEngine::new().shaper();
    let raster = CefRasterizer::new(Arc::new(CefLauncher), exe);
    let style = telop_style();
    let text = "｜空間系《くうかんけい》のエフェクトで奥行きを出してみた";
    let shaped = shaper.shape(text, &style).expect("組める");
    let start = Instant::now();
    let image = raster.rasterize(&shaped, &style, 1.0).expect("描ける");
    let first = start.elapsed();
    let start = Instant::now();
    let again = raster.rasterize(&shaped, &style, 1.0).expect("描ける");
    eprintln!(
        "文字 {}x{}: 最初 {:.1} ms（ホストの起動を含む）、2 回目 {:.1} ms",
        image.width(),
        image.height(),
        first.as_secs_f64() * 1000.0,
        start.elapsed().as_secs_f64() * 1000.0
    );
    save("text-cef-telop", &image);
    assert_eq!(image, again, "同じ入力なら同じ画素");
    assert_eq!(image.pixel(0, 0).a, 0.0, "角は透明");
    let dark = image
        .data()
        .chunks(4)
        .filter(|p| p[3] > 0.99 && p[0] < 0.01)
        .count();
    let yellow = image
        .data()
        .chunks(4)
        .filter(|p| p[3] > 0.99 && p[0] > 0.9)
        .count();
    assert!(
        dark > 500 && yellow > 500,
        "縁と字の色がある: 縁 {dark}、字 {yellow}"
    );
}

/// 不透明度が半分以上違う画素の割合。`a` の行 `a_y0..` と `b` の行 `b_y0..` を `rows` 行ずつ比べる。
fn differing(a: &RgbaImage, a_y0: u32, b: &RgbaImage, b_y0: u32, rows: u32) -> f64 {
    let width = a.width().min(b.width());
    let mut differ = 0usize;
    for y in 0..rows {
        for x in 0..width {
            let (pa, pb) = (a.pixel(x, a_y0 + y), b.pixel(x, b_y0 + y));
            differ += usize::from((pa.a - pb.a).abs() > 0.5);
        }
    }
    differ as f64 / f64::from(width * rows).max(1.0)
}

/// 不透明度が半分以上の画素の数（`y0..y1` の行）。
fn inked(image: &RgbaImage, y0: u32, y1: u32) -> usize {
    (y0..y1)
        .flat_map(|y| (0..image.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| image.pixel(x, y).a > 0.5)
        .count()
}

fn ruby_style() -> TextStyle {
    TextStyle {
        font_family: "Yu Gothic UI".into(),
        size_px: 64.0,
        ..TextStyle::default()
    }
}

/// 親文字と行がずれた画素の上限の割合。字の縁の丸めの差だけを許す。
const MAX_DIFFERING: f64 = 0.01;

#[test]
fn 長いふりがなでも親文字の行の幅と位置は変わらない() {
    let Some(exe) = host() else { return };
    let shaper = TextEngine::new().shaper();
    let raster = CefRasterizer::new(Arc::new(CefLauncher), exe);
    let style = ruby_style();
    let plain = shaper.shape("志を持つ", &style).expect("組める");
    let ruby = shaper
        .shape("志《こころざし》を持つ", &style)
        .expect("組める");
    let a = raster.rasterize(&plain, &style, 1.0).expect("描ける");
    let b = raster.rasterize(&ruby, &style, 1.0).expect("描ける");
    save("text-cef-ruby-long", &b);
    assert_eq!(a.width(), b.width(), "行の幅は親文字だけ");
    let room = (plain.bounds.y - ruby.bounds.y).round() as u32;
    let diff = differing(&a, 0, &b, room, a.height());
    assert!(diff < MAX_DIFFERING, "親文字の行がずれた: {diff}");
    assert!(inked(&b, 0, room) > 50, "ふりがなを上に描く");
}

/// 左右の端の列で、不透明度が半分以上の画素の数（`0..rows` の行）。
fn edge_ink(image: &RgbaImage, rows: u32) -> usize {
    let last = image.width() - 1;
    (0..rows)
        .flat_map(|y| [(0, y), (last, y)])
        .filter(|&(x, y)| image.pixel(x, y).a > 0.5)
        .count()
}

#[test]
fn 行の端の長いふりがなは画像の内側に寄せる() {
    let Some(exe) = host() else { return };
    let shaper = TextEngine::new().shaper();
    let raster = CefRasterizer::new(Arc::new(CefLauncher), exe);
    let style = ruby_style();
    let ruby = shaper
        .shape("志《こころざし》を持つ", &style)
        .expect("組める");
    let room = style.size_px * mm_text_api::RUBY_SCALE;
    for scale in [1.0, 2.0] {
        let image = raster.rasterize(&ruby, &style, scale).expect("描ける");
        save(&format!("text-cef-ruby-edge-{scale}x"), &image);
        let rows = (room * scale) as u32;
        assert!(inked(&image, 0, rows) > 50, "{scale} 倍: ふりがながある");
        assert_eq!(edge_ink(&image, rows), 0, "{scale} 倍: 端で切れない");
    }
}

#[test]
fn 二行目のルビは前の行に重ならない() {
    let Some(exe) = host() else { return };
    let shaper = TextEngine::new().shaper();
    let raster = CefRasterizer::new(Arc::new(CefLauncher), exe);
    let style = ruby_style();
    let plain = shaper
        .shape("一行目の文\n二行目の文", &style)
        .expect("組める");
    let ruby = shaper
        .shape("一行目の文\n｜二行目《にぎょうめ》の文", &style)
        .expect("組める");
    let a = raster.rasterize(&plain, &style, 1.0).expect("描ける");
    let b = raster.rasterize(&ruby, &style, 1.0).expect("描ける");
    save("text-cef-ruby-second-line", &b);
    let first = (plain.lines[1].rect.y - plain.bounds.y).floor() as u32;
    let diff = differing(&a, 0, &b, 0, first);
    assert!(diff < MAX_DIFFERING, "1 行目にふりがなが重なった: {diff}");
    let room = (ruby.lines[1].rect.y - plain.lines[1].rect.y).round() as u32;
    let diff = differing(&a, first, &b, first + room, a.height() - first);
    assert!(diff < MAX_DIFFERING, "2 行目の親文字がずれた: {diff}");
    assert!(
        inked(&b, first, first + room) > 50,
        "ふりがなを 2 行目の上に描く"
    );
}
