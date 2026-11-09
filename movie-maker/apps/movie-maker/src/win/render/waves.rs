//! 帯の波形の読み込み。キャッシュの波形（`cache/waveform/`）を専用のスレッドで読み、読めた素材から Timeline に渡す。
//! 取り込みの途中でまだ無い素材は、取り込みが終われば出るよう、間を置いて探し直す。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mm_ingest::sidecar::read_sidecar;
use mm_ingest::CacheLayout;
use mm_ingest_api::SidecarBody;
use mm_ui_layout::Peaks;
use mm_ui_wgpu::Ui;

use crate::win::state::Shared;

/// キャッシュに無かった素材を探し直す間隔。
const RESCAN: Duration = Duration::from_secs(2);

/// 読む素材の ID とコンテンツハッシュ。
pub(super) type Wanted = (Arc<str>, String);

/// 読みたい素材の一覧とその版（一覧を作り直すたびに進む）。
pub(super) struct WaveWanted<'a> {
    pub assets: &'a [Wanted],
    pub rev: u64,
}

/// 素材ごとの読み込みの状態。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Load {
    Reading,
    Done,
    /// キャッシュに無い、または読めなかった。間を置いて探し直す。
    Absent,
}

struct Job {
    asset: Arc<str>,
    path: PathBuf,
    hash: String,
}

type Done = (Arc<str>, Option<Peaks>);

pub(super) struct WaveSync {
    jobs: Sender<Job>,
    done: Receiver<Done>,
    state: HashMap<Arc<str>, Load>,
    next_scan: Instant,
    seen_rev: u64,
}

impl WaveSync {
    pub(super) fn new() -> Self {
        let (jobs, rx) = channel::<Job>();
        let (tx, done) = channel::<Done>();
        let spawned = std::thread::Builder::new()
            .name("waveform".into())
            .spawn(move || {
                for job in rx {
                    let peaks = read(&job);
                    if tx.send((job.asset, peaks)).is_err() {
                        break;
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("波形を読むスレッドを始められない: {e}");
        }
        Self {
            jobs,
            done,
            state: HashMap::new(),
            next_scan: Instant::now(),
            seen_rev: 0,
        }
    }

    /// 読んだ状態を捨てる（UI を作り直したとき、読み込み済みのピークも消えるので読み直す）。
    pub(super) fn forget(&mut self) {
        self.state.clear();
        self.next_scan = Instant::now();
    }

    /// 読めた素材のピークを `ui` に渡し、まだ読んでいない素材 `wanted` のキャッシュを探す。
    /// 一覧が変わった周でなければ、`RESCAN` の間隔でだけ探す。
    pub(super) fn refresh(&mut self, shared: &Shared, ui: &mut Ui, wanted: WaveWanted) {
        while let Ok((asset, peaks)) = self.done.try_recv() {
            let load = match peaks {
                Some(peaks) => {
                    ui.set_wave(asset.clone(), peaks);
                    Load::Done
                }
                None => Load::Absent,
            };
            self.state.insert(asset, load);
        }
        let now = Instant::now();
        let changed = std::mem::replace(&mut self.seen_rev, wanted.rev) != wanted.rev;
        if !changed && now < self.next_scan {
            return;
        }
        self.next_scan = now + RESCAN;
        let Some(root) = shared.cache_dir.get() else {
            return;
        };
        let layout = CacheLayout::new(root.clone());
        for (asset, hash) in wanted.assets {
            if matches!(self.state.get(asset), Some(Load::Reading | Load::Done)) {
                continue;
            }
            let path = layout.waveform(hash);
            let load = if path.is_file() {
                let job = Job {
                    asset: asset.clone(),
                    path,
                    hash: hash.clone(),
                };
                if self.jobs.send(job).is_ok() {
                    Load::Reading
                } else {
                    Load::Absent
                }
            } else {
                Load::Absent
            };
            self.state.insert(asset.clone(), load);
        }
    }
}

/// 波形の付属ファイルを読む。読めなければ `None`（取り込みの途中の書きかけも含む）。
fn read(job: &Job) -> Option<Peaks> {
    let sidecar = read_sidecar(&job.path, &job.hash).ok()?;
    let SidecarBody::Waveform(w) = sidecar.body else {
        return None;
    };
    Some(Peaks::new(
        f64::from(w.peaks_per_second),
        usize::from(w.channels),
        &w.peaks,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_ingest_api::{Sidecar, Waveform, SIDECAR_SCHEMA_VERSION};

    fn write_sidecar(dir: &std::path::Path, hash: &str, peaks: Vec<f32>) -> PathBuf {
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
        let path = dir.join("wave.json");
        std::fs::write(&path, serde_json::to_vec(&sidecar).unwrap()).unwrap();
        path
    }

    #[test]
    fn キャッシュの波形を読んでピークにする() {
        let dir = tempfile::tempdir().unwrap();
        // 2 チャンネルのインターリーブ。1 個目は左 0.2 と右 0.5、2 個目は 0.1 と 0.1。
        let path = write_sidecar(dir.path(), "h", vec![0.2, 0.5, 0.1, 0.1]);
        let job = Job {
            asset: "a".into(),
            path,
            hash: "h".into(),
        };
        let peaks = read(&job).expect("読める");
        assert_eq!(peaks.max_in(0.0, 0.01), 0.5);
        assert_eq!(peaks.max_in(0.01, 0.02), 0.1);
    }

    #[test]
    fn ハッシュが違う付属ファイルと無いファイルは読まない() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_sidecar(dir.path(), "other", vec![0.2, 0.5]);
        let job = |path, hash: &str| Job {
            asset: "a".into(),
            path,
            hash: hash.into(),
        };
        assert!(read(&job(path, "h")).is_none());
        assert!(read(&job(dir.path().join("none.json"), "h")).is_none());
    }
}
