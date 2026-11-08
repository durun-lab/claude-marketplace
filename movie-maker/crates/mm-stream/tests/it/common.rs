//! 結合テストの共通部品。

use std::path::Path;
use std::process::Command;

use mm_doc::{
    Asset, Composition, Cut, Layer, LayerAudio, Node, Project, Sequence, SEQUENCE_KIND,
    SEQUENCE_PARAM,
};
use mm_runtime::{assemble, Backend, RuntimeConfig};
use mm_stream::{StreamConfig, Streamer};
use mm_types::color::ColorSpace;
use mm_types::{Id, MmError, ParamValue, Rational, RationalTime};

/// 素材の長さ（秒）。
pub const MEDIA_SECONDS: i64 = 14;

/// GPU で描ける設定。組めなければ `None`（`MM_REQUIRE_GPU` があれば失敗）。
pub fn gpu_config() -> Option<RuntimeConfig> {
    let config = RuntimeConfig {
        render: Backend::Wgpu,
        audio: Backend::Dsp,
        ..RuntimeConfig::sample()
    };
    usable(config)
}

/// 実際のデコードとエンコードで描ける設定。FFmpeg を入れていない設定では `None`。
pub fn ffmpeg_config() -> Option<RuntimeConfig> {
    let config = RuntimeConfig {
        decode: Backend::Ffmpeg,
        encode: Backend::Ffmpeg,
        ..RuntimeConfig::real()
    };
    usable(config)
}

fn usable(config: RuntimeConfig) -> Option<RuntimeConfig> {
    match assemble(&config) {
        Ok(_) => Some(config),
        Err(MmError::Unsupported(e)) => {
            // FFmpeg を入れていない設定のスキップと、GPU が無いことの失敗を分ける。
            let ffmpeg_missing = e.to_lowercase().contains("ffmpeg");
            if !ffmpeg_missing && std::env::var_os("MM_REQUIRE_GPU").is_some() {
                panic!("組めない: {e}");
            }
            eprintln!("スキップする: {e}");
            None
        }
        Err(e) => panic!("組めない: {e}"),
    }
}

pub fn streamer(config: RuntimeConfig, dir: &Path) -> Streamer {
    Streamer::spawn(StreamConfig::new(config, dir.join("cache/mobile"))).unwrap()
}

/// ffmpeg の CLI で、動きのある映像と音の素材を作る。
pub fn make_media(path: &Path) {
    let status = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-y"])
        .args(["-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"])
        .args(["-t", &MEDIA_SECONDS.to_string()])
        .args([
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(path)
        .status()
        .expect("ffmpeg を起動できない");
    assert!(status.success());
}

/// 素材 1 本を `seconds` 秒使うシーケンス `s` を、音つきのレイヤーに置いたコンポジション `main`（1280x720）。
pub fn project(media: &Path, seconds: i64) -> Project {
    let asset_id = Id::new("clip");
    let fps = Rational::integer(30);
    let footage =
        Node::new("footage", 1).with_param("asset", ParamValue::AssetRef(asset_id.clone()));
    let mut p = Project::new();
    p.assets.push(Asset {
        id: asset_id,
        path: media.to_string_lossy().into_owned(),
        content_hash: "stream-test-clip".into(),
        color_space: ColorSpace::bt709(),
        duration: RationalTime::seconds(MEDIA_SECONDS),
        fps: Some(fps),
        timebase_offset: RationalTime::ZERO,
    });
    p.sequences.push(Sequence {
        id: Id::new("s"),
        cuts: vec![Cut::new(
            "a",
            footage,
            RationalTime::ZERO,
            RationalTime::seconds(seconds),
        )],
        origin: None,
    });
    let sequence =
        Node::new(SEQUENCE_KIND, 1).with_param(SEQUENCE_PARAM, ParamValue::Text("s".into()));
    let layer = Layer {
        audio: Some(LayerAudio::new("voice")),
        ..Layer::new(
            "l-sequence",
            sequence,
            RationalTime::ZERO,
            RationalTime::ZERO,
            RationalTime::seconds(seconds),
        )
    };
    p.compositions.push(
        Composition::new(
            Id::new("main"),
            1280,
            720,
            fps,
            RationalTime::seconds(seconds),
        )
        .with_layers(vec![layer]),
    );
    p
}

/// ffprobe の JSON（`-show_streams -show_format`）。
pub fn probe(path: &Path) -> serde_json::Value {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .arg(path)
        .output()
        .expect("ffprobe を起動できない");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// JSON の文字列の数値を読む（ffprobe は数値を文字列で出す）。
pub fn number(v: &serde_json::Value) -> f64 {
    v.as_str()
        .and_then(|s| s.parse().ok())
        .or_else(|| v.as_f64())
        .expect("数値")
}
