//! 提案の区間の短い動画（`GET /m/api/proposals/{id}/clip.mp4`）。

use std::process::Command;

use mm_serve::{Client, Response};

use super::media::{
    ffmpeg_config, get, get_range, make_media, opacity_of_title, project, stream_rig,
};
use super::{code, Req, Rig};

/// 9〜10 秒の title の範囲に前後 2 秒を足し、10 秒のコンポジションの端で切った長さ（7〜10 秒）。
const CLIP_SECONDS: f64 = 3.0;
const DURATION_TOLERANCE: f64 = 0.15;

fn propose(r: &Rig, cookie: &str, opacity: f64) -> i64 {
    let made = r.send(
        &Req::device(cookie),
        "POST",
        "/m/api/proposals",
        Some(&opacity_of_title(opacity, r.version())),
    );
    assert_eq!(made.status, 200, "{}", made.error_message());
    made.json().unwrap()["id"].as_i64().unwrap()
}

/// ffprobe で読んだ長さ（秒）と、`at` 秒のフレームの赤の平均（0〜255）。
fn probe(r: &Rig, clip: &Response, at: f64) -> (f64, f64) {
    let path = r.tmp.path().join("clip.mp4");
    std::fs::write(&path, &clip.body).unwrap();
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(&path)
        .output()
        .expect("ffprobe を起動できない");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let seconds: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
    let frame = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-ss", &at.to_string(), "-i"])
        .arg(&path)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("ffmpeg を起動できない");
    assert!(frame.status.success());
    let red = frame
        .stdout
        .iter()
        .step_by(3)
        .map(|&v| f64::from(v))
        .sum::<f64>()
        / (frame.stdout.len() / 3) as f64;
    (seconds, red)
}

#[test]
fn 短い動画は提案を適用した_mp4_で_range_に_206_etag_に_304_で答え_書かない() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let media = tmp.path().join("media.mp4");
    make_media(&media);
    let r = stream_rig(&project(&media), config);
    let cookie = r.pair("phone");
    let version = r.version();
    let id = propose(&r, &cookie, 0.0);
    let path = format!("/m/api/proposals/{id}/clip.mp4");

    let clip = get(&r, &cookie, &path);
    assert_eq!(clip.status, 200, "{}", clip.error_message());
    assert_eq!(clip.header("content-type"), Some("video/mp4"));
    assert_eq!(clip.header("accept-ranges"), Some("bytes"));
    assert_eq!(r.version(), version, "提案を適用した動画を作っても書かない");
    // 短い動画の 2.5 秒はコンポジションの 9.5 秒。提案は赤い title を透明にする。
    let (seconds, red) = probe(&r, &clip, 2.5);
    assert!(
        (seconds - CLIP_SECONDS).abs() < DURATION_TOLERANCE,
        "長さ {seconds}"
    );
    assert!(
        red < 200.0,
        "提案を適用したフレーム（title が消える）: 赤 {red}"
    );

    let len = clip.body.len();
    let head = get_range(&r, &cookie, &path, "bytes=0-1");
    assert_eq!(head.status, 206);
    assert_eq!(head.body, clip.body[..2]);
    assert_eq!(
        head.header("content-range"),
        Some(format!("bytes 0-1/{len}").as_str())
    );

    let etag = clip.header("etag").unwrap().to_owned();
    let mut headers = Req::device(&cookie).headers();
    headers.push(("If-None-Match".into(), etag.clone()));
    let again = Client::new(r.mobile.port(), "")
        .send("GET", &path, &headers, None)
        .unwrap();
    assert_eq!(again.status, 304);
    let one = get(&r, &cookie, &format!("{path}?ops=0"));
    assert_eq!(
        one.header("etag"),
        Some(etag.as_str()),
        "全部を選んだのと同じ操作は同じ動画"
    );
}

#[test]
fn 範囲が違う提案は別の動画で_決めた提案と無い提案は断る() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let media = tmp.path().join("media.mp4");
    make_media(&media);
    let r = stream_rig(&project(&media), config);
    let cookie = r.pair("phone");
    let hide = propose(&r, &cookie, 0.0);
    let dim = propose(&r, &cookie, 0.5);
    let a = get(&r, &cookie, &format!("/m/api/proposals/{hide}/clip.mp4"));
    let b = get(&r, &cookie, &format!("/m/api/proposals/{dim}/clip.mp4"));
    assert_eq!((a.status, b.status), (200, 200));
    assert_ne!(a.header("etag"), b.header("etag"), "操作が違えば別の動画");
    let elsewhere = get(
        &r,
        &cookie,
        &format!("/m/api/proposals/{dim}/clip.mp4?comp=other"),
    );
    assert_eq!(
        (elsewhere.status, code(&elsewhere)),
        (404, "no_range".into())
    );

    let rejected = r.send(
        &Req::device(&cookie),
        "POST",
        &format!("/m/api/proposals/{dim}/reject"),
        None,
    );
    assert_eq!(rejected.status, 200, "{}", rejected.error_message());
    let decided = get(&r, &cookie, &format!("/m/api/proposals/{dim}/clip.mp4"));
    assert_eq!(decided.status, 422, "保留中でない提案は適用して見せない");
    let missing = get(&r, &cookie, "/m/api/proposals/9999/clip.mp4");
    assert_eq!(missing.status, 422, "{}", code(&missing));
}
