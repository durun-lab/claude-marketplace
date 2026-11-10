use std::time::Duration;

use super::*;
use mm_ingest_api::{Sidecar, Waveform, SIDECAR_SCHEMA_VERSION};

/// 読むスレッドの結果を待つ上限（届かなければ失敗にする）。
const WAIT: Duration = Duration::from_secs(10);

/// `path` に 2 チャンネルの波形の付属ファイルを書く。
fn write_sidecar(path: &Path, hash: &str, peaks: Vec<f32>) {
    let sidecar = Sidecar {
        schema_version: SIDECAR_SCHEMA_VERSION,
        content_hash: hash.to_owned(),
        producer: "test".to_owned(),
        body: SidecarBody::Waveform(Waveform {
            peaks_per_second: 100,
            channels: 2,
            peaks,
        }),
    };
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&sidecar).unwrap()).unwrap();
}

#[test]
fn キャッシュの波形を読んで_チャンネルの最大を見た目の振幅にする() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wave.json");
    // 2 チャンネルのインターリーブ。1 個目は左 0.2 と右 0.5、2 個目は 0.1 と 0.1。
    write_sidecar(&path, "h", vec![0.2, 0.5, 0.1, 0.1]);
    let peaks = read(&path, "h").expect("読める");
    assert!((peaks.max_in(0.0, 0.01) - 0.5_f32.sqrt()).abs() < 0.01);
    assert!((peaks.max_in(0.01, 0.02) - 0.1_f32.sqrt()).abs() < 0.01);
}

#[test]
fn ハッシュが違う付属ファイルと無いファイルは読まない() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wave.json");
    write_sidecar(&path, "other", vec![0.2, 0.5]);
    assert!(read(&path, "h").is_none());
    assert!(read(&dir.path().join("none.json"), "h").is_none());
}

#[test]
fn キャッシュに無い素材は合図が変わるまで探さず_取り込みの合図で読む() {
    let dir = tempfile::tempdir().unwrap();
    let shared = Shared::new();
    shared.cache_dir.set(dir.path().to_path_buf()).unwrap();
    let hashes: Vec<Arc<str>> = vec!["h".into()];
    let wanted = || WaveWanted {
        hashes: &hashes,
        serial: 1,
    };
    let mut sync = WaveSync::new();
    let mut got = Vec::new();
    sync.rescan(&shared, wanted());
    let absent = sync.done.recv_timeout(WAIT).expect("結果が届く");
    sync.accept(absent, |hash, _| got.push(hash));
    assert!(got.is_empty() && sync.state.is_empty(), "キャッシュに無い");

    let path = CacheLayout::new(dir.path().to_path_buf()).waveform("h");
    write_sidecar(&path, "h", vec![0.5, 0.5]);
    sync.rescan(&shared, wanted());
    assert!(sync.state.is_empty(), "合図が同じなら探さない");

    shared.ingest_serial.fetch_add(1, Ordering::SeqCst);
    sync.rescan(&shared, wanted());
    assert_eq!(sync.state.get("h"), Some(&Load::Reading));
    let found = sync.done.recv_timeout(WAIT).expect("結果が届く");
    sync.accept(found, |hash, _| got.push(hash));
    assert_eq!(got, [Arc::<str>::from("h")]);
    assert_eq!(sync.state.get("h"), Some(&Load::Done));
}

#[test]
fn 読めなかった結果が届く前に合図があれば_すぐ読み直す() {
    let dir = tempfile::tempdir().unwrap();
    let path = CacheLayout::new(dir.path().to_path_buf()).waveform("h");
    write_sidecar(&path, "h", vec![0.5, 0.5]);
    let mut sync = WaveSync::new();
    // 1 回目に探したときの「無い」が、2 回目の合図の後に届く。
    sync.scans = 2;
    let stale = Loaded {
        job: Job {
            hash: "h".into(),
            path,
            scan: 1,
        },
        peaks: None,
    };
    sync.accept(stale, |_, _| panic!("読めていない"));
    assert_eq!(sync.state.get("h"), Some(&Load::Reading));
    let mut got = Vec::new();
    let fresh = sync.done.recv_timeout(WAIT).expect("読み直しの結果が届く");
    sync.accept(fresh, |hash, _| got.push(hash));
    assert_eq!(got, [Arc::<str>::from("h")]);
}
