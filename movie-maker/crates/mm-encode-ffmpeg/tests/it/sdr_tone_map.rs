//! HDR の素材を SDR のコーデックで書くと、プレビューと同じトーンマップ（`bt2390_to_sdr`）が掛かること。
//! HDR の素材のフィクスチャ（`mm_testkit::hdr`）をデコーダーで線形にして書き、読み直した帯の値を、期待の線形の値に
//! トーンマップを掛けた値と比べる。8 bit の NV12（swscale）と 10 bit の ProRes の 2 つの符号化の経路を通す。

use std::path::{Path, PathBuf};

use mm_decode_api::{DecoderFactory, EncodeSettings, EncoderFactory};
use mm_decode_ffmpeg::FfmpegDecoderFactory;
use mm_encode_ffmpeg::FfmpegEncoderFactory;
use mm_testkit::hdr::{self, HdrKind};
use mm_types::tonemap::{bt2390_to_sdr, hdr_source_peak};
use mm_types::{ColorSpace, Rational, RgbaImage};

use crate::hdr::probe;

const FRAMES: usize = 3;

fn dir() -> PathBuf {
    let dir = std::env::temp_dir().join("mm_encode_ffmpeg_sdr_tone_map");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// PQ の素材のフィクスチャの最初のフレーム（線形）。
fn pq_frame() -> RgbaImage {
    let source = hdr::fixture(&dir(), HdrKind::Pq);
    let mut dec = FfmpegDecoderFactory.open(&source, None).unwrap();
    dec.next_frame().unwrap().unwrap().image.unwrap()
}

fn write(codec: &str, path: &Path, image: &RgbaImage, source_peak: f32) {
    let mut enc = FfmpegEncoderFactory.create().unwrap();
    let settings = EncodeSettings {
        size: hdr::SIZE,
        fps: Rational::integer(hdr::FPS),
        codec: codec.into(),
        output_path: path.to_owned(),
        audio: None,
        stream: None,
        source_peak,
    };
    enc.begin(&settings).unwrap();
    enc.push_frames(&vec![image.clone(); FRAMES]).unwrap();
    enc.finish().unwrap();
}

/// 読み直した帯の中央が、帯の期待の線形の値にトーンマップを掛けて SDR に切った値と合うことを確かめる。
/// 許容はフィクスチャの読み取りと同じ（`hdr::tolerance`）。切っただけなら SDR の白の付近の帯 1 が外れる。
fn check_bands(path: &Path, peak: f32) {
    let mut dec = FfmpegDecoderFactory.open(path, None).unwrap();
    let image = dec.next_frame().unwrap().unwrap().image.unwrap();
    for band in 0..hdr::BANDS.len() {
        let (x, y) = hdr::band_center(band);
        let p = image.pixel(x, y);
        let want = bt2390_to_sdr(hdr::expected(HdrKind::Pq, band), peak).map(|c| c.clamp(0.0, 1.0));
        for (got, want) in [p.r, p.g, p.b].into_iter().zip(want) {
            assert!(
                (got - want).abs() <= hdr::tolerance(want),
                "{} の帯 {band}: {got} 期待 {want}",
                path.display()
            );
        }
    }
}

#[test]
fn hdr_の素材を_sdr_で書くとプレビューと同じトーンマップが掛かる() {
    let image = pq_frame();
    let peak = hdr_source_peak(&[ColorSpace::bt2100_pq()]);
    for (codec, name) in [("h264", "h264.mp4"), ("prores_422hq", "prores.mov")] {
        let path = dir().join(name);
        write(codec, &path, &image, peak);
        check_bands(&path, peak);
    }
}

/// 4:2:0 の色差の位置（2x2 の中央）を書く。8 bit の SDR（swscale）と HDR（自前の変換）のどちらも。
#[test]
fn 書き出しは色差の位置を中央と宣言する() {
    let image = pq_frame();
    for (codec, name) in [("h264", "chroma_h264.mp4"), ("hevc_pq", "chroma_pq.mp4")] {
        let path = dir().join(name);
        write(codec, &path, &image, 1.0);
        let stream = probe(&path, &["-show_entries", "stream=chroma_location"]);
        assert!(
            stream.contains("chroma_location=center"),
            "{codec}: {stream}"
        );
    }
}
