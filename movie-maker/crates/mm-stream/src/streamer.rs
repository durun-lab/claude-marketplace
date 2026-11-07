//! 配信の入口。描画の 2 本のスレッド（コマと HLS）、版ごとのキャッシュ、先読みを持つ。

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use mm_doc::Project;
use mm_runtime::{assemble, Engine, RuntimeConfig};
use mm_types::{MmError, Result};

use crate::audio::{ensure_audio, respond, AudioRequest};
use crate::cache::{sanitize, CacheDir};
use crate::frames::{render_webp, StillCache, StillKey, StillRequest};
use crate::hls::{HlsRequest, Plan, SegmentName};
use crate::payload::{Payload, IMMUTABLE, REVALIDATE};
use crate::quality::HlsQuality;
use crate::segment::render_segment;

/// 先読みする区間の数の既定。
const DEFAULT_PREFETCH: u64 = 2;
/// 区間 1 つを待つ上限。
const SEGMENT_TIMEOUT: Duration = Duration::from_secs(300);

/// 配信の設定。
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// 描画とエンコードの実装の選択。
    pub runtime: RuntimeConfig,
    /// キャッシュの置き場所（プロジェクトの `cache/mobile/`）。起動と [`Streamer::reset`] で区間を捨てる。
    pub cache_dir: PathBuf,
    /// 要求された区間の後ろに先に作る区間の数。
    pub prefetch: u64,
}

impl StreamConfig {
    /// 先読み 2 区間の設定。
    pub fn new(runtime: RuntimeConfig, cache_dir: PathBuf) -> Self {
        Self {
            runtime,
            cache_dir,
            prefetch: DEFAULT_PREFETCH,
        }
    }
}

/// モバイルへの映像の配信。`mm-serve` のハンドラーが共有して呼ぶ。
///
/// どの関数も、呼んだスレッドを描画の終わりまで待たせる。非同期のハンドラーは `spawn_blocking` で呼ぶ。
/// `version` はプロジェクトの版（変更のたびに増える数）。変わると、コマと区間のキャッシュを捨てて作り直す。
pub struct Streamer {
    shared: Arc<Shared>,
    stills: Mutex<Sender<StillMsg>>,
}

struct Shared {
    cache: CacheDir,
    prefetch: u64,
    current: Mutex<Current>,
    queue: Mutex<Queue>,
    wake: Condvar,
    inflight: Mutex<HashMap<SegKey, Arc<Slot>>>,
    stills: Mutex<StillCache>,
    audio: Mutex<()>,
}

