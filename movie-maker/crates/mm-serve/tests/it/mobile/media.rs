//! モバイルの映像の道。コマ、HLS のプレイリストと区間、素材の音だけ。

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use mm_doc::{
    Asset, Composition, Cut, Layer, LayerAudio, Node, Project, Sequence, SEQUENCE_KIND,
    SEQUENCE_PARAM,
};
use mm_runtime::{assemble, Backend, RuntimeConfig};
use mm_serve::{Client, Core, EngineFrames, MobileConfig, MobileServer, Response, Server};
use mm_store::Store;
use mm_types::color::ColorSpace;
use mm_types::{Id, MmError, ParamValue, Rational, RationalTime, Rgba};
use serde_json::{json, Value};

use super::{code, rig, Req, Rig, HOST, NOW};

/// 素材の長さと、コンポジションに使う長さ（秒）。10 秒は 4 秒の区間で 3 本になる。
const MEDIA_SECONDS: i64 = 14;
const USED_SECONDS: i64 = 10;
/// プレイリストの区間の URI（相対パス）の前置きと、それを `main` のプレイリストから解いたパス。
const SEGMENT_URI_PREFIX: &str = "seg/";
const SEGMENTS: &str = "/m/api/stream/main/seg/";

/// 組めない理由が FFmpeg の無い設定なら `None`（`MM_REQUIRE_GPU` があれば GPU の無い失敗は落とす）。
fn usable(config: RuntimeConfig) -> Option<RuntimeConfig> {
    match assemble(&config) {
        Ok(_) => Some(config),
        Err(MmError::Unsupported(e)) => {
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

fn gpu_config() -> Option<RuntimeConfig> {
    usable(RuntimeConfig {
        render: Backend::Wgpu,
        audio: Backend::Dsp,
        ..RuntimeConfig::sample()
    })
}

fn ffmpeg_config() -> Option<RuntimeConfig> {
    usable(RuntimeConfig {
        decode: Backend::Ffmpeg,
        encode: Backend::Ffmpeg,
        ..RuntimeConfig::real()
    })
}

/// ffmpeg の CLI で、動きのある映像と音の素材を作る。
fn make_media(path: &Path) {
    let status = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-y"])
        .args(["-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"])
        .args(["-t", &MEDIA_SECONDS.to_string()])
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac"])
        .args(["-shortest"])
        .arg(path)
        .status()
        .expect("ffmpeg を起動できない");
    assert!(status.success());
}

fn title(red: f32) -> Layer {
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
}

/// 素材 1 本を使うシーケンスのレイヤー（音つき）と、9〜10 秒だけの `title` を置いた 1280x720 のコンポジション。
fn project(media: &Path) -> Project {
    let fps = Rational::integer(30);
    let footage =
        Node::new("footage", 1).with_param("asset", ParamValue::AssetRef(Id::new("clip")));
    let mut p = Project::new();
    p.assets.push(Asset {
        id: Id::new("clip"),
        path: media.to_string_lossy().into_owned(),
        content_hash: "mobile-stream-clip".into(),
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
            RationalTime::seconds(USED_SECONDS),
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
            RationalTime::seconds(USED_SECONDS),
        )
    };
    p.compositions.push(
        Composition::new(
            Id::new("main"),
            1280,
            720,
            fps,
            RationalTime::seconds(USED_SECONDS),
        )
        .with_layers(vec![layer, title(1.0)]),
    );
    p
}

/// 描画を `EngineFrames`（`Streamer` の `Engine`）にした、Claude と人の待ち受け。
fn stream_rig(project: &Project, config: RuntimeConfig) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p");
    let store = Store::create(&dir, project, NOW).unwrap();
    let frames = EngineFrames::spawn(config, dir.join("cache/mobile")).unwrap();
    let core = Core::new(store, Some(Arc::new(frames)));
    let server = Server::start(core.clone()).unwrap();
    let claude = Client::new(server.port(), server.token());
    let mut mobile_config = MobileConfig::new(vec![HOST.into()], tmp.path().join("devices.json"));
    mobile_config.port = 0;
    let mobile = MobileServer::start(core, mobile_config).unwrap();
    Rig {
        tmp,
        claude,
        mobile,
        _server: server,
    }
}

fn get(r: &Rig, cookie: &str, path: &str) -> Response {
    r.send(&Req::device(cookie), "GET", path, None)
}

fn get_range(r: &Rig, cookie: &str, path: &str, range: &str) -> Response {
    let mut headers = Req::device(cookie).headers();
    headers.push(("Range".into(), range.into()));
    Client::new(r.mobile.port(), "")
        .send("GET", path, &headers, None)
        .unwrap()
}

/// プレイリストの区間のキー（URI の最後の部分）。
fn keys(playlist: &Response) -> Vec<String> {
    String::from_utf8_lossy(&playlist.body)
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            l.strip_prefix(SEGMENT_URI_PREFIX)
                .unwrap_or_else(|| panic!("区間の URI はプレイリストからの相対パス: {l}"))
                .to_owned()
        })
        .collect()
}

