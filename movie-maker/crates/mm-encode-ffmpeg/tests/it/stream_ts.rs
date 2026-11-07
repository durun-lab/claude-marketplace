//! 配信の区間（`h264_ts`）を書き、ffprobe で読み直して、ビットレート・時刻の始まり・キーフレームの間隔を確かめる。

use std::f32::consts::TAU;
use std::path::{Path, PathBuf};
use std::process::Command;

use mm_audio_api::AudioBlock;
use mm_decode_api::{AudioStreamInfo, EncodeSettings, EncoderFactory, StreamOutput};
use mm_encode_ffmpeg::FfmpegEncoderFactory;
use mm_types::{Rational, RationalTime, Rgba, RgbaImage, Size};

const RATE: u32 = 48_000;
const FPS: i64 = 30;
const SEGMENT_SECS: i64 = 4;
const SEGMENTS: i64 = 2;
const VIDEO_BIT_RATE: u32 = 350_000;
const AUDIO_BIT_RATE: u32 = 64_000;
/// 目標のビットレートからの許容ずれ（割合）。
const RATE_TOLERANCE: f64 = 0.2;
/// 時刻の始まりの許容ずれ（秒）。
const START_TOLERANCE: f64 = 0.05;
/// 区間の始まりの秒（3 つ目の区間）。
const START_SECS: i64 = 8;

fn out_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("mm_encode_ffmpeg_stream_ts");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// 動きと細かい模様のある絵。ビットレートが目標に届く程度に複雑にする。
fn textured_frame(size: Size, n: i64) -> RgbaImage {
    let t = n as f32;
    let mut image = RgbaImage::filled(size, Rgba::new(0.0, 0.0, 0.0, 1.0));
    for y in 0..size.height {
        for x in 0..size.width {
            let (fx, fy) = (x as f32, y as f32);
            let wave = (fx * 0.11 + t * 0.35).sin() * (fy * 0.07 - t * 0.21).cos();
            let grain =
                ((x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503) ^ (n as u32 * 977)) % 255)
                    as f32
                    / 255.0;
            let v = (0.5 + 0.3 * wave + 0.08 * grain).clamp(0.0, 1.0);
            image.set_pixel(x, y, Rgba::new(v, v * 0.9, 1.0 - v, 1.0));
        }
    }
    image
}

fn tone(start: usize, frames: usize) -> AudioBlock {
    let mut samples = Vec::with_capacity(frames * 2);
    for i in start..start + frames {
        let v = 0.25 * (TAU * 440.0 * i as f32 / RATE as f32).sin();
        samples.extend([v, v]);
    }
    AudioBlock::from_samples(RATE, 2, samples).unwrap()
}

