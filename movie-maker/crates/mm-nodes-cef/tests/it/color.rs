//! web.cef と PNG の素材が、同じ 8 bit の色を書き出しの符号化値（Rec.1886）で同じ値にするかのテスト。
//! 素材のデコーダーと書き出しは 8 bit の値を Rec.1886 の符号化値として扱うので、CSS の色も同じ値に戻る必要がある。
//! CEF のテストと同じく `CEF_PATH` と `MM_CEF_HOST` が要る。PNG の側は feature `ffmpeg` のときだけ比べる。
#![cfg(windows)]

use mm_node_api::NoInputs;
use mm_testkit::patches::{assert_close, encode, expected, PATCHES};
use mm_types::{ParamValue, Params, RationalTime, RgbaImage, Size};

use mm_nodes_cef::{DATA_PARAM, HTML_PARAM, STATIC_PARAM};
use mm_testkit::node::Harness;

use crate::cef::{gpu, host, kind, render_gpu};

/// 1 枚のパッチの辺（画素）。
const PATCH: u32 = 16;

fn size() -> Size {
    Size::new(PATCH * PATCHES.len() as u32, PATCH)
}

fn page() -> String {
    let divs: String = PATCHES
        .iter()
        .enumerate()
        .map(|(i, [r, g, b])| {
            format!(
                "<div style=\"position:absolute;top:0;left:{}px;width:{PATCH}px;height:{PATCH}px;\
                 background:rgb({r},{g},{b})\"></div>",
                i as u32 * PATCH
            )
        })
        .collect();
    format!(
        "<!doctype html><html><head><style>html, body {{ margin: 0; background: transparent; }}</style>\
         </head><body>{divs}<script>window.mmSetTime = () => {{}};</script></body></html>"
    )
}

fn params() -> Params {
    let mut p = Params::new();
    p.set(HTML_PARAM, ParamValue::Text(page()))
        .set(DATA_PARAM, ParamValue::Json(serde_json::json!({})))
        .set(STATIC_PARAM, ParamValue::Bool(true));
    p
}

/// 各パッチの中央の画素の符号化値。
fn codes(image: &RgbaImage) -> Vec<[i32; 3]> {
    (0..PATCHES.len() as u32)
        .map(|i| encode(image.pixel(i * PATCH + PATCH / 2, PATCH / 2)))
        .collect()
}

fn render_cef_gpu() -> Option<Vec<[i32; 3]>> {
    let exe = host()?;
    let mut gpu = gpu();
    let image = render_gpu(&kind(&exe), &mut gpu, &params(), RationalTime::ZERO, size());
    Some(codes(&image))
}

#[test]
fn web_cef_の純色は書き出しで同じ符号化値になる() {
    let Some(got) = render_cef_gpu() else { return };
    assert_close("web.cef（GPU）", &got, &expected());
}

#[test]
fn web_cef_の_cpu_の経路も同じ符号化値になる() {
    let Some(exe) = host() else { return };
    let kind = kind(&exe);
    let p = params().with_defaults(&mm_node_api::NodeKind::params(&kind));
    let (image, _) = Harness::new(None)
        .render(&kind, &p, RationalTime::ZERO, size(), 1.0, &mut NoInputs)
        .expect("CPU の経路で描ける");
    assert_close("web.cef（CPU）", &codes(&image), &expected());
}

#[cfg(feature = "ffmpeg")]
mod png {
    use std::path::{Path, PathBuf};

    use mm_decode_api::DecoderFactory;
    use mm_decode_ffmpeg::FfmpegDecoderFactory;
    use mm_testkit::patches::{assert_close, expected, PATCHES};

    use super::{codes, render_cef_gpu, size, PATCH};

    /// パッチを並べた RGB の PNG を書く。
    fn write_png(path: &Path) {
        let s = size();
        let mut rgb = Vec::with_capacity((s.width * s.height * 3) as usize);
        for _ in 0..s.height {
            for x in 0..s.width {
                rgb.extend_from_slice(&PATCHES[(x / PATCH) as usize]);
            }
        }
        let file = std::fs::File::create(path).expect("PNG を作れる");
        let mut enc = png::Encoder::new(file, s.width, s.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut w| w.write_image_data(&rgb))
            .expect("PNG を書ける");
    }

    fn decode_png() -> Vec<[i32; 3]> {
        let dir: PathBuf = std::env::temp_dir().join("mm-nodes-cef-color");
        std::fs::create_dir_all(&dir).expect("一時のフォルダー");
        let path = dir.join("patches.png");
        write_png(&path);
        let mut dec = FfmpegDecoderFactory
            .open(&path, None)
            .expect("PNG を開ける");
        let frame = dec.next_frame().expect("デコード").expect("フレームがある");
        codes(frame.image.as_ref().expect("CPU の画像"))
    }

    #[test]
    fn png_の素材の純色は書き出しで同じ符号化値になる() {
        assert_close("PNG", &decode_png(), &expected());
    }

    #[test]
    fn web_cef_と_png_の素材の純色が同じ符号化値になる() {
        let Some(cef) = render_cef_gpu() else { return };
        assert_close("web.cef と PNG", &cef, &decode_png());
    }
}
