//! 配信のインターフェース。描画のスレッド、プレイリストとキーの表、静止画のフレームのキャッシュ、先読みを持つ。

mod clips;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use mm_doc::Project;
use mm_runtime::{Engine, Registry, RuntimeConfig};
use mm_types::{MmError, Result};

use crate::audio::{ensure_audio, respond, AudioRequest};
use crate::cache::{touch, CacheDir};
use crate::clip::ClipJobs;
use crate::frames::{render_webp, StillCache, StillKey, StillRequest};
use crate::hls::{master_playlist, HlsRequest, Plan};
use crate::key::{segment_key, KeyEnv, SegmentKey};
use crate::listing::{Listing, Listings};
use crate::payload::{Payload, IMMUTABLE, REVALIDATE};
use crate::quality::HlsQuality;
use crate::segment::render_segment;
use crate::worker::{lock, run, Interleave, SegJob, Slot, Urgent, Worker};

/// 先読みする区間の数の既定。
const DEFAULT_PREFETCH: u64 = 2;
/// 区間のディスクのキャッシュの上限の既定（バイト）。360p の 1 時間が約 250 MB なので、約 8 時間分。
const DEFAULT_CACHE_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// 提案の区間の短い動画のディスクのキャッシュの上限（バイト）。1 本 数 MB なので、百本ほど。
const CLIP_CACHE_LIMIT_BYTES: u64 = 256 * 1024 * 1024;

/// 配信の設定。
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// 描画とエンコードの実装の選択。
    pub runtime: RuntimeConfig,
    /// キャッシュの置き場所（プロジェクトの `cache/mobile/`）。
    pub cache_dir: PathBuf,
    /// 要求された区間の後ろに先に作る区間の数。
    pub prefetch: u64,
    /// 区間のディスクのキャッシュの上限（バイト）。超えたら使った時刻の古い区間から捨てる。
    pub cache_limit_bytes: u64,
}

impl StreamConfig {
    /// 先読み 2 区間、ディスクのキャッシュ 2 GiB の設定。
    pub fn new(runtime: RuntimeConfig, cache_dir: PathBuf) -> Self {
        Self {
            runtime,
            cache_dir,
            prefetch: DEFAULT_PREFETCH,
            cache_limit_bytes: DEFAULT_CACHE_LIMIT_BYTES,
        }
    }
}

/// モバイルへの映像の配信。`mm-serve` のハンドラーが共有して呼ぶ。
///
/// どの関数も、呼んだスレッドを描画の終わりまで待たせる。非同期のハンドラーは `spawn_blocking` で呼ぶ。
/// `version` はプロジェクトのバージョン（操作のたびに 1 進む数）。静止画のフレームとプレイリストの作り直しの単位。
pub struct Streamer {
    shared: Arc<Shared>,
}

struct Shared {
    cache: CacheDir,
    prefetch: u64,
    worker: Worker,
    /// 静止画のフレームのキャッシュと、それを作ったプロジェクトのバージョン。
    stills: Mutex<(Option<u64>, StillCache)>,
    listings: Mutex<Listings>,
    inflight: Mutex<HashMap<SegmentKey, Arc<Slot>>>,
    /// キーの計算に使う種類のレジストリと実装の名前。最初のプレイリストで `Engine` からコピーする。
    keying: Mutex<Option<Arc<Keying>>>,
    audio: Mutex<()>,
    /// 作っている短い動画。同じ動画への Range の要求が重なっても 1 回だけ作る。
    clips: Arc<ClipJobs>,
}

struct Keying {
    registry: Registry,
    backends: String,
}