fn opacity_of_title(value: f64, base_version: i64) -> Value {
    json!({
        "op": {"op": "setParam", "target": {"layer": "title", "path": "opacity"},
               "value": {"type": "number", "value": value}},
        "base_version": base_version,
    })
}

const MEDIA_PATHS: [&str; 4] = [
    "/m/api/frame?t=0",
    "/m/api/stream/main/360p.m3u8",
    "/m/api/stream/main/seg/0123456789abcdef.ts",
    "/m/api/audio/clip.m4a",
];

#[test]
fn 未登録の端末は映像の全部のパスで_401() {
    let r = rig();
    for path in MEDIA_PATHS {
        let res = r.send(&Req::tailnet(), "GET", path, None);
        assert_eq!(res.status, 401, "{path}");
        assert_eq!(code(&res), "no_device", "{path}");
    }
}

#[test]
fn 配信を持たない待ち受けと誤った指定は状態で区別して返す() {
    let r = rig();
    let cookie = r.pair("phone");
    for path in MEDIA_PATHS {
        assert_eq!(get(&r, &cookie, path).status, 503, "{path}");
    }
    let bad_quality = get(&r, &cookie, "/m/api/stream/main/720p.m3u8");
    assert_eq!(
        (bad_quality.status, code(&bad_quality)),
        (400, "bad_request".into())
    );
    assert_eq!(get(&r, &cookie, "/m/api/stream/main/360p.txt").status, 404);
    assert_eq!(get(&r, &cookie, "/m/api/stream/main/seg/zz.ts").status, 400);
    assert_eq!(get(&r, &cookie, "/m/api/audio/clip.mp3").status, 404);
    assert_eq!(get(&r, &cookie, "/m/api/frame?t=abc").status, 400);
}

#[test]
fn project_はカットの境の秒を返す() {
    let r = rig();
    let cookie = r.pair("phone");
    let res = get(&r, &cookie, "/m/api/project");
    assert_eq!(res.status, 200);
    assert!(res.json().unwrap()["cuts"].is_array());
}

#[test]
fn seg_という名のコンポジションもプレイリストを読め_区間は相対パスで指す() {
    let Some(config) = gpu_config() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let mut p = project(&tmp.path().join("media.mp4"));
    p.compositions[0].layers.remove(0);
    p.compositions[0].layers[0].start = RationalTime::ZERO;
    p.compositions[0].id = Id::new("seg");
    let r = stream_rig(&p, config);
    let cookie = r.pair("phone");

    let list = get(&r, &cookie, "/m/api/stream/seg/360p.m3u8");
    assert_eq!(list.status, 200, "{}", list.error_message());
    assert_eq!(keys(&list).len(), 3);
    // 今のプレイリストに無いキーは、コンポジションの名前に依らず区間の道で 404。
    let unknown = get(&r, &cookie, "/m/api/stream/seg/seg/0123456789abcdef.ts");
    assert_eq!((unknown.status, code(&unknown)), (404, "not_found".into()));
}

#[test]
fn コマは_webp_で返り_etag_が合えば_304_で_claude_の_frame_も同じ_engine_で描く() {
    let Some(config) = gpu_config() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let media = tmp.path().join("media.mp4");
    // 素材に頼らないコマを描くため、素材のレイヤーを外して title だけにする。
    let mut p = project(&media);
    p.compositions[0].layers.remove(0);
    p.compositions[0].layers[0].start = RationalTime::ZERO;
    let r = stream_rig(&p, config);
    let cookie = r.pair("phone");

    let first = get(&r, &cookie, "/m/api/frame?t=0.5&width=400");
    assert_eq!(first.status, 200, "{}", first.error_message());
    assert_eq!(first.header("content-type"), Some("image/webp"));
    assert_eq!(&first.body[8..12], b"WEBP");
    let etag = first.header("etag").unwrap().to_owned();
    let mut headers = Req::device(&cookie).headers();
    headers.push(("If-None-Match".into(), etag));
    let again = Client::new(r.mobile.port(), "")
        .send("GET", "/m/api/frame?t=0.5&width=400", &headers, None)
        .unwrap();
    assert_eq!(again.status, 304);
    assert!(again.body.is_empty());

    let png = r.claude.get("/frame?t=0.5&width=320").unwrap();
    assert_eq!(png.status, 200, "{}", png.error_message());
    assert_eq!(&png.body[1..4], b"PNG");

    // 編集の後はバージョンが進み、コマは作り直される。
    let before = r.version();
    let edit = r
        .claude
        .post("/ops", &opacity_of_title(0.5, before))
        .unwrap();
    assert_eq!(edit.status, 200, "{}", edit.error_message());
    let after = get(&r, &cookie, "/m/api/frame?t=0.5&width=400");
    assert_ne!(after.header("etag"), first.header("etag"));
}

