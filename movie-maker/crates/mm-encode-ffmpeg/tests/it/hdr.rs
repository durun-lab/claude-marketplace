//! HEVC Main10 の PQ と HLG の書き出し。線形の値（1 を超えるハイライトと Rec.709 の外の色を含む）を書いて
//! デコーダーで読み直し、許容の中で戻ることと、色の宣言と HDR10 のメタデータが付くことを ffprobe で確かめる。

use std::path::{Path, PathBuf};
use std::process::Command;

use mm_decode_api::{DecoderFactory, EncodeSettings, EncoderFactory};
use mm_decode_ffmpeg::FfmpegDecoderFactory;
use mm_encode_ffmpeg::FfmpegEncoderFactory;
use mm_types::transfer::{HDR_REFERENCE_WHITE_NITS, HLG_PEAK_NITS};
use mm_types::{Rational, Rgba, RgbaImage, Size};

/// 帯の幅と、帯の線形の値。黒、18% グレー、SDR の白、4 倍のハイライト、PQ だけで表せる 20 倍、Rec.709 の外の緑。
const BAND: u32 = 64;
const PATCHES: [[f32; 3]; 6] = [
    [0.0, 0.0, 0.0],
    [0.18, 0.18, 0.18],
    [1.0, 1.0, 1.0],
    [4.0, 4.0, 4.0],
    [20.0, 20.0, 20.0],
    [-0.3, 1.2, -0.05],
];
const SIZE: Size = Size::new(BAND * PATCHES.len() as u32, 160);
const FRAMES: usize = 5;
/// 往復の許容。10 bit の量子化（PQ の 1000 cd/m² 付近で 1 段が約 1%）と x265 の crf の分。
const RELATIVE: f32 = 0.03;
const ABSOLUTE: f32 = 0.004;

fn out_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("mm_encode_ffmpeg_hdr");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn patch_frame() -> RgbaImage {
    let mut image = RgbaImage::filled(SIZE, Rgba::new(0.0, 0.0, 0.0, 1.0));
    for (i, [r, g, b]) in PATCHES.iter().enumerate() {
        for y in 0..SIZE.height {
            for x in 0..BAND {
                image.set_pixel(i as u32 * BAND + x, y, Rgba::new(*r, *g, *b, 1.0));
            }
        }
    }
    image
}

fn write(codec: &str, path: &Path) {
    let mut enc = FfmpegEncoderFactory.create().unwrap();
    let settings = EncodeSettings {
        size: SIZE,
        fps: Rational::integer(30),
        codec: codec.into(),
        output_path: path.to_owned(),
        audio: None,
    };
    enc.begin(&settings).unwrap();
    let frames: Vec<RgbaImage> = (0..FRAMES).map(|_| patch_frame()).collect();
    enc.push_frames(&frames).unwrap();
    assert_eq!(enc.finish().unwrap().frames, FRAMES as u64);
}

/// 最初のフレームの帯の中央を読み、期待（`peak` を超える値は `peak` に切る）との相対の差の最大を返す。
fn read_back(path: &Path, color_space: &str, peak: f32) -> f32 {
    let mut dec = FfmpegDecoderFactory.open(path, None).unwrap();
    assert_eq!(dec.info().color_space.0, color_space);
    let image = dec.next_frame().unwrap().unwrap().image.unwrap();
    let mut worst = 0.0f32;
    for (i, patch) in PATCHES.iter().enumerate() {
        let p = image.pixel(i as u32 * BAND + BAND / 2, SIZE.height / 2);
        for (got, want) in [p.r, p.g, p.b].into_iter().zip(patch.map(|v| v.min(peak))) {
            assert!(
                (got - want).abs() <= want.abs() * RELATIVE + ABSOLUTE,
                "帯 {i}: {got} 期待 {want}"
            );
            worst = worst.max((got - want).abs() / want.abs().max(1.0));
        }
    }
    worst
}

/// ffprobe の出力（`key=value` の行）を返す。
fn probe(path: &Path, args: &[&str]) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0"])
        .args(args)
        .args(["-of", "default=nw=1"])
        .arg(path)
        .output()
        .expect("ffprobe を実行できる");
    assert!(out.status.success(), "ffprobe が読めた");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn pq_で書いて読み直すと_1_を超える値も切られずに戻る() {
    let path = out_path("pq.mp4");
    write("hevc_pq", &path);
    let worst = read_back(&path, "bt2100_pq", f32::INFINITY);
    eprintln!("PQ の往復の最大の相対の差 {worst:e}");
    let stream = probe(
        &path,
        &[
            "-show_entries",
            "stream=codec_name,profile,pix_fmt,color_transfer,color_primaries,color_space",
        ],
    );
    for want in [
        "codec_name=hevc",
        "profile=Main 10",
        "pix_fmt=yuv420p10le",
        "color_transfer=smpte2084",
        "color_primaries=bt2020",
        "color_space=bt2020nc",
    ] {
        assert!(stream.contains(want), "{want} が無い: {stream}");
    }
    let side = probe(&path, &["-read_intervals", "%+#1", "-show_frames"]);
    assert!(
        side.contains("Mastering display metadata"),
        "マスタリングの表示: {side}"
    );
    assert!(side.contains("max_content=1000"), "MaxCLL: {side}");
    assert!(side.contains("max_average=400"), "MaxFALL: {side}");
}

#[test]
fn hlg_で書いて読み直すと表示の上限までの値が戻る() {
    let path = out_path("hlg.mp4");
    write("hevc_hlg", &path);
    let peak = HLG_PEAK_NITS / HDR_REFERENCE_WHITE_NITS;
    let worst = read_back(&path, "bt2100_hlg", peak);
    eprintln!("HLG の往復の最大の相対の差 {worst:e}");
    let stream = probe(
        &path,
        &["-show_entries", "stream=color_transfer,color_primaries"],
    );
    assert!(stream.contains("color_transfer=arib-std-b67"), "{stream}");
    assert!(stream.contains("color_primaries=bt2020"), "{stream}");
}
