//! 描画のスレッド。`Engine` を 1 つだけ持ち、静止画のコマ、`GET /frame`、区間を順に描く。
//!
//! `Engine` は GPU のコンテキストとフレームのキャッシュを持ち、VRAM を食うので、配信の全部で 1 つにする。
//! 待たせると困る順に取り出す: コマとキャッシュの破棄、要求された区間、先読みの区間。

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use mm_doc::Project;
use mm_runtime::{assemble, Engine, RuntimeConfig};
use mm_types::{MmError, Result};

use crate::hls::Plan;
use crate::key::SegmentKey;

/// 区間 1 つを待つ上限。
const SEGMENT_TIMEOUT: Duration = Duration::from_secs(300);

/// 描画のスレッドで `Engine` を借りて走らせる仕事。組み立てに失敗したら `Err` を渡す。
pub(crate) type EngineJob = Box<dyn FnOnce(Result<&mut Engine>) + Send>;

/// 区間を 1 つ作る仕事。
pub(crate) struct SegJob {
    pub key: SegmentKey,
    pub project: Arc<Project>,
    pub plan: Arc<Plan>,
    pub index: u64,
    pub slot: Arc<Slot>,
}

/// 区間の出来上がりを待つ所。同じキーの要求は 1 つのスロットを共有する。
#[derive(Default)]
pub(crate) struct Slot {
    outcome: Mutex<Option<std::result::Result<(), String>>>,
    done: Condvar,
}

impl Slot {
    pub(crate) fn finish(&self, outcome: std::result::Result<(), String>) {
        let mut slot = lock(&self.outcome);
        if slot.is_none() {
            *slot = Some(outcome);
        }
        self.done.notify_all();
    }

    pub(crate) fn wait(&self) -> Result<()> {
        let guard = lock(&self.outcome);
        let (guard, _) = self
            .done
            .wait_timeout_while(guard, SEGMENT_TIMEOUT, |o| o.is_none())
            .unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some(Ok(())) => Ok(()),
            Some(Err(msg)) => Err(MmError::Internal(msg.clone())),
            None => Err(MmError::Timeout {
                what: "区間を作る".into(),
                limit: SEGMENT_TIMEOUT,
            }),
        }
    }
}

pub(crate) enum Urgent {
    Run(EngineJob),
    Invalidate(Vec<String>),
}

/// 取り出した仕事。
pub(crate) enum Job {
    Urgent(Urgent),
    Segment(SegJob),
}

#[derive(Default)]
struct Queue {
    urgent: VecDeque<Urgent>,
    demand: VecDeque<SegJob>,
    prefetch: VecDeque<SegJob>,
    closed: bool,
}

/// 描画のスレッドへの仕事のキュー。
#[derive(Default)]
pub(crate) struct Worker {
    queue: Mutex<Queue>,
    wake: Condvar,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Worker {
    pub(crate) fn urgent(&self, job: Urgent) {
        lock(&self.queue).urgent.push_back(job);
        self.wake.notify_all();
    }

    /// 区間を積む。要求された区間は先読みより先に作る。
    pub(crate) fn segment(&self, job: SegJob, demand: bool) {
        let mut q = lock(&self.queue);
        if demand {
            q.demand.push_back(job);
        } else {
            q.prefetch.push_back(job);
        }
        drop(q);
        self.wake.notify_all();
    }

    /// 先読みの区間のうち `keep` が偽のものをキューから外して返す（呼び出し側が作りかけの表から消すため）。
    pub(crate) fn retain_prefetch(&self, keep: impl Fn(&SegmentKey) -> bool) -> Vec<SegJob> {
        let mut q = lock(&self.queue);
        let (kept, dropped): (Vec<_>, Vec<_>) = q.prefetch.drain(..).partition(|j| keep(&j.key));
        q.prefetch = kept.into();
        dropped
    }

    pub(crate) fn close(&self) {
        lock(&self.queue).closed = true;
        self.wake.notify_all();
    }

    fn next(&self) -> Option<Job> {
        let mut q = lock(&self.queue);
        loop {
            if q.closed {
                return None;
            }
            if let Some(job) = q.urgent.pop_front() {
                return Some(Job::Urgent(job));
            }
            if let Some(job) = q.demand.pop_front().or_else(|| q.prefetch.pop_front()) {
                return Some(Job::Segment(job));
            }
            q = self.wake.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// 描画のスレッドの本体。`Engine` の組み立ては最初の仕事まで遅らせる。`segment` は区間を作って置く。
pub(crate) fn run(
    worker: &Worker,
    config: &RuntimeConfig,
    segment: impl Fn(&mut Engine, &SegJob) -> Result<()>,
) {
    let mut engine: Option<Engine> = None;
    while let Some(job) = worker.next() {
        match job {
            Job::Urgent(Urgent::Invalidate(ids)) => {
                if let Some(e) = engine.as_mut() {
                    e.invalidate_touched(&ids);
                }
            }
            Job::Urgent(Urgent::Run(job)) => job(ensure(&mut engine, config)),
            Job::Segment(job) => {
                let result = ensure(&mut engine, config).and_then(|e| segment(e, &job));
                job.slot
                    .finish(result.map_err(|e| format!("区間 {} を作れない: {e}", job.key)));
            }
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
