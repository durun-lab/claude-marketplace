//! GPU のコンテキストを渡したときの、YUV のプレーンを上げる経路（feature `gpu`）。
//! CPU の経路と同じ画素（番号の焼き込みが読め、値が近い）になることを確かめる。
//! GPU が無い環境ではスキップし、`MM_REQUIRE_GPU` があれば失敗にする。

use crate::common;

use std::sync::Arc;

use mm_decode_api::{DecodedFrame, DecoderFactory};
use mm_decode_ffmpeg::{FfmpegCpuDecoderFactory, FfmpegDecoderFactory};
use mm_gpu::{GpuContext, GpuRequest, WgpuGpu};
use mm_node_api::GpuHelper;
use mm_testkit::hdr::{self, HdrKind};
use mm_types::{RationalTime, RgbaImage};

/// 色差の補間（CPU は bicubic、GPU は双線形）と f16 の丸めの差の許容。番号の矩形は輝度だけなので十分に小さい。
const TOLERANCE: f32 = 0.05;

fn gpu() -> Option<Arc<GpuContext>> {
    match GpuContext::shared(&GpuRequest::default()) {
        Ok(ctx) => Some(ctx),
        Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_some() => {
            panic!("MM_REQUIRE_GPU が有るのに GPU を作れない: {e}")
        }
        Err(e) => {
            eprintln!("GPU が無いのでスキップする: {e}");
            None
        }
    }
}

fn download(helper: &mut WgpuGpu, frame: &DecodedFrame) -> RgbaImage {
    let handle = frame.gpu.as_ref().expect("GPU のフレーム");
    let imported = helper.import(handle).expect("借用できる");
    let image = helper.download(&imported).expect("リードバックできる");
    helper.before_submit();
    helper.after_submit();
    (*image).clone()
}

#[test]
fn gpu_に上げたフレームは_cpu_の経路と同じ画素になる() {
    let Some(ctx) = gpu() else { return };
    let path = common::fixtures().join("cfr.mp4");
    let mut on_gpu = FfmpegDecoderFactory
        .open(&path, Some(ctx.as_ref()))
        .expect("開ける");
    let mut on_cpu = FfmpegCpuDecoderFactory
        .open(&path, Some(ctx.as_ref()))
        .expect("開ける");
    let mut helper = WgpuGpu::new(ctx.clone());
    for _ in 0..5 {
        let g = on_gpu.next_frame().expect("デコード").expect("ある");
        let c = on_cpu.next_frame().expect("デコード").expect("ある");
        assert!(g.image.is_none() && g.gpu.is_some(), "GPU のフレームで返す");
        let got = download(&mut helper, &g);
        let want = c.image.expect("CPU の画像");
        assert_eq!(
            common::read_frame_number(&got),
            common::read_frame_number(&want)
        );
        let diff = got.max_diff(&want).expect("同じ大きさ");
        assert!(diff <= TOLERANCE, "フレーム {} の差 {diff}", g.index);
    }
    on_gpu.seek(RationalTime::ZERO).expect("シーク");
    let first = on_gpu.next_frame().expect("デコード").expect("ある");
    assert_eq!(first.index, 0);
    assert_eq!(common::read_frame_number(&download(&mut helper, &first)), 0);
}

/// 10 bit の PQ と HLG を 16 bit のプレーンのまま上げ、シェーダーで 1 を超える線形の光にする。
#[test]
fn hdr_の素材は_gpu_に上げても_1_を超える線形の光になる() {
    let request = GpuRequest {
        optional_features: GpuRequest::default().optional_features
            | wgpu::Features::TEXTURE_FORMAT_16BIT_NORM,
        ..GpuRequest::default()
    };
    let ctx = match GpuContext::with(&request) {
        Ok(ctx) => ctx,
        Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_some() => panic!("GPU を作れない: {e}"),
        Err(_) => return,
    };
    if !ctx
        .device
        .features()
        .contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM)
    {
        eprintln!("16 bit の正規化テクスチャが無いのでスキップする");
        return;
    }
    let mut helper = WgpuGpu::new(ctx.clone());
    for kind in [HdrKind::Pq, HdrKind::Hlg] {
        let path = hdr::fixture(&common::fixtures(), kind);
        let mut dec = FfmpegDecoderFactory
            .open(&path, Some(ctx.as_ref()))
            .expect("開ける");
        let frame = dec.next_frame().expect("デコード").expect("ある");
        assert!(frame.gpu.is_some(), "GPU のフレームで返す");
        let image = download(&mut helper, &frame);
        hdr::check_bands(&image, kind, "GPU").unwrap();
        let on_cpu = FfmpegCpuDecoderFactory
            .open(&path, Some(ctx.as_ref()))
            .expect("開ける");
        assert_eq!(dec.backend_id(), Some(mm_decode_ffmpeg::UPLOAD_WIDE_ID));
        assert_eq!(
            on_cpu.backend_id(),
            Some(mm_decode_ffmpeg::WIDE_ID),
            "画素の一致しない CPU の経路とフレームのキャッシュで画像を引き合わない"
        );
        eprintln!(
            "GPU に上げた {kind:?} の帯の最大の相対の差 {:e}",
            hdr::max_relative_error(&image, kind)
        );
    }
}
