//! 提案の区間の短い動画。ffprobe で読めること、範囲と長さ、moov が先頭にあること、`Range` に 206 で答えること、
//! 提案を適用したフレームになっていること、同じキーは作り直さないこと。

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use mm_doc::{Layer, Node, Project};
use mm_stream::{ClipRequest, Payload, CLIP_PAD_SECONDS};
use mm_types::{ParamValue, RationalTime, Rgba};

use crate::common::{ffmpeg_config, make_media, number, probe, project, streamer};

/// 提案の範囲（コンポジションの秒）。短い動画は前後に 2 秒を足した 2〜8 秒。
const SPAN: (f64, f64) = (4.0, 6.0);
/// 長さの誤差の許容（秒）。AAC の端の分だけ映像より長くなりうる。
const DURATION_TOLERANCE: f64 = 0.15;

fn req(ops: &str) -> ClipRequest {
    ClipRequest {
        composition: "main".into(),
        start: SPAN.0,
        end: SPAN.1,
        ops: ops.into(),
        touched: vec!["red".into()],
    }
}

/// 提案を適用した後のプロジェクト。範囲の 4〜6 秒に、画面を覆う赤い単色のレイヤーを足す。
fn applied(base: &Project) -> Project {
    let red = Node::new("solid", 1).with_param(
        "color",
        ParamValue::Color(Rgba {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }),
    );
    let mut p = base.clone();
    p.compositions[0].layers.push(Layer::new(
        "red",
        red,
        RationalTime::seconds(4),
        RationalTime::ZERO,
        RationalTime::seconds(2),
    ));
    p
}

/// MP4 の最上位のボックスの種類を順に並べる。
fn top_boxes(mp4: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 8 <= mp4.len() {
        let size = u32::from_be_bytes(mp4[at..at + 4].try_into().unwrap()) as usize;
        out.push(String::from_utf8_lossy(&mp4[at + 4..at + 8]).into_owned());
        if size < 8 {
            break;
        }
        at += size;
    }
    out
}

/// 短い動画の `seconds` 秒のフレームの、赤と緑の平均（0〜255）。
fn red_and_green_at(path: &Path, seconds: f64) -> (f64, f64) {
    let out = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-ss", &seconds.to_string(), "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("ffmpeg を起動できない");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let pixels = out.stdout.len() / 3;
    let mean = |channel: usize| {
        out.stdout
            .iter()
            .skip(channel)
            .step_by(3)
            .map(|&v| f64::from(v))
            .sum::<f64>()
            / pixels as f64
    };
    (mean(0), mean(1))
}

fn save(dir: &Path, name: &str, clip: &Payload) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, &clip.body).unwrap();
    path
}

#[test]
fn 短い動画は範囲の前後を足した長さの_360p_の_mp4_で_moov_が先頭にある() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let s = streamer(config, dir.path());
    let project = applied(&project(&media, 10));

    let started = Instant::now();
    let clip = s.clip(&project, 1, &req("[0]"), None).unwrap();
    eprintln!(
        "短い動画（6 秒）を作る時間: {:?}、{} バイト",
        started.elapsed(),
        clip.body.len()
    );
    assert_eq!(clip.status, 200);
    assert_eq!(clip.header("Content-Type"), Some("video/mp4"));
    assert_eq!(clip.header("Accept-Ranges"), Some("bytes"));
    let boxes = top_boxes(&clip.body);
    let at = |name: &str| {
        boxes
            .iter()
            .position(|b| b == name)
            .unwrap_or_else(|| panic!("{name} が無い: {boxes:?}"))
    };
    assert!(
        at("moov") < at("mdat"),
        "moov が mdat より前（faststart）: {boxes:?}"
    );

    let info = probe(&save(dir.path(), "clip.mp4", &clip));
    let streams = info["streams"].as_array().unwrap();
    let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
    assert_eq!(video["codec_name"], "h264");
    assert_eq!(
        (video["width"].as_u64(), video["height"].as_u64()),
        (Some(640), Some(360))
    );
    assert!(streams
        .iter()
        .any(|s| s["codec_type"] == "audio" && s["codec_name"] == "aac"));
    let expected = SPAN.1 - SPAN.0 + 2.0 * CLIP_PAD_SECONDS;
    let seconds = number(&info["format"]["duration"]);
    assert!(
        (seconds - expected).abs() < DURATION_TOLERANCE,
        "長さ {seconds}"
    );
    assert!(number(&info["format"]["start_time"]).abs() < DURATION_TOLERANCE);
}

#[test]
fn 短い動画は提案を適用したフレームで_range_に_206_で答え_同じキーは作り直さない() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let s = streamer(config, dir.path());
    let base = project(&media, 10);
    let after = s.clip(&applied(&base), 1, &req("[0]"), None).unwrap();
    let before = s.clip(&base, 1, &req("[]"), None).unwrap();
    assert_ne!(after.header("ETag"), before.header("ETag"));

    // 短い動画の 3 秒はコンポジションの 5 秒で、赤いレイヤーが覆う所。前の 1 秒（3 秒）は覆わない。
    let (after_path, before_path) = (
        save(dir.path(), "after.mp4", &after),
        save(dir.path(), "before.mp4", &before),
    );
    let (red, green) = red_and_green_at(&after_path, 3.0);
    assert!(
        red > 200.0 && green < 40.0,
        "提案を適用したフレーム: 赤 {red}、緑 {green}"
    );
    let (red_before, green_before) = red_and_green_at(&before_path, 3.0);
    assert!(
        green_before > 60.0,
        "元のフレーム: 赤 {red_before}、緑 {green_before}"
    );
    let (_, green_edge) = red_and_green_at(&after_path, 1.0);
    assert!(
        green_edge > 60.0,
        "範囲の前のプリロールは覆わない: 緑 {green_edge}"
    );

    let len = after.body.len();
    let head = s
        .clip(&applied(&base), 1, &req("[0]"), Some("bytes=0-99"))
        .unwrap();
    assert_eq!(head.status, 206);
    assert_eq!(head.body, after.body[..100]);
    assert_eq!(
        head.header("Content-Range"),
        Some(format!("bytes 0-99/{len}").as_str())
    );
    assert_eq!(head.header("ETag"), after.header("ETag"));
    let beyond = s
        .clip(
            &applied(&base),
            1,
            &req("[0]"),
            Some(&format!("bytes={len}-")),
        )
        .unwrap();
    assert_eq!(beyond.status, 416);

    let started = Instant::now();
    let again = s.clip(&applied(&base), 2, &req("[0]"), None).unwrap();
    assert_eq!(
        again.body, after.body,
        "バージョンが進んでも中身が同じなら同じ動画"
    );
    assert!(
        started.elapsed().as_millis() < 500,
        "キャッシュから返る: {:?}",
        started.elapsed()
    );
}
