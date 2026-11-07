//! PWA の静的な配信（`GET /m/`）。資格の無い端末にも配り、`/m/api/` の未知のパスは画面にしない。

use std::sync::Arc;

use mm_serve::DirAssets;

use super::{code, rig_with, Req, Rig};

/// `ui/mobile` のビルドの出力に似た小さな置き場を付けた待ち受け。
fn rig_with_assets() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("index.html", "<!doctype html><title>mm</title>"),
        ("app.js", "console.log(1)"),
        ("sw.js", "self.skipWaiting()"),
        ("manifest.webmanifest", r#"{"scope":"./"}"#),
    ];
    for (name, body) in files {
        std::fs::write(dir.path().join(name), body).unwrap();
    }
    // 置き場は一時フォルダーの置き場所なので、試験の後は OS の一時フォルダーの掃除に任せる。
    let root = dir.keep();
    rig_with(|c| c.assets = Some(Arc::new(DirAssets::new(root))))
}

#[test]
fn 画面と登録の画面は資格が無くても同じ入口を返す() {
    let r = rig_with_assets();
    for path in ["/m/", "/m/pair", "/m/watch/c1"] {
        let res = r.send(&Req::tailnet(), "GET", path, None);
        assert_eq!(res.status, 200, "{path}");
        assert_eq!(res.header("content-type"), Some("text/html; charset=utf-8"));
        assert!(String::from_utf8_lossy(&res.body).contains("<title>mm</title>"));
    }
}

#[test]
fn ファイルは型を付けて返し_service_worker_のスコープを絞る() {
    let r = rig_with_assets();
    let sw = r.send(&Req::tailnet(), "GET", "/m/sw.js", None);
    assert_eq!(sw.status, 200);
    assert_eq!(
        sw.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(sw.header("service-worker-allowed"), Some("/m/"));
    assert_eq!(sw.header("cache-control"), Some("no-cache"));
    let manifest = r.send(&Req::tailnet(), "GET", "/m/manifest.webmanifest", None);
    assert_eq!(
        manifest.header("content-type"),
        Some("application/manifest+json")
    );
}

#[test]
fn 無いファイルと_api_の未知のパスは_404() {
    let r = rig_with_assets();
    let device = Req::device(&r.pair("phone"));
    for path in ["/m/missing.js", "/m/api/nothing", "/m/%2e%2e/secret.txt"] {
        let res = r.send(&device, "GET", path, None);
        assert_eq!(res.status, 404, "{path}");
    }
}

#[test]
fn 置き場が無いときは_404_で_host_は確かめる() {
    let r = rig_with(|_| {});
    assert_eq!(r.send(&Req::tailnet(), "GET", "/m/", None).status, 404);
    let bad_host = Req {
        host: Some("evil.example".into()),
        ..Req::tailnet()
    };
    assert_eq!(
        code(&r.send(&bad_host, "GET", "/m/", None)),
        "forbidden_host"
    );
}

#[test]
fn 登録の_post_は画面の_get_と同じパスで受ける() {
    let r = rig_with_assets();
    let cookie = r.pair("phone");
    assert!(cookie.starts_with("mm_device="));
    assert_eq!(r.send(&Req::tailnet(), "GET", "/m/pair", None).status, 200);
}