impl Streamer {
    /// 描画のスレッドを始め、前の起動の一時ファイルを捨てる。`Engine` の組み立ては最初の要求まで遅らせる。
    pub fn spawn(config: StreamConfig) -> Result<Self> {
        let cache = CacheDir::new(config.cache_dir);
        cache.remove_partials();
        let shared = Arc::new(Shared {
            cache,
            prefetch: config.prefetch,
            worker: Worker::default(),
            stills: Mutex::default(),
            listings: Mutex::default(),
            inflight: Mutex::default(),
            keying: Mutex::default(),
            audio: Mutex::new(()),
            clips: Arc::default(),
        });
        let thread = shared.clone();
        let (runtime, limit) = (config.runtime, config.cache_limit_bytes);
        std::thread::Builder::new()
            .name("stream-render".into())
            .spawn(move || {
                run(&thread.worker, &runtime, |engine, job, urgent| {
                    let result = build(&thread.cache, engine, job, limit, urgent);
                    let mut inflight = lock(&thread.inflight);
                    if inflight
                        .get(&job.key)
                        .is_some_and(|s| Arc::ptr_eq(s, &job.slot))
                    {
                        inflight.remove(&job.key);
                    }
                    result
                })
            })
            .map_err(MmError::Io)?;
        Ok(Self { shared })
    }

    /// 配信の描画のスレッドの `Engine` で `job` を実行し、結果を待つ。区間を描いている間は、描いているフレームの終わりまで待つ。
    /// `mm-serve` の `GET /frame` もこれで描くと、`Engine` を配信と共有できる。
    pub fn with_engine<R, F>(&self, job: F) -> Result<R>
    where
        R: Send + 'static,
        F: FnOnce(&mut Engine) -> Result<R> + Send + 'static,
    {
        let (reply, rx) = channel();
        self.shared
            .worker
            .urgent(Urgent::Run(Box::new(move |engine| {
                let _ = reply.send(engine.and_then(job));
            })));
        rx.recv()
            .map_err(|_| MmError::Internal("配信の描画のスレッドが結果を返す前に終わった".into()))?
    }

    /// プロジェクトを開き直したときに呼ぶ。バージョンの数え直しで同じ数が別の中身を指すので、
    /// バージョンで分けた静止画のフレームとプレイリストを捨てる。区間はキーが中身で決まるので残す。
    pub fn reset(&self) -> Result<()> {
        *lock(&self.shared.stills) = Default::default();
        lock(&self.shared.listings).clear();
        self.drop_unlisted_prefetch();
        Ok(())
    }

    /// 操作が触った ID に頼る、描画のキャッシュを捨てる（`mm-serve` の `Core` が操作のたびに呼ぶ）。
    pub fn invalidate(&self, touched: &[String]) {
        self.shared
            .worker
            .urgent(Urgent::Invalidate(touched.to_vec()));
    }

    /// 静止画のフレーム。WebP。
    pub fn still(&self, project: &Project, version: u64, req: &StillRequest) -> Result<Payload> {
        let key = StillKey::of(req);
        let cached = {
            let mut stills = lock(&self.shared.stills);
            if stills.0 != Some(version) {
                *stills = (Some(version), StillCache::default());
            }
            stills.1.get(&key)
        };
        let body = match cached {
            Some(b) => b,
            None => {
                let (project, request) = (project.clone(), req.clone());
                let body = Arc::new(self.with_engine(move |e| render_webp(e, &project, &request))?);
                let mut stills = lock(&self.shared.stills);
                if stills.0 == Some(version) {
                    stills.1.put(key.clone(), body.clone());
                }
                body
            }
        };
        Ok(Payload::ok("image/webp", REVALIDATE, body.to_vec())
            .with_header("ETag", key.tag(version)))
    }

    /// HLS のプレイリスト（m3u8）。区間の URI は `segment_base` に区間のファイル名（`<キー>.ts`）を足す。
    /// 先頭の数区間は、プレーヤーが読みに来る前に作り始める。
    pub fn playlist(
        &self,
        project: &Project,
        version: u64,
        req: &HlsRequest,
        segment_base: &str,
    ) -> Result<Payload> {
        let listing = self.listing(project, version, req)?;
        let count = listing.keys.len() as u64;
        self.prefetch(&listing, 0, count.min(self.shared.prefetch));
        let text = listing.plan.playlist(&listing.keys, segment_base);
        Ok(Payload::ok(
            "application/vnd.apple.mpegurl",
            REVALIDATE,
            text.into_bytes(),
        ))
    }