/// ffprobe の出力（1 行 1 件）を行ごとに返す。
fn probe_lines(path: &Path, args: &[&str]) -> Vec<String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error"])
        .args(args)
        .arg(path)
        .output()
        .expect("ffprobe を実行できる");
    assert!(
        out.status.success(),
        "ffprobe が読めた: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// 映像のパケットの (表示時刻の秒, バイト数, キーフレームか)。
fn video_packets(path: &Path) -> Vec<(f64, u64, bool)> {
    probe_lines(
        path,
        &[
            "-select_streams",
            "v:0",
            "-show_entries",
            "packet=pts_time,size,flags",
            "-of",
            "csv=p=0",
        ],
    )
    .iter()
    .map(|line| {
        let mut f = line.split(',');
        let pts = f.next().unwrap().parse().unwrap();
        let size = f.next().unwrap().parse().unwrap();
        (pts, size, f.next().unwrap().starts_with('K'))
    })
    .collect()
}

fn write_segment(path: &Path, stream: StreamOutput, seconds: i64) {
    let size = Size::new(640, 360);
    let mut enc = FfmpegEncoderFactory.create().unwrap();
    enc.set_stream_output(stream).unwrap();
    enc.begin(&EncodeSettings {
        size,
        fps: Rational::integer(FPS as u32),
        codec: "h264_ts".into(),
        output_path: path.to_owned(),
        audio: Some(AudioStreamInfo {
            sample_rate: RATE,
            channels: 2,
        }),
    })
    .unwrap();
    let frames: Vec<RgbaImage> = (0..seconds * FPS)
        .map(|n| textured_frame(size, n))
        .collect();
    for chunk in frames.chunks(10) {
        enc.push_frames(chunk).unwrap();
    }
    let mut sent = 0;
    while sent < (seconds as usize) * RATE as usize {
        enc.push_audio(&tone(sent, 4_800)).unwrap();
        sent += 4_800;
    }
    let report = enc.finish().unwrap();
    assert_eq!(report.frames, (seconds * FPS) as u64);
}

fn stream_output(start_secs: i64, max_rate: Option<u32>) -> StreamOutput {
    StreamOutput {
        video_bit_rate: VIDEO_BIT_RATE,
        video_max_rate: max_rate,
        audio_bit_rate: AUDIO_BIT_RATE,
        keyframe_interval_frames: (SEGMENT_SECS * FPS) as u32,
        start_offset: RationalTime::new(i128::from(start_secs), 1).unwrap(),
    }
}

#[test]
fn 配信の区間は目標のビットレートの_20_パーセント以内で書ける() {
    let path = out_path("rate.ts");
    let seconds = SEGMENT_SECS * SEGMENTS;
    write_segment(
        &path,
        stream_output(0, Some(VIDEO_BIT_RATE * 3 / 2)),
        seconds,
    );
    let packets = video_packets(&path);
    let bits = packets.iter().map(|p| p.1).sum::<u64>() as f64 * 8.0;
    let rate = bits / seconds as f64;
    let target = f64::from(VIDEO_BIT_RATE);
    assert!(
        (rate - target).abs() <= target * RATE_TOLERANCE,
        "映像のビットレート {rate:.0} bit/s（目標 {target:.0}）"
    );
}

#[test]
fn 配信の区間は_ts_で時刻が指定の秒から始まる() {
    let path = out_path("offset.ts");
    write_segment(&path, stream_output(START_SECS, None), SEGMENT_SECS);
    let format = probe_lines(
        &path,
        &["-show_entries", "format=format_name", "-of", "csv=p=0"],
    );
    assert_eq!(format, ["mpegts"]);
    let codecs = probe_lines(
        &path,
        &["-show_entries", "stream=codec_name", "-of", "csv=p=0"],
    );
    assert!(codecs.contains(&"h264".to_owned()) && codecs.contains(&"aac".to_owned()));
    let first = video_packets(&path)
        .iter()
        .map(|p| p.0)
        .fold(f64::INFINITY, f64::min);
    assert!(
        (first - START_SECS as f64).abs() <= START_TOLERANCE,
        "最初の映像の時刻 {first}（指定 {START_SECS}）"
    );
}

#[test]
fn キーフレームは区間の先頭に入り間隔は固定で途中に入らない() {
    let path = out_path("gop.ts");
    let seconds = SEGMENT_SECS * SEGMENTS;
    write_segment(&path, stream_output(0, None), seconds);
    let mut keys: Vec<f64> = video_packets(&path)
        .iter()
        .filter(|p| p.2)
        .map(|p| p.0)
        .collect();
    keys.sort_by(f64::total_cmp);
    assert_eq!(keys.len(), SEGMENTS as usize, "キーフレーム {keys:?}");
    for (i, t) in keys.iter().enumerate() {
        let want = (i as i64 * SEGMENT_SECS) as f64;
        assert!(
            (t - want).abs() <= START_TOLERANCE,
            "{i} 個目 {t}（{want}）"
        );
    }
}

#[test]
fn ビットレートの指定は配信の区間のコーデックにだけ要る() {
    let path = out_path("misuse.mp4");
    let settings = |codec: &str| EncodeSettings {
        size: Size::new(64, 64),
        fps: Rational::integer(FPS as u32),
        codec: codec.into(),
        output_path: path.clone(),
        audio: None,
    };
    let mut enc = FfmpegEncoderFactory.create().unwrap();
    assert!(
        enc.begin(&settings("h264_ts")).is_err(),
        "指定なしの h264_ts"
    );
    let mut enc = FfmpegEncoderFactory.create().unwrap();
    enc.set_stream_output(stream_output(0, None)).unwrap();
    assert!(enc.begin(&settings("h264")).is_err(), "h264 に指定");
}
