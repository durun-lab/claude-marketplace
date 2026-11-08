//! 提案の区間の短い動画。ffprobe で読めること、範囲と長さ、moov が先頭にあること、`Range` に 206 で答えること、
//! 提案を適用したフレームになっていること、同じキーは作り直さないこと、範囲の中の編集で前のキーが前のバイトのまま残ること。

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use mm_doc::{Layer, Node, Project};
use mm_stream::{ClipRequest, Payload, Streamer, CLIP_PAD_SECONDS};
use mm_types::{MmError, ParamValue, RationalTime, Rgba};

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
    let clip = clip(&s, &project, 1, "[0]", None);
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
    let after = clip(&s, &applied(&base), 1, "[0]", None);
    let before = clip(&s, &base, 1, "[]", None);
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
    let head = clip(&s, &applied(&base), 1, "[0]", Some("bytes=0-99"));
    assert_eq!(head.status, 206);
    assert_eq!(head.body, after.body[..100]);
    assert_eq!(
        head.header("Content-Range"),
        Some(format!("bytes 0-99/{len}").as_str())
    );
    assert_eq!(head.header("ETag"), after.header("ETag"));
    let beyond = clip(
        &s,
        &applied(&base),
        1,
        "[0]",
        Some(&format!("bytes={len}-")),
    );
    assert_eq!(beyond.status, 416);

    let started = Instant::now();
    let again = clip(&s, &applied(&base), 2, "[0]", None);
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

#[test]
fn 範囲の中を編集するとキーが変わり_前のキーは作ったバイトのまま答える() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let s = streamer(config, dir.path());
    let first = applied(&project(&media, 10));
    let old = s.clip_key(&first, 1, &req("[0]")).unwrap();
    let played = s.clip(&first, 1, &req("[0]"), old, None).unwrap().body;

    let mut edited = first.clone();
    let red = edited.compositions[0].layers.last_mut().unwrap();
    red.source = red.source.clone().with_param(
        "color",
        ParamValue::Color(Rgba {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        }),
    );
    let new = s.clip_key(&edited, 2, &req("[0]")).unwrap();
    assert_ne!(new, old, "範囲の中の編集は別のキー");
    let stale = s.clip(&edited, 2, &req("[0]"), old, None).unwrap_err();
    assert!(matches!(stale, MmError::NotFound(_)), "{stale}");
    assert!(s.cached_clip(new, None).unwrap().is_none());

    let rest = s.cached_clip(old, Some("bytes=100-199")).unwrap().unwrap();
    assert_eq!(rest.status, 206);
    assert_eq!(rest.body, played[100..200], "再生の続きは前の動画のバイト");
    assert_eq!(
        rest.header("Cache-Control"),
        Some("public, max-age=31536000, immutable")
    );
}

/// 今の中身のキーで短い動画を読む（端末が提案の応答の URL で読むのと同じ）。
fn clip(s: &Streamer, project: &Project, version: u64, ops: &str, range: Option<&str>) -> Payload {
    let req = req(ops);
    let key = s.clip_key(project, version, &req).unwrap();
    s.clip(project, version, &req, key, range).unwrap()
}
