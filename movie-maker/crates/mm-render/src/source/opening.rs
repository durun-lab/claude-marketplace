//! 素材をバックグラウンドのスレッドで開く。長い録画は開くときに全パケットを読んで PTS の索引を作る（4〜9 GB で数秒〜
//! 数十秒）ので、描画を呼ぶスレッド（アプリでは再生スレッド）は開き始めたフレームで決めた時間だけ待ち、開き終わらなければ
//! そのレイヤーを描かずに先へ進む。開き終わったデコーダーは、次に同じ素材を求められたときに受け取る。

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mm_decode_api::{Decoder, DecoderFactory, GpuContextRef};
use mm_doc::Asset;
use mm_types::{AssetId, MmError, Result};

use crate::spans::hash_prefix;

/// 素材を開く道具。バックグラウンドのスレッドへ渡せるよう、持ち主の参照で持つ。
#[derive(Clone)]
pub struct Opener {
    pub decoders: Arc<dyn DecoderFactory>,
    pub gpu: Arc<dyn GpuContextRef>,
}

impl Opener {
    /// 呼んだスレッドで開き終わるまで待つ。スパンにはファイル名でなく素材の `contentHash` の先頭を載せる。
    fn open_now(&self, path: &Path, hash: &str) -> Result<Box<dyn Decoder>> {
        let _span = tracing::info_span!("render.open_source", asset = hash).entered();
        self.decoders.open(path, Some(self.gpu.as_ref()))
    }
}

/// 素材を開いている途中のために描かなかったレイヤーの数と、開き終わった数。再生スレッドが描き直す時を決めるのに読む。
/// 複製しても同じ数を指す。
#[derive(Clone, Default)]
pub struct PendingSources(Arc<Counts>);

#[derive(Default)]
struct Counts {
    layers: AtomicUsize,
    opened: AtomicU64,
}

impl PendingSources {
    /// 直前のフレームで、素材を開いている途中のために描かなかったレイヤーの数。
    pub fn layers(&self) -> usize {
        self.0.layers.load(Ordering::SeqCst)
    }

    /// バックグラウンドのスレッドで開き終わった数。増えたら描き直すと、描かなかったレイヤーが入る。
    pub fn opened(&self) -> u64 {
        self.0.opened.load(Ordering::SeqCst)
    }
}

/// バックグラウンドのスレッドで開いている途中の素材 1 つ。
struct Job {
    rx: Receiver<Result<Box<dyn Decoder>>>,
    /// 開き始めたフレームの番号。待つのはこのフレームだけで、後のフレームは開き終わったかを見るだけにする。
    started: u64,
    /// 最後に求められたフレームの番号。
    asked: u64,
}

/// バックグラウンドのスレッドで開いている途中の素材と、描画を呼ぶスレッドが待つ上限。
#[derive(Default)]
pub(crate) struct Opening {
    jobs: HashMap<AssetId, Job>,
    /// 1 フレームで開くのを待つ上限。`None` ならバックグラウンドのスレッドを使わず、開き終わるまで待つ。
    wait: Option<Duration>,
    /// このフレームで待ってよい時刻。フレームの外（音の要求）では待たない。
    deadline: Option<Instant>,
    pending: PendingSources,
}

impl Opening {
    pub(crate) fn set_wait(&mut self, wait: Option<Duration>) {
        self.wait = wait;
    }

    pub(crate) fn pending(&self) -> PendingSources {
        self.pending.clone()
    }

    /// フレームの始め。待ってよい時刻を決め、描かなかったレイヤーの数を 0 に戻す。
    pub(crate) fn begin_frame(&mut self) {
        self.deadline = self.wait.map(|w| Instant::now() + w);
        self.pending.0.layers.store(0, Ordering::SeqCst);
    }

    /// `asset` を開く。待つ上限が無ければ呼んだスレッドで開く。あればバックグラウンドのスレッドで開き、開き始めた
    /// フレームの待ってよい時刻までに開き終わらなければ `None`（次に求められたときに受け取る）。
    pub(crate) fn open(
        &mut self,
        opener: &Opener,
        asset: &Asset,
        frame: u64,
    ) -> Result<Option<Box<dyn Decoder>>> {
        if self.wait.is_none() {
            let hash = hash_prefix(&asset.content_hash);
            return opener.open_now(Path::new(&asset.path), hash).map(Some);
        }
        let job = self.jobs.entry(asset.id.clone()).or_insert_with(|| Job {
            rx: spawn(opener.clone(), asset, self.pending.clone()),
            started: frame,
            asked: frame,
        });
        job.asked = frame;
        let left = match self.deadline {
            Some(d) if job.started == frame => d.saturating_duration_since(Instant::now()),
            _ => Duration::ZERO,
        };
        match job.rx.recv_timeout(left) {
            Ok(opened) => {
                self.jobs.remove(&asset.id);
                opened.map(Some)
            }
            Err(RecvTimeoutError::Timeout) => {
                self.pending.0.layers.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.jobs.remove(&asset.id);
                Err(MmError::Internal(format!(
                    "素材 {} を開くスレッドを起こせなかった",
                    asset.id
                )))
            }
        }
    }

    /// `idle` フレームの間に求められなかった途中のものを捨てる。開き終わったデコーダーはバックグラウンドのスレッドが閉じる。
    pub(crate) fn drop_idle(&mut self, frame: u64, idle: u64) {
        self.jobs.retain(|_, job| frame - job.asked <= idle);
    }

    /// 全部捨てる（デバイスを作り直したとき）。
    pub(crate) fn clear(&mut self) {
        self.jobs.clear();
    }

    /// 素材 ID が `ids` にあるものを捨てる（素材の中身やパスが変わったとき）。
    pub(crate) fn forget(&mut self, ids: &[String]) {
        self.jobs
            .retain(|id, _| !ids.iter().any(|i| i == id.as_str()));
    }

    pub(crate) fn pending_layers(&self) -> usize {
        self.pending.layers()
    }
}

/// バックグラウンドのスレッドで開き、結果を返す受け口。スレッドを起こせなければ送り手ごと捨てられ、受け口が切れたことで分かる。
/// デコーダーが panic してもエラーを送って開き終わった数を進める。送らずに終わると、描かなかったレイヤーが描き直されない。
fn spawn(
    opener: Opener,
    asset: &Asset,
    pending: PendingSources,
) -> Receiver<Result<Box<dyn Decoder>>> {
    let (tx, rx) = mpsc::channel();
    let (id, path) = (asset.id.clone(), std::path::PathBuf::from(&asset.path));
    let hash = hash_prefix(&asset.content_hash).to_owned();
    let job = move || {
        let panicked = || MmError::Internal(format!("素材 {id} を開く途中で panic した"));
        let opened = catch_unwind(AssertUnwindSafe(|| opener.open_now(&path, &hash)))
            .unwrap_or_else(|_| Err(panicked()));
        let _ = tx.send(opened);
        pending.0.opened.fetch_add(1, Ordering::SeqCst);
    };
    let _ = std::thread::Builder::new()
        .name("render-open-source".into())
        .spawn(job);
    rx
}
