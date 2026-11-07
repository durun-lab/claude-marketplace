//! 区間ごとに作る HLS。ffprobe が読めること、長さとビットレートが目標に近いこと、版が変わると作り直すこと。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mm_stream::{HlsQuality, HlsRequest, Payload, SegmentName};

use crate::common::{ffmpeg_config, make_media, number, probe, project, streamer};

/// 目標の全体のビットレート（bit/s）。
const TARGET_BPS: f64 = 500_000.0;
/// 先読みの区間が現れるまで待つ上限。
const PREFETCH_WAIT: Duration = Duration::from_secs(120);

fn req() -> HlsRequest {
    HlsRequest {
        sequence: "s".into(),
        quality: HlsQuality::P360,
    }
}

fn segment_file(dir: &Path, version: u64, index: u64) -> PathBuf {
    dir.join(format!("cache/mobile/hls/s/360p/v{version}-{index}.ts"))
}

fn probe_segment(dir: &Path, body: &[u8], name: &str) -> serde_json::Value {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    probe(&path)
}

fn stream<'a>(info: &'a serde_json::Value, kind: &str) -> &'a serde_json::Value {
    info["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["codec_type"] == kind)
        .unwrap_or_else(|| panic!("{kind} のストリームが無い"))
}

fn names(playlist: &Payload) -> Vec<SegmentName> {
    String::from_utf8_lossy(&playlist.body)
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.trim_start_matches("seg/").parse().unwrap())
        .collect()
}

#[test]
fn 目録と区間が返り_ffprobe_で読めて長さとビットレートが目標に近い() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let project = project(&media, 10);
    let s = streamer(config, dir.path());

    let playlist = s.playlist(&project, 1, &req(), "seg/").unwrap();
    assert_eq!(
        playlist.header("Content-Type"),
        Some("application/vnd.apple.mpegurl")
    );
    let list = names(&playlist);
    assert_eq!(list.len(), 3);

    let started = Instant::now();
    let first = s.segment(&project, 1, &req(), list[0]).unwrap();
    eprintln!("最初の区間（4 秒）を作る時間: {:?}", started.elapsed());
    assert_eq!(first.header("Content-Type"), Some("video/mp2t"));
    assert!(first.header("Cache-Control").unwrap().contains("immutable"));
    let info = probe_segment(dir.path(), &first.body, "v1-0.ts");
    let video = stream(&info, "video");
    assert_eq!(video["codec_name"], "h264");
    assert_eq!(
        (video["width"].as_u64(), video["height"].as_u64()),
        (Some(640), Some(360))
    );
    assert_eq!(stream(&info, "audio")["codec_name"], "aac");
    let seconds = number(&info["format"]["duration"]);
    assert!((seconds - 4.0).abs() < 0.3, "長さ {seconds}");
    let bps = first.body.len() as f64 * 8.0 / seconds;
    eprintln!("区間のビットレート: {:.0} kbps", bps / 1000.0);
    assert!(
        (0.75 * TARGET_BPS..1.15 * TARGET_BPS).contains(&bps),
        "ビットレート {bps}"
    );

    // 最後の区間は短く、時刻はプレーヤーの時間軸の続きから始まる。
    let last = s.segment(&project, 1, &req(), list[2]).unwrap();
    let info = probe_segment(dir.path(), &last.body, "v1-2.ts");
    let start = number(&info["format"]["start_time"]);
    let seconds = number(&info["format"]["duration"]);
    assert!((start - 8.0).abs() < 0.3, "始まり {start}");
    assert!((seconds - 2.0).abs() < 0.3, "長さ {seconds}");
}

#[test]
fn 要求した区間の後ろを先読みし_作った区間は作り直さない() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let project = project(&media, 14);
    let s = streamer(config, dir.path());
    let list = names(&s.playlist(&project, 1, &req(), "seg/").unwrap());

    let first = s.segment(&project, 1, &req(), list[0]).unwrap();
    let deadline = Instant::now() + PREFETCH_WAIT;
    while !(segment_file(dir.path(), 1, 1).exists() && segment_file(dir.path(), 1, 2).exists()) {
        assert!(Instant::now() < deadline, "先読みの区間が現れない");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !segment_file(dir.path(), 1, 3).exists(),
        "先読みは 2 区間まで"
    );

    let started = Instant::now();
    let again = s.segment(&project, 1, &req(), list[0]).unwrap();
    assert_eq!(again.body, first.body);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "作り直さずに返る"
    );
}

#[test]
fn プロジェクトが変わると区間を作り直し_古い版の区間は読めない() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let before = project(&media, 10);
    let s = streamer(config, dir.path());
    let old = names(&s.playlist(&before, 1, &req(), "seg/").unwrap());
    let old_first = s.segment(&before, 1, &req(), old[0]).unwrap();
    assert!(segment_file(dir.path(), 1, 0).exists());

    // 2 秒に縮めると区間は 1 つになる。
    let mut after = before.clone();
    after.sequences[0].cuts[0].out_point = mm_types::RationalTime::seconds(2);
    let playlist = s.playlist(&after, 2, &req(), "seg/").unwrap();
    let fresh = names(&playlist);
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0].version, 2);
    let new_first = s.segment(&after, 2, &req(), fresh[0]).unwrap();
    assert_ne!(new_first.body, old_first.body);
    assert!(segment_file(dir.path(), 2, 0).exists());
    assert!(!segment_file(dir.path(), 1, 0).exists(), "古い版は捨てる");
    assert!(s.segment(&after, 2, &req(), old[0]).is_err());
    let info = probe_segment(dir.path(), &new_first.body, "v2-0.ts");
    assert!((number(&info["format"]["duration"]) - 2.0).abs() < 0.3);
}

#[test]
fn 範囲の外の区間と無い_sequence_は拒む() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let project = project(&media, 10);
    let s = streamer(config, dir.path());
    let beyond = SegmentName {
        version: 1,
        index: 3,
    };
    assert!(s.segment(&project, 1, &req(), beyond).is_err());
    let missing = HlsRequest {
        sequence: "nope".into(),
        quality: HlsQuality::P360,
    };
    assert!(s.playlist(&project, 1, &missing, "").is_err());
}
