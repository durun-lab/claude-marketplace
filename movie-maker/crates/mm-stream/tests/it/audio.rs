//! 素材の音だけ。

use mm_runtime::RuntimeConfig;
use mm_stream::AudioRequest;

use crate::common::{make_media, number, probe, project, streamer};

/// 1 時間の目標（バイト）。AAC 64 kbps で約 29 MB。
const HOUR_BYTES: f64 = 29.0 * 1_000_000.0;
const HOUR_SECONDS: f64 = 3600.0;

fn req() -> AudioRequest {
    AudioRequest {
        asset: "clip".into(),
    }
}

#[test]
fn 素材の音は_aac_64kbps_の_m4a_で_range_に答える() {
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let project = project(&media, 10);
    let s = streamer(RuntimeConfig::sample(), dir.path());

    let full = s.audio(&project, &req(), None).unwrap();
    assert_eq!(full.status, 200);
    assert_eq!(full.header("Content-Type"), Some("audio/mp4"));
    assert_eq!(full.header("Accept-Ranges"), Some("bytes"));
    let path = dir.path().join("out.m4a");
    std::fs::write(&path, &full.body).unwrap();
    let info = probe(&path);
    let audio = &info["streams"][0];
    assert_eq!(audio["codec_name"], "aac");
    assert_eq!(audio["channels"].as_u64(), Some(1));
    assert!(
        info["streams"].as_array().unwrap().len() == 1,
        "映像は含まない"
    );
    let seconds = number(&info["format"]["duration"]);
    assert!((seconds - 14.0).abs() < 0.5, "長さ {seconds}");
    let per_hour = full.body.len() as f64 / seconds * HOUR_SECONDS;
    eprintln!("1 時間あたり: {:.1} MB", per_hour / 1e6);
    assert!(per_hour < HOUR_BYTES * 1.15, "1 時間 {per_hour} バイト");

    let head = s.audio(&project, &req(), Some("bytes=0-99")).unwrap();
    assert_eq!(head.status, 206);
    assert_eq!(head.body, full.body[..100]);
    let total = full.body.len();
    assert_eq!(
        head.header("Content-Range"),
        Some(format!("bytes 0-99/{total}").as_str())
    );
    let tail = s.audio(&project, &req(), Some("bytes=-50")).unwrap();
    assert_eq!(tail.body, full.body[total - 50..]);
    let out = s
        .audio(&project, &req(), Some(&format!("bytes={total}-")))
        .unwrap();
    assert_eq!(out.status, 416);
}

#[test]
fn 無い素材はエラーになる() {
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let project = project(&media, 10);
    let s = streamer(RuntimeConfig::sample(), dir.path());
    assert!(s
        .audio(
            &project,
            &AudioRequest {
                asset: "nope".into()
            },
            None
        )
        .is_err());
}
