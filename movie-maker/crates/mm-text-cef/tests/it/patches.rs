//! 文字の色の 16 進の値が、CEF で描く経路でも書き出しの符号化値に戻るかの検査。
//! 大きい「■」を描き、被覆率が 1 の画素の色を読む。CEF の検査と同じく `CEF_PATH` と `MM_CEF_HOST` が要る。

use std::sync::Arc;

use mm_host_cef::CefLauncher;
use mm_testkit::patches::{assert_close, color, encode, expected, solid_pixel, PATCHES};
use mm_text_api::{TextRasterizer, TextShaper, TextStyle};
use mm_text_cef::CefRasterizer;
use mm_text_glyphon::TextEngine;

use crate::cef::host;

/// 中が塗りつぶしになる大きさ（画素）。
const SIZE_PX: f32 = 48.0;

#[test]
fn cef_の文字の色は書き出しで同じ符号化値になる() {
    let Some(exe) = host() else { return };
    let shaper = TextEngine::new().shaper();
    let raster = CefRasterizer::new(Arc::new(CefLauncher), exe);
    let got: Vec<[i32; 3]> = PATCHES
        .iter()
        .map(|&rgb| {
            let st = TextStyle {
                font_family: "Yu Gothic".into(),
                size_px: SIZE_PX,
                color: color(rgb),
                ..TextStyle::default()
            };
            let shaped = shaper.shape("■", &st).expect("組める");
            encode(solid_pixel(
                &raster.rasterize(&shaped, &st, 1.0).expect("描ける"),
            ))
        })
        .collect();
    assert_close("text の cef", &got, &expected());
}
