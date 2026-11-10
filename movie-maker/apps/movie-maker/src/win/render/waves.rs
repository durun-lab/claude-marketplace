//! 帯の波形の読み込み。キャッシュの波形（`cache/audio/<コンテンツハッシュ>/waveform.json`）を専用のスレッドで読み、
//! 読めた素材から Timeline に渡す。キャッシュにまだ無い素材は、素材の一覧が変わったときと、取り込みが素材の出力を
//! 作り終えたとき（`Shared::ingest_serial`）に探し直す。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use mm_ingest::sidecar::read_sidecar;
use mm_ingest::CacheLayout;
use mm_ingest_api::SidecarBody;
use mm_ui_layout::Peaks;
use mm_ui_wgpu::Ui;

use crate::win::state::Shared;

/// 波形を読みたい素材のコンテンツハッシュと、その一覧の通し番号（一覧を作り直すたびに進む）。
pub(super) struct WaveWanted<'a> {
    pub hashes: &'a [Arc<str>],
    pub serial: u64,
}

/// 素材ごとの読み込みの状態。キャッシュに無かった素材は持たず、次の合図で探し直す。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Load {
    Reading,
    Done,
}

/// 読むスレッドに頼む 1 件。
struct Job {
    hash: Arc<str>,
    path: PathBuf,
    /// 頼んだときの探した回数。読めなかった結果が届く前に次の合図があれば読み直す。
    scan: u64,
}

/// 読んだ結果。読めなければ `peaks` が `None`（キャッシュに無い、ハッシュが違う）。
struct Loaded {
    job: Job,
    peaks: Option<Peaks>,
}

pub(super) struct WaveSync {
    jobs: Sender<Job>,
    done: Receiver<Loaded>,
    state: HashMap<Arc<str>, Load>,
    /// 最後に探したときの合図（素材の一覧と取り込みの通し番号）。`None` なら次の周で探す。
    seen: Option<(u64, u64)>,
    /// 探した回数。
    scans: u64,
}

impl WaveSync {
    pub(super) fn new() -> Self {
        let (jobs, rx) = channel::<Job>();
        let (tx, done) = channel::<Loaded>();
        let spawned = std::thread::Builder::new()
            .name("waveform".into())
            .spawn(move || {
                for job in rx {
                    let peaks = read(&job.path, &job.hash);
                    if tx.send(Loaded { job, peaks }).is_err() {
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
            seen: None,
            scans: 0,
        }
    }

    /// 読んだ状態を捨てる（UI を作り直すと渡したピークも消えるので、次の周で読み直す）。
    pub(super) fn forget(&mut self) {
        self.state.clear();
        self.seen = None;
    }

    /// 読めた素材のピークを `ui` に渡し、合図が変わっていれば、まだ読めていない素材 `wanted` のキャッシュを探す。
    pub(super) fn refresh(&mut self, shared: &Shared, ui: &mut Ui, wanted: WaveWanted) {
        while let Ok(loaded) = self.done.try_recv() {
            self.accept(loaded, |hash, peaks| ui.set_wave(hash, peaks));
        }
        self.rescan(shared, wanted);
    }

    /// 届いた結果を受ける。読めなかった素材は、頼んだ後に合図があったなら読み直し、無ければ次の合図を待つ。
    fn accept(&mut self, Loaded { job, peaks }: Loaded, mut set: impl FnMut(Arc<str>, Peaks)) {
        match peaks {
            Some(peaks) => {
                set(job.hash.clone(), peaks);
                self.state.insert(job.hash, Load::Done);
            }
            None if job.scan < self.scans => self.request(job.hash, job.path),
            None => {
                self.state.remove(&job.hash);
            }
        }
    }

    /// 合図（素材の一覧か取り込みの通し番号）が前に探したときから変わっていれば、読んでいない素材を読むスレッドに頼む。
    fn rescan(&mut self, shared: &Shared, wanted: WaveWanted) {
        let signal = (wanted.serial, shared.ingest_serial.load(Ordering::SeqCst));
        if self.seen == Some(signal) {
            return;
        }
        let Some(root) = shared.cache_dir.get() else {
            return;
        };
        self.seen = Some(signal);
        self.scans += 1;
        let layout = CacheLayout::new(root.clone());
        for hash in wanted.hashes {
            if !self.state.contains_key(hash) {
                self.request(hash.clone(), layout.waveform(hash));
            }
        }
    }

    fn request(&mut self, hash: Arc<str>, path: PathBuf) {
        let job = Job {
            hash: hash.clone(),
            path,
            scan: self.scans,
        };
        if self.jobs.send(job).is_ok() {
            self.state.insert(hash, Load::Reading);
        }
    }
}

/// 波形の付属ファイルを読む。読めなければ `None`（無い、ハッシュが違う）。付属ファイルは書き終えてから名前を付けるので、
/// 書きかけは見えない。
fn read(path: &Path, hash: &str) -> Option<Peaks> {
    let sidecar = read_sidecar(path, hash).ok()?;
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
mod tests;
