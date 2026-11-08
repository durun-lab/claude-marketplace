//! 提案の区間の短い動画（`GET /m/api/proposals/{id}/clip/{key}.mp4`）と、提案の応答の `clip_url`。

use mm_serve::{Client, Response};
use serde_json::json;

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

/// 提案の応答の `clip_url`（`query` は `?comp=&ops=`）。出せなければ `None`。
fn clip_url(r: &Rig, cookie: &str, id: i64, query: &str) -> Option<String> {
    let res = get(r, cookie, &format!("/m/api/proposals/{id}{query}"));
    assert_eq!(res.status, 200, "{}", res.error_message());
    res.json().unwrap()["clip_url"].as_str().map(str::to_owned)
}

/// ffprobe で読んだ長さ（秒）と、`at` 秒のフレームの赤の平均（0〜255）。
fn probe(r: &Rig, clip: &Response, at: f64) -> (f64, f64) {
    let path = r.tmp.path().join("clip.mp4");
    std::fs::write(&path, &clip.body).unwrap();
    let out = mm_types::process::command("ffprobe")
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
    let frame = mm_types::process::command("ffmpeg")
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
    let path = clip_url(&r, &cookie, id, "").expect("保留中で範囲のある提案は URL を返す");
    assert!(
        path.starts_with(&format!("/m/api/proposals/{id}/clip/"))
            && path.contains(".mp4?comp=main"),
        "{path}"
    );

    let clip = get(&r, &cookie, &path);
    assert_eq!(clip.status, 200, "{}", clip.error_message());
    assert_eq!(clip.header("content-type"), Some("video/mp4"));
    assert_eq!(clip.header("accept-ranges"), Some("bytes"));
    assert!(clip.header("cache-control").unwrap().contains("immutable"));
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
    let one = clip_url(&r, &cookie, id, "?ops=0").unwrap();
    assert_eq!(
        one.split('?').next(),
        path.split('?').next(),
        "全部を選んだのと同じ操作は同じキー"
    );
}

#[test]
fn 範囲の中が編集されると_url_が変わり_前の_url_の続きは前のバイトで答える() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let media = tmp.path().join("media.mp4");
    make_media(&media);
    let r = stream_rig(&project(&media), config);
    let cookie = r.pair("phone");
    let id = propose(&r, &cookie, 0.0);
    let old = clip_url(&r, &cookie, id, "").unwrap();
    let played = get(&r, &cookie, &old);
    assert_eq!(played.status, 200, "{}", played.error_message());

    // 短い動画の 7〜10 秒に掛かるシーケンスのレイヤーを PC で薄くする。
    let edit = r
        .claude
        .post(
            "/ops",
            &json!({
                "op": {"op": "setParam", "target": {"layer": "l-sequence", "path": "opacity"},
                       "value": {"type": "number", "value": 0.5}},
                "base_version": r.version(),
            }),
        )
        .unwrap();
    assert_eq!(edit.status, 200, "{}", edit.error_message());
    let fresh = clip_url(&r, &cookie, id, "").unwrap();
    assert_ne!(fresh, old, "範囲の中の編集で URL のキーが変わる");

    let rest = get_range(&r, &cookie, &old, "bytes=100-199");
    assert_eq!(rest.status, 206, "{}", rest.error_message());
    assert_eq!(
        rest.body,
        played.body[100..200],
        "再生の続きは前の動画のバイト"
    );
    let unknown = format!("/m/api/proposals/{id}/clip/0123456789abcdef.mp4?comp=main");
    let stale = get(&r, &cookie, &unknown);
    assert_eq!((stale.status, code(&stale)), (404, "not_found".into()));
    let bad = get(&r, &cookie, &format!("/m/api/proposals/{id}/clip/nope.mp4"));
    assert_eq!(bad.status, 400);
    let made = get(&r, &cookie, &fresh);
    assert_eq!(made.status, 200, "{}", made.error_message());
    assert_ne!(made.header("etag"), played.header("etag"));
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
    let (hide_url, dim_url) = (
        clip_url(&r, &cookie, hide, "").unwrap(),
        clip_url(&r, &cookie, dim, "").unwrap(),
    );
    assert_ne!(hide_url, dim_url, "操作が違えば別のキー");
    let a = get(&r, &cookie, &hide_url);
    let b = get(&r, &cookie, &dim_url);
    assert_eq!((a.status, b.status), (200, 200));
    assert_ne!(a.header("etag"), b.header("etag"), "操作が違えば別の動画");
    assert_eq!(clip_url(&r, &cookie, dim, "?comp=other"), None);
    let elsewhere = get(
        &r,
        &cookie,
        &format!("/m/api/proposals/{dim}/clip/0123456789abcdef.mp4?comp=other"),
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
    assert_eq!(
        clip_url(&r, &cookie, dim, ""),
        None,
        "保留中でない提案は URL を返さない"
    );
    let decided = get(&r, &cookie, &dim_url);
    assert_eq!(
        decided.status, 422,
        "保留中でない提案は作り終えた動画も返さない"
    );
    let missing = get(
        &r,
        &cookie,
        "/m/api/proposals/9999/clip/0123456789abcdef.mp4",
    );
    assert_eq!(missing.status, 422, "{}", code(&missing));

    // 提案が触る title を PC で外すと、保留中の提案は今のプロジェクトに当てられない。
    let removed = r
        .claude
        .post(
            "/ops",
            &json!({"op": {"op": "removeLayer", "layer": "title"}, "base_version": r.version()}),
        )
        .unwrap();
    assert_eq!(removed.status, 200, "{}", removed.error_message());
    assert_eq!(
        clip_url(&r, &cookie, hide, ""),
        None,
        "当てられない提案も詳細は返し、URL だけを出さない"
    );
}
