//! [`Improver`]: 要望の起票、一覧、状態、設定、すぐ止める、回を回す、の公開 API。
//! アプリ（Feedback パネルの IPC）と `mm improve` が同じ `data_dir` を指して使う。
//!
//! 起動の契機（要望を保存したとき、設定を変えたとき、アプリを起動したとき）に [`Improver::kick`] を呼ぶと、
//! バックグラウンドの実行が動いていなければスレッドを 1 本起こし、回を起こす条件が続く間だけ回す。使用制限で止まったら、
//! 明ける時刻まで眠ってから続ける（時刻を知っているので、確かめ直しの繰り返しはしない）。

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use mm_types::Result;
use serde::{Deserialize, Serialize};

use crate::agent::Agent;
use crate::checks::Checks;
use crate::config::{ImproveConfig, Settings, SettingsPatch};
use crate::errors::{ErrorInput, RecentLanding};
use crate::files::write_json;
use crate::forge::Forge;
use crate::proc::kill_tree;
use crate::round::{self, Ctx, Step, AFTER_RESET_MS, LOCK_FILE, SETTINGS_FILE, STOP_MARK};

/// ロックが取れなかったときに取り直す回数と間隔。`status` がロックを確かめる一瞬と重なっただけなら、次で取れる。
const BUSY_RETRIES: u32 = 3;
const BUSY_WAIT_MS: u64 = 200;
use crate::state::{Phase, RunLock, RunOutcome, RunReport, RunState};
use crate::store::{FeedbackItem, FeedbackKind, FeedbackStatus, FeedbackStore, NewFeedback};
use crate::time::now_ms;

/// 動いている回の見え方。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    /// 回の ID。
    pub id: String,
    /// 渡した要望。
    pub items: Vec<String>,
    /// 始めた時刻（ミリ秒）。
    pub started_at: u64,
    /// 工程。
    pub phase: Phase,
    /// 使用制限が明ける時刻（ミリ秒）。
    pub limit_resets_at: Option<u64>,
}

impl From<&RunState> for RunView {
    fn from(r: &RunState) -> Self {
        Self {
            id: r.id.clone(),
            items: r.items.clone(),
            started_at: r.started_at,
            phase: r.phase,
            limit_resets_at: r.limit_resets_at,
        }
    }
}

/// 状態。Feedback パネルの見出しと `mm improve status` が出す。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImproveStatus {
    /// 設定。
    pub settings: Settings,
    /// いまプロセスのどれかが回を動かしているか。
    pub running: bool,
    /// 回（使用制限で止まっているものも含む）。
    pub run: Option<RunView>,
    /// 回に渡していない未対応の要望の数。
    pub pending: usize,
    /// 待機の理由。
    pub waiting: Option<String>,
    /// 最後に終えた回。
    pub last: Option<RunReport>,
}

/// 一覧の絞り込み。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListFilter {
    /// 全部。
    #[default]
    All,
    /// 未対応だけ。
    Pending,
    /// 済み（取り込んだか PR までで止めたもの）だけ。
    Done,
}

#[derive(Default)]
struct Background {
    enabled: bool,
    running: bool,
}

struct Inner {
    ctx: Ctx,
    bg: Mutex<Background>,
    cv: Condvar,
}

/// 自動の改善。複製は同じものを指す。
#[derive(Clone)]
pub struct Improver {
    inner: Arc<Inner>,
}

