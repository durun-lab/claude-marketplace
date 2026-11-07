//! `mm-testkit` のデコーダーの契約を、実素材で通す。

use crate::common;

use std::sync::Arc;

use mm_decode_api::DecoderFactory;
use mm_decode_ffmpeg::FfmpegDecoderFactory;
use mm_testkit::decode::{contract, DecodeFixture};
use mm_testkit::hdr::{self, HdrKind};
use mm_types::{Rational, RationalTime};

#[test]
fn ffmpeg_デコーダーが契約を通る() {
    let dir = common::fixtures();
    let fx = DecodeFixture {
        path: dir.join("cfr.mp4"),
        duration: RationalTime::seconds(2),
        fps: Rational::integer(30),
        broken_path: Some(dir.join("broken.mp4")),
    };
    contract(|| Arc::new(FfmpegDecoderFactory), &fx);
}

/// HEVC Main10 の PQ と HLG を、8 bit を経由せずに 1 を超える線形の光（作業色空間）で読む。CPU の経路。
#[test]
fn pq_と_hlg_の素材を_8_bit_を経由せず線形の光で読む() {
    for kind in [HdrKind::Pq, HdrKind::Hlg] {
        let path = hdr::fixture(&common::fixtures(), kind);
        let mut dec = FfmpegDecoderFactory.open(&path, None).expect("開ける");
        assert_eq!(dec.info().color_space.0, kind.color_space());
        let frame = dec.next_frame().unwrap().expect("1 フレーム目");
        let image = frame.image.expect("CPU の画像");
        hdr::check_bands(&image, kind, "CPU").unwrap();
        eprintln!(
            "CPU の {kind:?} の帯の最大の相対の差 {:e}",
            hdr::max_relative_error(&image, kind)
        );
    }
}

/// 広い信号の素材だけ実装の名前に画素の版を付け、フレームのキャッシュで前の作り方の画素を引かない。
/// 8 bit の Rec.709 の素材は名前を変えず、覚えた画像を使い続ける。
#[test]
fn 広い信号の素材だけ実装の名前に画素の版を付ける() {
    let dir = common::fixtures();
    let id = |path: &std::path::Path| FfmpegDecoderFactory.open(path, None).unwrap().backend_id();
    let wide = mm_decode_ffmpeg_core::wide_backend_id!("ffmpeg");
    assert_eq!(id(&dir.join("cfr.mp4")), Some("ffmpeg"));
    assert_eq!(id(&dir.join("p10.mp4")), Some(wide));
    assert_eq!(id(&hdr::fixture(&dir, HdrKind::Pq)), Some(wide));
}

#[test]
fn 契約の素材でも焼き込んだ番号がフレームの番号と一致する() {
    let dir = common::fixtures();
    let mut dec = FfmpegDecoderFactory
        .open(&dir.join("cfr.mp4"), None)
        .unwrap();
    let frame = dec.next_frame().unwrap().unwrap();
    assert_eq!(common::read_frame_number(frame.image.as_ref().unwrap()), 0);
}