#[derive(Default)]
struct Current {
    version: Option<u64>,
    project: Option<Arc<Project>>,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

enum Job {
    Segment(Box<SegJob>),
    Invalidate(Vec<String>),
}

struct SegJob {
    key: SegKey,
    project: Arc<Project>,
    plan: Arc<Plan>,
    slot: Arc<Slot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SegKey {
    sequence: String,
    quality: HlsQuality,
    version: u64,
    index: u64,
}

enum Outcome {
    Done,
    /// 作る前にプロジェクトの版が変わった。
    Stale,
    Failed(String),
}

#[derive(Default)]
struct Slot {
    outcome: Mutex<Option<Outcome>>,
    done: Condvar,
}

enum StillMsg {
    Render(StillJob),
    Invalidate(Vec<String>),
}

struct StillJob {
    project: Arc<Project>,
    request: StillRequest,
    reply: Sender<Result<Vec<u8>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Streamer {
    /// 描画のスレッドを 2 本始め、前の起動のキャッシュを捨てる。`Engine` の組み立ては最初の要求まで遅らせる。
    /// 版の数え方は起動ごとに変わるので、残った区間は使えない。
    pub fn spawn(config: StreamConfig) -> Result<Self> {
        let shared = Arc::new(Shared {
            cache: CacheDir::new(config.cache_dir),
            prefetch: config.prefetch,
            current: Mutex::default(),
            queue: Mutex::default(),
            wake: Condvar::new(),
            inflight: Mutex::default(),
            stills: Mutex::default(),
            audio: Mutex::new(()),
        });
        shared.cache.clear_hls()?;
        let (tx, rx) = channel::<StillMsg>();
        let runtime = config.runtime.clone();
        std::thread::Builder::new()
            .name("stream-stills".into())
            .spawn(move || still_loop(rx, &runtime))
            .map_err(MmError::Io)?;
        let hls = shared.clone();
        let runtime = config.runtime;
        std::thread::Builder::new()
            .name("stream-hls".into())
            .spawn(move || hls_loop(&hls, &runtime))
            .map_err(MmError::Io)?;
        Ok(Self {
            shared,
            stills: Mutex::new(tx),
        })
    }

    /// プロジェクトを開き直したときに呼ぶ。版の数え直しで同じ数が別の中身を指すので、キャッシュを全部捨てる。
    pub fn reset(&self) -> Result<()> {
        *lock(&self.shared.current) = Current::default();
        lock(&self.shared.stills).clear();
        lock(&self.shared.queue)
            .jobs
            .retain(|j| matches!(j, Job::Invalidate(_)));
        self.shared.cache.clear_hls()
    }

    /// 操作が触った ID に頼る、描画のキャッシュを捨てる（`mm-serve` の `Core` が操作のたびに呼ぶ）。
    pub fn invalidate(&self, touched: &[String]) {
        lock(&self.shared.queue)
            .jobs
            .push_front(Job::Invalidate(touched.to_vec()));
        self.shared.wake.notify_all();
        let _ = lock(&self.stills).send(StillMsg::Invalidate(touched.to_vec()));
    }

    /// 静止画のコマ。WebP。
    pub fn still(&self, project: &Project, version: u64, req: &StillRequest) -> Result<Payload> {
        let project = self.observe(project, version);
        let key = StillKey::of(req);
        let cached = lock(&self.shared.stills).get(&key);
        let body = match cached {
            Some(b) => b,
            None => {
                let (reply, rx) = channel();
                let job = StillJob {
                    project,
                    request: req.clone(),
                    reply,
                };
                lock(&self.stills)
                    .send(StillMsg::Render(job))
                    .map_err(|_| MmError::Internal("コマの描画のスレッドが終わっている".into()))?;
                let body = Arc::new(rx.recv().map_err(|_| {
                    MmError::Internal("コマの描画のスレッドが結果を返す前に終わった".into())
                })??);
                lock(&self.shared.stills).put(key.clone(), body.clone());
                body
            }
        };
        Ok(Payload::ok("image/webp", REVALIDATE, body.to_vec())
            .with_header("ETag", key.tag(version)))
    }

    /// HLS のプレイリスト（m3u8）。区間の URI は `segment_base` に [`SegmentName`] の文字列を足す。
    /// 先頭の数区間は、プレーヤーが読みに来る前に作り始める。
    pub fn playlist(
        &self,
        project: &Project,
        version: u64,
        req: &HlsRequest,
        segment_base: &str,
    ) -> Result<Payload> {
        let project = self.observe(project, version);
        let plan = Arc::new(Plan::new(&project, req)?);
        self.prefetch(
            &project,
            &plan,
            version,
            0,
            plan.count().min(self.shared.prefetch),
        );
        let text = plan.playlist(version, segment_base);
        Ok(Payload::ok(
            "application/vnd.apple.mpegurl",
            REVALIDATE,
            text.into_bytes(),
        ))
    }

    /// HLS の区間（MPEG-TS）。無ければ作って待ち、後ろの区間を先読みに回す。
    /// `name.version` が `version` と違う（編集の前のプレイリストの URL）ときは `NotFound`。
    pub fn segment(
        &self,
        project: &Project,
        version: u64,
        req: &HlsRequest,
        name: SegmentName,
    ) -> Result<Payload> {
        if name.version != version {
            return Err(MmError::NotFound(format!(
                "区間 {name} はプロジェクトの版 {version} に無い"
            )));
        }
        let project = self.observe(project, version);
        let plan = Arc::new(Plan::new(&project, req)?);
        if name.index >= plan.count() {
            return Err(MmError::NotFound(format!("区間 {name} は列の外")));
        }
        let path = self.segment_path(&plan, version, name.index);
        if !path.exists() {
            let key = seg_key(&plan, version, name.index);
            let slot = self.submit(key, &project, &plan, true);
            wait(&slot)?;
        }
        let from = name.index + 1;
        self.prefetch(
            &project,
            &plan,
            version,
            from,
            plan.count().min(from + self.shared.prefetch),
        );
        let body = std::fs::read(&path)?;
        Ok(Payload::ok("video/mp2t", IMMUTABLE, body))
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

    fn segment_path(&self, plan: &Plan, version: u64, index: u64) -> PathBuf {
        self.shared
            .cache
            .segment(&plan.sequence, plan.quality.as_str(), version, index)
    }

    /// 版が変わっていたら、古いキャッシュと積んだ仕事を捨てて、この版のプロジェクトを覚える。
    fn observe(&self, project: &Project, version: u64) -> Arc<Project> {
        let mut cur = lock(&self.shared.current);
        if cur.version == Some(version) {
            if let Some(p) = &cur.project {
                return p.clone();
            }
        }
        let arc = Arc::new(project.clone());
        cur.version = Some(version);
        cur.project = Some(arc.clone());
        drop(cur);
        lock(&self.shared.stills).clear();
        lock(&self.shared.queue).jobs.retain(|j| match j {
            Job::Segment(s) => s.key.version == version,
            Job::Invalidate(_) => true,
        });
        self.shared.cache.prune_hls(version);
        arc
    }

    /// `from..to` の区間を、まだ無ければ後ろに積む。
    fn prefetch(&self, project: &Arc<Project>, plan: &Arc<Plan>, version: u64, from: u64, to: u64) {
        for index in from..to {
            if !self.segment_path(plan, version, index).exists() {
                self.submit(seg_key(plan, version, index), project, plan, false);
            }
        }
    }

    /// 区間を作る仕事を積む。同じ区間が積まれていればそれを返す。急ぎの要求は先頭に積み直す。
    fn submit(
        &self,
        key: SegKey,
        project: &Arc<Project>,
        plan: &Arc<Plan>,
        demand: bool,
    ) -> Arc<Slot> {
        let mut inflight = lock(&self.shared.inflight);
        let existing = inflight.get(&key).cloned();
        let slot = existing.clone().unwrap_or_default();
        if existing.is_none() || demand {
            let job = Job::Segment(Box::new(SegJob {
                key: key.clone(),
                project: project.clone(),
                plan: plan.clone(),
                slot: slot.clone(),
            }));
            let mut queue = lock(&self.shared.queue);
            if demand {
                queue.jobs.push_front(job);
            } else {
                queue.jobs.push_back(job);
            }
            self.shared.wake.notify_all();
        }
        inflight.insert(key, slot.clone());
        slot
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        lock(&self.shared.queue).closed = true;
        self.shared.wake.notify_all();
    }
}

fn seg_key(plan: &Plan, version: u64, index: u64) -> SegKey {
    SegKey {
        sequence: plan.sequence.clone(),
        quality: plan.quality,
        version,
        index,
    }
}

fn wait(slot: &Slot) -> Result<()> {
    let guard = lock(&slot.outcome);
    let (guard, timeout) = slot
        .done
        .wait_timeout_while(guard, SEGMENT_TIMEOUT, |o| o.is_none())
        .unwrap_or_else(|e| e.into_inner());
    match guard.as_ref() {
        Some(Outcome::Done) => Ok(()),
        Some(Outcome::Stale) => Err(MmError::NotFound(
            "プロジェクトが変わって区間を作り直す".into(),
        )),
        Some(Outcome::Failed(msg)) => Err(MmError::Internal(msg.clone())),
        None => {
            debug_assert!(timeout.timed_out());
            Err(MmError::Timeout {
                what: "区間を作る".into(),
                limit: SEGMENT_TIMEOUT,
            })
        }
    }
}

fn ensure<'a>(engine: &'a mut Option<Engine>, config: &RuntimeConfig) -> Result<&'a mut Engine> {
    if engine.is_none() {
        *engine = Some(assemble(config)?);
    }
    engine
        .as_mut()
        .ok_or_else(|| MmError::Internal("エンジンが組み立たない".into()))
}

fn still_loop(rx: std::sync::mpsc::Receiver<StillMsg>, config: &RuntimeConfig) {
    let mut engine: Option<Engine> = None;
    while let Ok(msg) = rx.recv() {
        match msg {
            StillMsg::Invalidate(ids) => {
                if let Some(e) = engine.as_mut() {
                    e.invalidate_touched(&ids);
                }
            }
            StillMsg::Render(job) => {
                let result = ensure(&mut engine, config)
                    .and_then(|e| render_webp(e, &job.project, &job.request));
                let _ = job.reply.send(result);
            }
        }
    }
}

fn hls_loop(shared: &Shared, config: &RuntimeConfig) {
    let mut engine: Option<Engine> = None;
    while let Some(job) = next_job(shared) {
        match job {
            Job::Invalidate(ids) => {
                if let Some(e) = engine.as_mut() {
                    e.invalidate_touched(&ids);
                }
            }
            Job::Segment(job) => {
                let outcome = build(shared, &mut engine, config, &job);
                finish(shared, &job, outcome);
            }
        }
    }
}

fn next_job(shared: &Shared) -> Option<Job> {
    let mut q = lock(&shared.queue);
    loop {
        if q.closed {
            return None;
        }
        if let Some(job) = q.jobs.pop_front() {
            return Some(job);
        }
        q = shared.wake.wait(q).unwrap_or_else(|e| e.into_inner());
    }
}

fn build(
    shared: &Shared,
    engine: &mut Option<Engine>,
    config: &RuntimeConfig,
    job: &SegJob,
) -> Outcome {
    if lock(&shared.current).version != Some(job.key.version) {
        return Outcome::Stale;
    }
    let path = shared.cache.segment(
        &job.key.sequence,
        job.key.quality.as_str(),
        job.key.version,
        job.key.index,
    );
    if path.exists() {
        return Outcome::Done;
    }
    let result = ensure(engine, config)
        .and_then(|e| render_segment(e, &job.project, &job.plan, job.key.index, &path));
    match result {
        Ok(()) => Outcome::Done,
        Err(e) => Outcome::Failed(format!(
            "区間 {} を作れない: {e}",
            sanitize(&job.key.sequence)
        )),
    }
}

fn finish(shared: &Shared, job: &SegJob, outcome: Outcome) {
    {
        let mut inflight = lock(&shared.inflight);
        if inflight
            .get(&job.key)
            .is_some_and(|s| Arc::ptr_eq(s, &job.slot))
        {
            inflight.remove(&job.key);
        }
    }
    let mut slot = lock(&job.slot.outcome);
    if slot.is_none() {
        *slot = Some(outcome);
    }
    job.slot.done.notify_all();
}