#[test]
fn 提案を適用したコマは_png_で保存したデータを変えない() {
    let Some(config) = gpu_config() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let mut p = project(&tmp.path().join("media.mp4"));
    p.compositions[0].layers.remove(0);
    p.compositions[0].layers[0].start = RationalTime::ZERO;
    let r = stream_rig(&p, config);
    let cookie = r.pair("phone");
    let version = r.version();

    let made = r.send(
        &Req::device(&cookie),
        "POST",
        "/m/api/proposals",
        Some(&opacity_of_title(0.0, version)),
    );
    assert_eq!(made.status, 200, "{}", made.error_message());
    let id = made.json().unwrap()["id"].as_i64().unwrap();

    let preview = get(&r, &cookie, &format!("/m/api/frame?t=0.5&proposal={id}"));
    assert_eq!(preview.status, 200, "{}", preview.error_message());
    assert_eq!(preview.header("content-type"), Some("image/png"));
    assert_eq!(r.version(), version);
    let missing = get(&r, &cookie, "/m/api/frame?t=0.5&proposal=9999");
    // 無い提案は Claude の `GET /frame?proposal=` と同じ状態（操作の層の拒否）で断る。
    assert_eq!(missing.status, 422);
}

#[test]
fn プレイリストと区間が往復し_音は_range_に答え_編集で変わらない区間はキーが同じ() {
    let Some(config) = ffmpeg_config() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let media = tmp.path().join("media.mp4");
    make_media(&media);
    let r = stream_rig(&project(&media), config);
    let cookie = r.pair("phone");

    let list = get(&r, &cookie, "/m/api/stream/main/360p.m3u8");
    assert_eq!(list.status, 200, "{}", list.error_message());
    assert_eq!(
        list.header("content-type"),
        Some("application/vnd.apple.mpegurl")
    );
    let old = keys(&list);
    assert_eq!(old.len(), 3);

    let bodies: Vec<Response> = old
        .iter()
        .map(|k| get(&r, &cookie, &format!("{SEGMENTS}{k}")))
        .collect();
    for seg in &bodies {
        assert_eq!(seg.status, 200, "{}", seg.error_message());
        assert_eq!(seg.header("content-type"), Some("video/mp2t"));
        assert!(seg.header("cache-control").unwrap().contains("immutable"));
        assert_eq!(seg.body[0], 0x47, "MPEG-TS の同期バイト");
    }
    let unknown = get(&r, &cookie, &format!("{SEGMENTS}0123456789abcdef.ts"));
    assert_eq!((unknown.status, code(&unknown)), (404, "not_found".into()));
    let no_comp = get(&r, &cookie, "/m/api/stream/nothing/360p.m3u8");
    assert_eq!(no_comp.status, 404);

    let audio = get(&r, &cookie, "/m/api/audio/clip.m4a");
    assert_eq!(audio.status, 200, "{}", audio.error_message());
    assert_eq!(audio.header("accept-ranges"), Some("bytes"));
    let total = audio.body.len();
    let head = get_range(&r, &cookie, "/m/api/audio/clip.m4a", "bytes=0-99");
    assert_eq!(head.status, 206);
    assert_eq!(head.body, audio.body[..100]);
    assert_eq!(
        head.header("content-range"),
        Some(format!("bytes 0-99/{total}").as_str())
    );
    let beyond = get_range(
        &r,
        &cookie,
        "/m/api/audio/clip.m4a",
        &format!("bytes={total}-"),
    );
    assert_eq!(beyond.status, 416);

    // 9〜10 秒の title の不透明度だけを変えると、8〜10 秒の区間だけキーが変わる。
    let before = r.version();
    let edit = r
        .claude
        .post("/ops", &opacity_of_title(0.5, before))
        .unwrap();
    assert_eq!(edit.status, 200, "{}", edit.error_message());
    let fresh = keys(&get(&r, &cookie, "/m/api/stream/main/360p.m3u8"));
    assert_eq!(fresh.len(), 3);
    assert_eq!(&fresh[..2], &old[..2]);
    assert_ne!(fresh[2], old[2]);
    let reused = get(&r, &cookie, &format!("{SEGMENTS}{}", fresh[0]));
    assert_eq!(reused.body, bodies[0].body);
    let rebuilt = get(&r, &cookie, &format!("{SEGMENTS}{}", fresh[2]));
    assert_eq!(rebuilt.status, 200, "{}", rebuilt.error_message());
    assert_ne!(rebuilt.body, bodies[2].body);
}
