//! 静止画のフレーム。

use std::time::Instant;

use mm_node_api::Quality;
use mm_stream::{StillRequest, STILL_MAX_WIDTH, STILL_MIN_WIDTH};
use mm_testkit::decode::SampleMedia;
use mm_testkit::fixtures::vertical_project;
use mm_types::{RationalTime, Size};

use crate::common::{gpu_config, streamer};

fn request(width: u32) -> StillRequest {
    StillRequest {
        composition: None,
        time: RationalTime::new(1, 2).unwrap(),
        width,
    }
}

fn dimensions(webp: &[u8]) -> (u32, u32) {
    let image = webp::Decoder::new(webp)
        .decode()
        .expect("WebP として読める");
    (image.width(), image.height())
}

#[test]
fn 静止画のフレームは_webp_で返り幅は範囲に丸められキャッシュされる() {
    let Some(config) = gpu_config() else { return };
    let dir = tempfile::tempdir().unwrap();
    let (project, _, _) = vertical_project(&SampleMedia::default(), &dir.path().join("o.json"));
    let s = streamer(config, dir.path());

    let first = s.still(&project, 1, &request(480)).unwrap();
    assert_eq!(first.status, 200);
    assert_eq!(first.header("Content-Type"), Some("image/webp"));
    assert_eq!(&first.body[..4], b"RIFF");
    assert_eq!(&first.body[8..12], b"WEBP");
    let (w, h) = dimensions(&first.body);
    assert_eq!(w, 480);
    let comp = &project.compositions[0];
    assert_eq!(h, 480 * comp.height / comp.width);
    assert!(first.body.len() < 100 * 1024, "{} バイト", first.body.len());

    let started = Instant::now();
    let again = s.still(&project, 1, &request(480)).unwrap();
    assert_eq!(again.body, first.body);
    assert_eq!(again.header("ETag"), first.header("ETag"));
    assert!(started.elapsed().as_millis() < 50, "キャッシュから返る");

    let narrow = s.still(&project, 1, &request(10)).unwrap();
    assert_eq!(dimensions(&narrow.body).0, STILL_MIN_WIDTH);
    let wide = s.still(&project, 1, &request(5000)).unwrap();
    assert_eq!(dimensions(&wide.body).0, STILL_MAX_WIDTH);
}

#[test]
fn バージョンが変わると_etag_が変わる() {
    let Some(config) = gpu_config() else { return };
    let dir = tempfile::tempdir().unwrap();
    let (project, _, _) = vertical_project(&SampleMedia::default(), &dir.path().join("o.json"));
    let s = streamer(config, dir.path());
    let a = s.still(&project, 1, &request(400)).unwrap();
    let b = s.still(&project, 2, &request(400)).unwrap();
    assert_ne!(a.header("ETag"), b.header("ETag"));
}

#[test]
fn 無いコンポジションはエラーになる() {
    let Some(config) = gpu_config() else { return };
    let dir = tempfile::tempdir().unwrap();
    let (project, _, _) = vertical_project(&SampleMedia::default(), &dir.path().join("o.json"));
    let s = streamer(config, dir.path());
    let req = StillRequest {
        composition: Some("nope".into()),
        ..request(400)
    };
    assert!(s.still(&project, 1, &req).is_err());
}

#[test]
fn 配信の描画のスレッドの_engine_を借用して描ける() {
    let Some(config) = gpu_config() else { return };
    let dir = tempfile::tempdir().unwrap();
    let (project, _, _) = vertical_project(&SampleMedia::default(), &dir.path().join("o.json"));
    let s = streamer(config, dir.path());
    let comp = project.compositions[0].id.clone();
    let size = Size::new(64, 36);
    let drawn = s
        .with_engine(move |e| {
            let image = e.render_frame(&project, &comp, RationalTime::ZERO, size, Quality::Full)?;
            Ok(image.size())
        })
        .unwrap();
    assert_eq!(drawn, size);
}