impl Improver {
    /// 部品を決めて作る。バックグラウンドの実行は [`Improver::with_background`] で有効にする。
    pub fn new(
        cfg: ImproveConfig,
        agent: Arc<dyn Agent>,
        checks: Arc<dyn Checks>,
        forge: Arc<dyn Forge>,
    ) -> Self {
        let store = FeedbackStore::new(cfg.feedback_dir());
        let ctx = Ctx {
            cfg,
            agent,
            checks,
            forge,
            store,
            errors_lock: Mutex::new(()),
        };
        let inner = Inner {
            ctx,
            bg: Mutex::new(Background::default()),
            cv: Condvar::new(),
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    /// 起動の契機にバックグラウンドで回を回す（アプリ）。CLI は `run_once` を前で回す。
    pub fn with_background(self) -> Self {
        self.bg().enabled = true;
        self
    }

    fn ctx(&self) -> &Ctx {
        &self.inner.ctx
    }

    fn bg(&self) -> MutexGuard<'_, Background> {
        self.inner.bg.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 要望を起票する（Feedback パネル、CLI）。待機中なら、新しい要望を機に待機を解く。
    pub fn submit(&self, new: NewFeedback) -> Result<FeedbackItem> {
        let item = self.ctx().store.add(FeedbackKind::Feedback, new)?;
        self.clear_waiting()?;
        self.kick();
        Ok(item)
    }

    /// エラーか遅い処理を記録し、条件を満たせば自動で起票する。起票した要望の ID を返す。
    /// 記録の読み書きはプロセスの中で順に行うので、panic のフックとバックグラウンドのスレッドが同時に呼んでも同じものを 2 件にしない。
    pub fn record(&self, input: &ErrorInput) -> Result<Option<String>> {
        let ctx = self.ctx();
        let st = ctx.state()?;
        let landing = st.last_landed.as_ref().map(|(id, at)| RecentLanding {
            run_id: id,
            at: *at,
        });
        let filed = ctx
            .with_errors(|book| book.record(input, &ctx.store, now_ms(), landing))?
            .flatten();
        if filed.is_some() {
            self.clear_waiting()?;
            self.kick();
        }
        Ok(filed)
    }

    fn clear_waiting(&self) -> Result<()> {
        let ctx = self.ctx();
        if !ctx.settings()?.enabled {
            return Ok(());
        }
        let mut st = ctx.state()?;
        if st.waiting.take().is_some() {
            ctx.save_state(&st)?;
        }
        Ok(())
    }

    /// 要望の一覧（新しい順）。
    pub fn list(&self, filter: ListFilter) -> Result<Vec<FeedbackItem>> {
        let all = self.ctx().store.all()?;
        Ok(all
            .into_iter()
            .filter(|i| match filter {
                ListFilter::All => true,
                ListFilter::Pending => i.status == FeedbackStatus::Pending,
                ListFilter::Done => i.status != FeedbackStatus::Pending,
            })
            .collect())
    }

    /// 要望を ID で引く（返事を含む）。
    pub fn get(&self, id: &str) -> Result<FeedbackItem> {
        self.ctx().store.get(id)
    }

    /// 状態。
    pub fn status(&self) -> Result<ImproveStatus> {
        let ctx = self.ctx();
        let st = ctx.state()?;
        let in_run: Vec<&String> = st.run.iter().flat_map(|r| &r.items).collect();
        let pending = ctx
            .store
            .pending()?
            .iter()
            .filter(|i| !in_run.contains(&&i.id))
            .count();
        let running = RunLock::try_acquire(&ctx.cfg.file(LOCK_FILE))?.is_none();
        Ok(ImproveStatus {
            settings: ctx.settings()?,
            running,
            run: st.run.as_ref().map(RunView::from),
            pending,
            waiting: st.waiting,
            last: st.last,
        })
    }

    /// 設定を変える。ON にし直すと待機を解く。
    pub fn update_settings(&self, patch: &SettingsPatch) -> Result<ImproveStatus> {
        let ctx = self.ctx();
        let mut s = ctx.settings()?;
        s.apply(patch);
        write_json(&ctx.cfg.file(SETTINGS_FILE), &s)?;
        if patch.enabled == Some(true) {
            let mut st = ctx.state()?;
            st.waiting = None;
            ctx.save_state(&st)?;
        }
        self.kick();
        self.status()
    }

    /// すぐ止める。OFF にし、動いている `claude -p` を木ごと止める。回は worktree を消して終わり、要望は未対応に残る。
    pub fn stop(&self) -> Result<ImproveStatus> {
        let ctx = self.ctx();
        let mut s = ctx.settings()?;
        s.enabled = false;
        write_json(&ctx.cfg.file(SETTINGS_FILE), &s)?;
        std::fs::create_dir_all(&ctx.cfg.data_dir)?;
        std::fs::write(ctx.cfg.file(STOP_MARK), now_ms().to_string())?;
        let st = ctx.state()?;
        if let Some(pid) = st.run.as_ref().and_then(|r| r.agent_pid) {
            kill_tree(pid);
        }
        // 使用制限で眠っている回は誰もロックを持っていないので、ここで終える。
        if let Some(_lock) = RunLock::try_acquire(&ctx.cfg.file(LOCK_FILE))? {
            let mut st = ctx.state()?;
            match st.run.clone() {
                Some(run) => drop(round::fail(ctx, &mut st, &run, RunOutcome::Stopped)?),
                None => ctx.clear_stop()?,
            }
        }
        self.inner.cv.notify_all();
        self.status()
    }

    /// 1 歩をこの場で回す（CLI の `mm improve run`、テスト）。回を起こしたら終えるまで返らない。
    pub fn run_once(&self) -> Result<Step> {
        let ctx = self.ctx();
        let Some(_lock) = RunLock::try_acquire(&ctx.cfg.file(LOCK_FILE))? else {
            return Ok(Step::Busy);
        };
        round::step(ctx)
    }

    /// 起動の契機。バックグラウンドの実行が有効で、動いていなければスレッドを起こす。
    pub fn kick(&self) {
        {
            let mut bg = self.bg();
            if !bg.enabled || bg.running {
                return;
            }
            bg.running = true;
        }
        let me = self.clone();
        std::thread::spawn(move || me.background_loop());
    }

    fn background_loop(&self) {
        let mut busy = 0;
        loop {
            match self.run_once() {
                Ok(Step::Finished(_)) => continue,
                Ok(Step::Limited { resets_at }) if self.sleep_until(resets_at + AFTER_RESET_MS) => {
                    continue
                }
                // `status` もロックを一瞬取って確かめるので、重なったら少し待って取り直す。
                // 取り直しても取れなければ、ほかのプロセス（`mm improve run`）が回を動かしている。
                Ok(Step::Busy)
                    if busy < BUSY_RETRIES && self.sleep_until(now_ms() + BUSY_WAIT_MS) =>
                {
                    busy += 1;
                }
                _ => break,
            }
        }
        self.bg().running = false;
        self.inner.cv.notify_all();
    }

    /// `at` まで眠る。止められたら `false`。
    fn sleep_until(&self, at: u64) -> bool {
        let mut bg = self.bg();
        loop {
            let now = now_ms();
            if now >= at {
                return true;
            }
            let (g, _) = self
                .inner
                .cv
                .wait_timeout(bg, Duration::from_millis(at - now))
                .unwrap_or_else(|e| e.into_inner());
            bg = g;
            if self.ctx().stop_requested() || !self.ctx().settings().is_ok_and(|s| s.enabled) {
                return false;
            }
        }
    }

    /// バックグラウンドの実行が止まるまで待つ（テストとアプリの終了）。
    pub fn wait_idle(&self) {
        let mut bg = self.bg();
        while bg.running {
            bg = self.inner.cv.wait(bg).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// 置き場所。
    pub fn config(&self) -> &ImproveConfig {
        &self.ctx().cfg
    }
}

impl std::fmt::Debug for Improver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Improver")
            .field("data_dir", &self.ctx().cfg.data_dir)
            .finish()
    }
}