    /// 画質を選ぶ親のプレイリスト（m3u8）。各画質の `BANDWIDTH` は区間のピークのビットレート
    /// （[`HlsQuality::peak_bits_per_second`]）。画質ごとのプレイリストの URI は `variant_uri` が決める。
    pub fn master_playlist(
        &self,
        project: &Project,
        composition: &str,
        variant_uri: &dyn Fn(HlsQuality) -> String,
    ) -> Result<Payload> {
        let variants = HlsQuality::ALL
            .iter()
            .map(|&quality| {
                let req = HlsRequest {
                    composition: composition.into(),
                    quality,
                };
                Ok((Plan::new(project, &req)?, variant_uri(quality)))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Payload::ok(
            "application/vnd.apple.mpegurl",
            REVALIDATE,
            master_playlist(&variants).into_bytes(),
        ))
    }

    /// HLS の区間（MPEG-TS）。無ければ作って待ち、後ろの区間を先読みに回す。
    /// 今のプレイリストに無く、ディスクにも無いキーは `NotFound`（端末はプレイリストを読み直す）。
    pub fn segment(&self, key: SegmentKey) -> Result<Payload> {
        let path = self.shared.cache.segment(key);
        let found = lock(&self.shared.listings).find(key);
        for _ in 0..2 {
            if !path.exists() {
                let (listing, index) = found.clone().ok_or_else(|| {
                    MmError::NotFound(format!("区間 {key} は今のプレイリストに無い"))
                })?;
                self.submit(&listing, index, true).wait()?;
            }
            if let Some((listing, index)) = &found {
                let from = index + 1;
                self.prefetch(listing, from, from + self.shared.prefetch);
            }
            touch(&path);
            match std::fs::read(&path) {
                Ok(body) => return Ok(Payload::ok("video/mp2t", IMMUTABLE, body)),
                // 読む前にキャッシュの上限で捨てられたら、もう一度作る。
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(MmError::Internal(format!("区間 {key} を作っても読めない")))
    }

    /// 素材の音だけ（AAC 64 kbps の m4a）。初めての素材は変換が終わるまで待つ。`range` は `Range` ヘッダーの値。
    pub fn audio(
        &self,
        project: &Project,
        req: &AudioRequest,
        range: Option<&str>,
    ) -> Result<Payload> {
        let hash = project
            .asset(&mm_types::Id::new(req.asset.as_str()))?
            .content_hash
            .clone();
        let path = {
            let _one_at_a_time = lock(&self.shared.audio);
            ensure_audio(project, req, &self.shared.cache)?
        };
        respond(&path, &hash, range)
    }

    /// このバージョンの `req` の区間の割り方とキー。同じバージョンなら前に作ったものを返す。
    fn listing(&self, project: &Project, version: u64, req: &HlsRequest) -> Result<Arc<Listing>> {
        if let Some(l) = lock(&self.shared.listings).get(req, version) {
            return Ok(l);
        }
        let keying = self.keying()?;
        let plan = Plan::new(project, req)?;
        let env = KeyEnv {
            project,
            nodes: &keying.registry,
            backends: &keying.backends,
            version,
        };
        let keys = (0..plan.count())
            .map(|i| segment_key(&env, &plan, i))
            .collect::<Result<Vec<_>>>()?;
        let listing = Arc::new(Listing {
            version,
            project: Arc::new(project.clone()),
            plan: Arc::new(plan),
            keys,
        });
        lock(&self.shared.listings).insert(req, listing.clone());
        self.drop_unlisted_prefetch();
        Ok(listing)
    }

    fn keying(&self) -> Result<Arc<Keying>> {
        if let Some(k) = lock(&self.shared.keying).as_ref() {
            return Ok(k.clone());
        }
        let keying = Arc::new(self.with_engine(|e| {
            let decoders = e.decoder_backend_ids();
            let backends = e
                .backend_names()
                .iter()
                .map(|&(role, name)| backend_label(role, name, &decoders))
                .collect::<Vec<_>>()
                .join(",");
            Ok(Keying {
                registry: e.registry().clone(),
                backends,
            })
        })?);
        *lock(&self.shared.keying) = Some(keying.clone());
        Ok(keying)
    }

    /// どのプレイリストにも無くなった区間の先読みをやめる。
    fn drop_unlisted_prefetch(&self) {
        let listings = lock(&self.shared.listings);
        let dropped = self
            .shared
            .worker
            .retain_prefetch(|k| listings.find(*k).is_some());
        drop(listings);
        let mut inflight = lock(&self.shared.inflight);
        for job in dropped {
            if inflight
                .get(&job.key)
                .is_some_and(|s| Arc::ptr_eq(s, &job.slot))
            {
                inflight.remove(&job.key);
            }
        }
    }

    /// `from..to` の区間を、まだ無ければ後ろに積む。
    fn prefetch(&self, listing: &Arc<Listing>, from: u64, to: u64) {
        for index in from..to.min(listing.keys.len() as u64) {
            if !self
                .shared
                .cache
                .segment(listing.keys[index as usize])
                .exists()
            {
                self.submit(listing, index, false);
            }
        }
    }

    /// 区間を作る仕事を積む。同じ区間が積まれていればそのスロットを返す。要求された区間は先読みより先に作る。
    fn submit(&self, listing: &Arc<Listing>, index: u64, demand: bool) -> Arc<Slot> {
        let key = listing.keys[index as usize];
        let mut inflight = lock(&self.shared.inflight);
        let existing = inflight.get(&key).cloned();
        let slot = existing.clone().unwrap_or_default();
        if existing.is_none() || demand {
            let job = SegJob {
                key,
                project: listing.project.clone(),
                plan: listing.plan.clone(),
                index,
                slot: slot.clone(),
            };
            self.shared.worker.segment(job, demand);
        }
        inflight.insert(key, slot.clone());
        slot
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        self.shared.worker.close();
    }
}

/// 配信の区間のキーに入れる実装の名前。デコーダーには名乗りうる名前の全部を添える（フレームのキャッシュの
/// `unopened_decoder` と同じ形）。どれかの画素の作り方のバージョンが上がると名前が変わり、前の区間を使わない。
pub(crate) fn backend_label(role: &str, name: &str, decoders: &[&str]) -> String {
    if role == "decode" {
        format!("{role}={name}[{}]", decoders.join(","))
    } else {
        format!("{role}={name}")
    }
}

/// 区間を作ってキャッシュに置き、上限を超えた分を捨てる。先に同じキーの区間ができていれば何もしない。
fn build(
    cache: &CacheDir,
    engine: &mut Engine,
    job: &SegJob,
    limit: u64,
    urgent: Interleave<'_>,
) -> Result<()> {
    let path = cache.segment(job.key);
    if path.exists() {
        return Ok(());
    }
    render_segment(engine, &job.project, &job.plan, job.index, &path, urgent)?;
    cache.evict_segments(limit);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::backend_label;

    #[test]
    fn デコーダーには名乗りうる名前を添え_ほかはそのまま() {
        assert_eq!(
            backend_label("decode", "auto", &["d3d12va@2", "ffmpeg@1"]),
            "decode=auto[d3d12va@2,ffmpeg@1]"
        );
        assert_eq!(
            backend_label("decode", "ffmpeg", &["ffmpeg", "ffmpeg@1"]),
            "decode=ffmpeg[ffmpeg,ffmpeg@1]"
        );
        assert_eq!(backend_label("render", "auto", &["x"]), "render=auto");
    }
}
