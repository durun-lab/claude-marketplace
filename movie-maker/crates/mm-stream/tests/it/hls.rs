//! 区間ごとに作る HLS。ffprobe で読めること、長さとビットレートが目標に近いこと、
//! 編集で変わらない区間は同じキーのまま作り直さないこと。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mm_doc::{Layer, Node};
use mm_stream::{HlsQuality, HlsRequest, Payload, SegmentKey};
use mm_types::{ParamValue, RationalTime, Rgba};

use crate::common::{ffmpeg_config, make_media, number, probe, project, streamer};

/// 目標の全体のビットレート（bit/s）。
const TARGET_BPS: f64 = 500_000.0;
/// 先読みの区間が現れるまで待つ上限。
const PREFETCH_WAIT: Duration = Duration::from_secs(120);

fn req() -> HlsRequest {
    HlsRequest {
        composition: "main".into(),
        quality: HlsQuality::P360,
    }
}

fn segment_file(dir: &Path, key: SegmentKey) -> PathBuf {
    dir.join("cache/mobile/hls").join(key.file_name())
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

fn keys(playlist: &Payload) -> Vec<SegmentKey> {
    String::from_utf8_lossy(&playlist.body)
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.trim_start_matches("seg/").parse().unwrap())
        .collect()
}

#[test]
fn プレイリストと区間が返り_ffprobe_で読めて長さとビットレートが目標に近い() {
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
    let list = keys(&playlist);
    assert_eq!(list.len(), 3);

    let started = Instant::now();
    let first = s.segment(list[0]).unwrap();
    eprintln!("最初の区間（4 秒）を作る時間: {:?}", started.elapsed());
    assert_eq!(first.header("Content-Type"), Some("video/mp2t"));
    assert!(first.header("Cache-Control").unwrap().contains("immutable"));
    let info = probe_segment(dir.path(), &first.body, "first.ts");
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
    let last = s.segment(list[2]).unwrap();
    let info = probe_segment(dir.path(), &last.body, "last.ts");
    let start = number(&info["format"]["start_time"]);
    let seconds = number(&info["format"]["duration"]);
    assert!((start - 8.0).abs() < 0.3, "始まり {start}");
    assert!((seconds - 2.0).abs() < 0.3, "長さ {seconds}");
}

/// 映像と音の始まりが区間の始まりの秒から、これ以内（秒）にそろう。
const START_TOLERANCE: f64 = 0.002;

#[test]
fn 区間の映像と音は区間の始まりの秒からそろって始まる() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let project = project(&media, 10);
    let s = streamer(config, dir.path());
    let list = keys(&s.playlist(&project, 1, &req(), "seg/").unwrap());
    for (index, expected) in [(0, 0.0), (1, 4.0), (2, 8.0)] {
        let body = s.segment(list[index]).unwrap().body;
        let info = probe_segment(dir.path(), &body, &format!("seg{index}.ts"));
        assert_eq!(info["format"]["format_name"], "mpegts");
        for kind in ["video", "audio"] {
            let start = number(&stream(&info, kind)["start_time"]);
            assert!(
                (start - expected).abs() < START_TOLERANCE,
                "区間 {index} の {kind} の始まり {start}（期待 {expected}）"
            );
        }
    }
}

#[test]
fn 親のプレイリストはピークの_bandwidth_を載せ_区間の実測のピークに収まる() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let project = project(&media, 10);
    let s = streamer(config, dir.path());
    let master = s
        .master_playlist(&project, "main", &|q| format!("{}.m3u8", q.as_str()))
        .unwrap();
    let text = String::from_utf8(master.body).unwrap();
    let bandwidth: f64 = text
        .lines()
        .find(|l| l.contains("640x360"))
        .and_then(|l| l.split("BANDWIDTH=").nth(1))
        .and_then(|l| l.split(',').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("BANDWIDTH が読めない: {text}"));
    let list = keys(&s.playlist(&project, 1, &req(), "seg/").unwrap());
    let first = s.segment(list[0]).unwrap();
    let bps = first.body.len() as f64 * 8.0 / 4.0;
    eprintln!("BANDWIDTH {bandwidth} bps、最初の区間の実測 {bps:.0} bps");
    assert!(
        bps <= bandwidth,
        "実測 {bps} が BANDWIDTH {bandwidth} を超える"
    );
}

/// H.264 の NAL の種類のうち SPS。
const NAL_SPS: u8 = 7;

/// 区間の最初の SPS の profile_idc、constraint の旗、level_idc から作る RFC 6381 の `avc1.PPCCLL`。
/// SPS は最初の映像の TS パケットの中にあるので、パケットの境をまたがない。
fn avc1_of(ts: &[u8]) -> String {
    let at = ts
        .windows(4)
        .position(|w| w[..3] == [0, 0, 1] && w[3] & 0x1f == NAL_SPS)
        .expect("区間に SPS が無い");
    let sps = &ts[at + 4..at + 7];
    format!("avc1.{:02x}{:02x}{:02x}", sps[0], sps[1], sps[2])
}

/// AAC の profile の ffprobe の名前から、RFC 6381 の `mp4a.40.<オブジェクトの種類>`。
fn mp4a_of(audio: &serde_json::Value) -> &'static str {
    match audio["profile"].as_str() {
        Some("LC") => "mp4a.40.2",
        other => panic!("想定しない AAC の profile: {other:?}"),
    }
}

