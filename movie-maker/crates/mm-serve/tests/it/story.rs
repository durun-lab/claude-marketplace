//! Story の読み出しの HTTP（`story_routes`）の結合テスト。認証は付けず、ルーターだけを立てる。

use std::fs;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::Path;
use std::sync::Arc;

use mm_doc::{Asset, Project};
use mm_serve::{story_routes, Client, Core};
use mm_store::Store;
use mm_types::{ColorSpace, Id, RationalTime};
use serde_json::Value;

const NOW: i64 = 1_700_000_000_000;
const HASH: &str = "ca152c0602faf08d897d38f301d5883036d18f9155cac34ae0cc582653c33f75";
const ASSET: &str = "a1";

struct Rig {
    _tmp: tempfile::TempDir,
    _runtime: tokio::runtime::Runtime,
    client: Client,
}

fn rig() -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p");
    let mut project = Project::new();
    project.assets.push(Asset {
        id: Id::new(ASSET),
        path: "C:/media/speech-ja.wav".into(),
        content_hash: HASH.into(),
        color_space: ColorSpace::bt709(),
        duration: RationalTime::seconds(5),
        fps: None,
        timebase_offset: RationalTime::zero(),
    });
    let store = Store::create(&dir, &project, NOW).unwrap();
    let sidecar = dir
        .join("cache/ingest")
        .join(format!("{HASH}.transcript.json"));
    fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ingest/transcript-ja.json");
    fs::copy(fixture, sidecar).unwrap();

    let core = Core::new(store, None);
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let router = story_routes(Arc::clone(&core));
    runtime.spawn(async move {
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        axum::serve(listener, router).await.unwrap();
    });
    Rig {
        _tmp: tmp,
        _runtime: runtime,
        client: Client::new(port, "unused"),
    }
}

fn get(rig: &Rig, path: &str) -> (u16, Value) {
    let res = rig.client.get(path).unwrap();
    (res.status, res.json().unwrap())
}

#[test]
fn 概要は素材と文字起こしの有無を返す() {
    let rig = rig();
    let (status, body) = get(&rig, "/story/overview");
    assert_eq!(status, 200);
    assert_eq!(body["assets"][0]["id"], ASSET);
    assert_eq!(body["assets"][0]["name"], "speech-ja.wav");
    assert_eq!(body["assets"][0]["has_transcript"], true);
    assert_eq!(body["open_sequence"], Value::Null);
}

#[test]
fn 文字起こしと範囲と検索は_story_api_と同じ形() {
    let rig = rig();
    let (status, t) = get(&rig, &format!("/story/transcript?asset={ASSET}"));
    assert_eq!(status, 200);
    let words = t["words"].as_array().unwrap();
    assert!(!words.is_empty());
    assert!(words[0]["id"].is_string() && words[0]["start"]["num"].is_i64());
    let (first, last) = (&words[0]["id"], &words[1]["id"]);

    let path = format!(
        "/story/range?asset={ASSET}&first={}&last={}",
        first.as_str().unwrap(),
        last.as_str().unwrap()
    );
    let (status, range) = get(&rig, &path);
    assert_eq!(status, 200);
    assert_eq!(range["start"], words[0]["start"]);

    let (status, hits) = get(&rig, "/story/search?q=%E6%96%87%E5%AD%97");
    assert_eq!(status, 200);
    assert!(hits[0]["word"].is_string() && hits[0]["context"].is_string());
}

#[test]
fn 知らない素材や空の検索語は_400() {
    let rig = rig();
    let (status, body) = get(&rig, "/story/transcript?asset=none");
    assert_eq!(status, 400);
    assert_eq!(body["error"]["code"], "bad_request");
    assert_eq!(get(&rig, "/story/search?q=").0, 400);
}
