//! 文字の色の 16 進の値が、CPU と glyphon（GPU）の両方の経路で書き出しの符号化値に戻るかの検査。
//! 大きい「■」を描き、被覆率が 1 の画素の色を読む。

use mm_testkit::patches::{assert_close, color, encode, expected, solid_pixel, PATCHES};
use mm_text_api::{TextRasterizer, TextShaper, TextStyle};
use mm_text_glyphon::TextEngine;

/// 中が塗りつぶしになる大きさ（画素）。
const SIZE_PX: f32 = 48.0;

fn codes(engine: &TextEngine, raster: &dyn TextRasterizer) -> Vec<[i32; 3]> {
    let shaper = engine.shaper();
    PATCHES
        .iter()
        .map(|&rgb| {
            let st = TextStyle {
                font_family: "Yu Gothic".into(),
                size_px: SIZE_PX,
                color: color(rgb),
                ..TextStyle::default()
            };
            let shaped = shaper.shape("■", &st).expect("シェーピング");
            encode(solid_pixel(
                &raster.rasterize(&shaped, &st, 1.0).expect("描ける"),
            ))
        })
        .collect()
}

#[test]
fn cpu_の文字の色は書き出しで同じ符号化値になる() {
    let engine = TextEngine::new();
    let got = codes(&engine, engine.rasterizer().as_ref());
    assert_close("text（CPU）", &got, &expected());
}

#[test]
fn glyphon_の文字の色は書き出しで同じ符号化値になる() {
    let Some(ctx) = crate::gpu::gpu() else { return };
    let engine = TextEngine::new();
    let got = codes(&engine, engine.gpu_rasterizer(ctx).as_ref());
    assert_close("text（glyphon）", &got, &expected());
}