#[test]
fn 親のプレイリストの_codecs_は各画質の区間の_sps_と音の種類に一致する() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let project = project(&media, 10);
    let s = streamer(config, dir.path());
    let master = s
        .master_playlist(&project, "main", &|q| format!("{}.m3u8", q.as_str()))
        .unwrap();
    let text = String::from_utf8(master.body).unwrap();
    for quality in HlsQuality::ALL {
        let uri = format!("{}.m3u8", quality.as_str());
        let lines: Vec<&str> = text.lines().collect();
        let at = lines.iter().position(|l| *l == uri).expect(&uri);
        let codecs = lines[at - 1]
            .split("CODECS=\"")
            .nth(1)
            .and_then(|l| l.split('"').next())
            .unwrap_or_else(|| panic!("CODECS が読めない: {text}"));
        let req = HlsRequest { quality, ..req() };
        let list = keys(&s.playlist(&project, 1, &req, "seg/").unwrap());
        let body = s.segment(list[0]).unwrap().body;
        let info = probe_segment(dir.path(), &body, &format!("{}.ts", quality.as_str()));
        let actual = format!("{},{}", avc1_of(&body), mp4a_of(stream(&info, "audio")));
        assert_eq!(codecs, actual, "{}", quality.as_str());
    }
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
    let list = keys(&s.playlist(&project, 1, &req(), "seg/").unwrap());

    let first = s.segment(list[0]).unwrap();
    let deadline = Instant::now() + PREFETCH_WAIT;
    while !(segment_file(dir.path(), list[1]).exists()
        && segment_file(dir.path(), list[2]).exists())
    {
        assert!(Instant::now() < deadline, "先読みの区間が現れない");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !segment_file(dir.path(), list[3]).exists(),
        "先読みは 2 区間まで"
    );

    let started = Instant::now();
    let again = s.segment(list[0]).unwrap();
    assert_eq!(again.body, first.body);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "作り直さずに返る"
    );
}

#[test]
fn 編集で変わらない区間は同じキーで作り直さず_変わった区間だけ作り直す() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let mut before = project(&media, 10);
    let title = |red: f32| {
        let node = Node::new("solid", 1).with_param(
            "color",
            ParamValue::Color(Rgba {
                r: red,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
        );
        Layer::new(
            "title",
            node,
            RationalTime::seconds(9),
            RationalTime::ZERO,
            RationalTime::seconds(1),
        )
    };
    before.compositions[0].layers.push(title(1.0));
    let s = streamer(config, dir.path());
    let old = keys(&s.playlist(&before, 1, &req(), "seg/").unwrap());
    let bodies: Vec<Payload> = old.iter().map(|k| s.segment(*k).unwrap()).collect();

    // 9〜10 秒のレイヤーの色だけを変えると、8〜10 秒の区間だけキーが変わる。
    let mut after = before.clone();
    after.compositions[0].layers[1] = title(0.0);
    let fresh = keys(&s.playlist(&after, 2, &req(), "seg/").unwrap());
    assert_eq!(fresh.len(), 3);
    assert_eq!(&fresh[..2], &old[..2]);
    assert_ne!(fresh[2], old[2]);
    let started = Instant::now();
    let reused = s.segment(fresh[0]).unwrap();
    assert_eq!(reused.body, bodies[0].body);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "作り直さずに返る"
    );
    let rebuilt = s.segment(fresh[2]).unwrap();
    assert_ne!(rebuilt.body, bodies[2].body);
    let info = probe_segment(dir.path(), &rebuilt.body, "rebuilt.ts");
    assert!((number(&info["format"]["duration"]) - 2.0).abs() < 0.3);
}

#[test]
fn プレイリストに無いキーと無いコンポジションは拒む() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let s = streamer(config, dir.path());
    let unknown: SegmentKey = "0123456789abcdef".parse().unwrap();
    assert!(matches!(
        s.segment(unknown),
        Err(mm_types::MmError::NotFound(_))
    ));
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let missing = HlsRequest {
        composition: "nope".into(),
        quality: HlsQuality::P360,
    };
    assert!(s.playlist(&project(&media, 10), 1, &missing, "").is_err());
}

/// `hls` の下に、作りかけの区間の一時ファイル（`<キー>.partial.ts`）があるか。
fn has_partial(hls: &Path) -> bool {
    std::fs::read_dir(hls).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.file_name().to_string_lossy().contains(".partial."))
    })
}

#[test]
fn 区間を作っている途中にも_engine_を借用する仕事が割り込む() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("media.mp4");
    make_media(&media);
    let project = project(&media, 10);
    let s = Arc::new(streamer(config, dir.path()));
    let list = keys(&s.playlist(&project, 1, &req(), "seg/").unwrap());
    let done = Arc::new(AtomicBool::new(false));
    let fetch = {
        let (s, done, first) = (s.clone(), done.clone(), list[0]);
        std::thread::spawn(move || {
            let body = s.segment(first).map(|p| p.body.len());
            done.store(true, Ordering::SeqCst);
            body
        })
    };
    // GET /frame の代わりに、区間を待つ間ずっと `Engine` を借用する仕事を積み、区間の途中で動けた回数を数える
    let hls = dir.path().join("cache/mobile/hls");
    let mut waits = Vec::new();
    while !done.load(Ordering::SeqCst) {
        let hls = hls.clone();
        let started = Instant::now();
        if s.with_engine(move |_| Ok(has_partial(&hls))).unwrap() {
            waits.push(started.elapsed());
        }
    }
    waits.sort();
    let inside = waits.len();
    eprintln!(
        "区間の途中で動いた仕事 {inside} 回、待ちの中央値 {:?}、最大 {:?}",
        waits.get(inside / 2),
        waits.last()
    );
    assert!(fetch.join().unwrap().unwrap() > 0, "区間ができる");
    assert!(
        inside > 0,
        "区間の一時ファイルがある間（描いている途中）に、積んだ仕事が動く"
    );
}
