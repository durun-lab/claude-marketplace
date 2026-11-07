//! 回の起動と区切り。別の worktree を作って基準を取り、`claude -p` にたまった要望をまとめて実装させ、
//! Stop で区切ったら囲いの検査（gate.rs）へ渡す。使用制限で止まったら状態に明ける時刻を残し、明けたら同じセッションで続ける。

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mm_types::Result;

use crate::agent::{stop_message, Agent, AgentEnd, AgentRequest};
use crate::areas::Areas;
use crate::checks::{CheckCx, Checks};
use crate::config::{ImproveConfig, Settings, DEFAULT_BUDGET_USD};
use crate::errors::ErrorBook;
use crate::files::{read_or_default, write_json};
use crate::forge::Forge;
use crate::git;
use crate::prompt::{prompt_for, PromptContext, CONTINUE_PROMPT, DONE_MARK, NUDGE_PROMPT};
use crate::state::{ImproveState, Phase, RunOutcome, RunReport, RunState};
use crate::store::FeedbackStore;
use crate::time::{now_ms, short_hex, stamp, uuid_v4};

/// 印を書かずに止まったときに続きを頼む上限。
const MAX_NUDGES: u32 = 2;
/// 1 つの回で `claude -p` を呼ぶ上限。会話ログに古い使用制限が残っているなど、すぐ終わる呼び出しを繰り返さない。
pub const MAX_CALLS: u32 = 12;
const MS_PER_MIN: u64 = 60_000;
/// 明ける時刻ちょうどに頼むとまだ断られることがあるので、少し待ってから続ける。
pub const AFTER_RESET_MS: u64 = 60_000;
pub(crate) const STATE_FILE: &str = "state.json";
pub(crate) const SETTINGS_FILE: &str = "settings.json";
pub(crate) const ERRORS_FILE: &str = "errors.json";
pub(crate) const STOP_MARK: &str = "stop";
pub(crate) const LOCK_FILE: &str = "run.lock";
pub(crate) const BASELINE_FILE: &str = "baseline.json";
pub(crate) const REPORT_FILE: &str = "report.json";

/// 1 歩の結果。
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// 回を起こさなかった。理由（OFF、待機、未対応なし）。
    Idle(String),
    /// ほかのプロセスかスレッドが回を動かしている。
    Busy,
    /// 使用制限で止まっている。明ける時刻（ミリ秒）。
    Limited {
        /// 明ける時刻。
        resets_at: u64,
    },
    /// 回を終えた。
    Finished(RunReport),
}

/// 回が使う部品と置き場所。
pub(crate) struct Ctx {
    pub cfg: ImproveConfig,
    pub agent: Arc<dyn Agent>,
    pub checks: Arc<dyn Checks>,
    pub forge: Arc<dyn Forge>,
    pub store: FeedbackStore,
    /// `errors.json` の読み書きを順にする。
    pub errors_lock: Mutex<()>,
}

thread_local! {
    /// このスレッドが `errors.json` の錠を持っているか。持っている間の panic を panic のフックが記録しようとして、
    /// 同じ錠を待って止まらないようにする。
    static HOLDING_ERRORS: Cell<bool> = const { Cell::new(false) };
}

/// 抜けるとき（panic で巻き戻るときも）に印を戻す。
struct ReleaseErrors;

impl Drop for ReleaseErrors {
    fn drop(&mut self) {
        HOLDING_ERRORS.set(false);
    }
}

impl Ctx {
    /// `errors.json` を錠の中で読み、`f` で変えて書く。このスレッドが既に持っていれば何もせず `None`。
    pub fn with_errors<R>(&self, f: impl FnOnce(&mut ErrorBook) -> Result<R>) -> Result<Option<R>> {
        if HOLDING_ERRORS.replace(true) {
            return Ok(None);
        }
        let _release = ReleaseErrors;
        let _lock = self.errors_lock.lock().unwrap_or_else(|e| e.into_inner());
        let path = self.cfg.file(ERRORS_FILE);
        let mut book = ErrorBook::load(&path)?;
        let out = f(&mut book)?;
        book.save(&path)?;
        Ok(Some(out))
    }

    pub fn state(&self) -> Result<ImproveState> {
        read_or_default(&self.cfg.file(STATE_FILE))
    }

    pub fn save_state(&self, st: &ImproveState) -> Result<()> {
        write_json(&self.cfg.file(STATE_FILE), st)
    }

    pub fn settings(&self) -> Result<Settings> {
        read_or_default(&self.cfg.file(SETTINGS_FILE))
    }

    pub fn stop_requested(&self) -> bool {
        self.cfg.file(STOP_MARK).exists()
    }

    pub fn clear_stop(&self) -> Result<()> {
        let p = self.cfg.file(STOP_MARK);
        if p.exists() {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }

    /// 回の worktree の movie-maker。
    pub fn root_of(&self, run: &RunState) -> PathBuf {
        run.worktree.join(&self.cfg.subdir)
    }

    /// 本線の areas.toml。回が書き換えても、取り込む先の版で判定する。無ければ全部を人の承認に回す。
    pub fn areas(&self) -> Result<Areas> {
        let path = self.cfg.subdir.join("docs").join("areas.toml");
        let path = path.to_string_lossy().replace('\\', "/");
        match git::show(&self.cfg.repo, &self.cfg.mainline, &path) {
            Ok(text) => Areas::parse(&text),
            Err(_) => Areas::parse(""),
        }
    }
}

/// 1 歩進める。錠は呼ぶ側が持つ。
pub(crate) fn step(ctx: &Ctx) -> Result<Step> {
    let mut st = ctx.state()?;
    let run = match st.run.clone() {
        Some(run) => run,
        None => match start(ctx, &mut st)? {
            Ok(run) => run,
            Err(step) => return Ok(step),
        },
    };
    if let Some(t) = run.limit_resets_at {
        if now_ms() < t + AFTER_RESET_MS {
            return Ok(Step::Limited { resets_at: t });
        }
    }
    drive(ctx, st, run)
}

/// 新しい回を起こす。起こさないときは理由の `Step` を返す。
fn start(ctx: &Ctx, st: &mut ImproveState) -> Result<std::result::Result<RunState, Step>> {
    if !ctx.settings()?.enabled {
        return Ok(Err(Step::Idle("OFF".into())));
    }
    if let Some(w) = &st.waiting {
        return Ok(Err(Step::Idle(format!("待機中: {w}"))));
    }
    let pending = ctx.store.pending()?;
    if pending.is_empty() {
        return Ok(Err(Step::Idle("未対応の要望がない".into())));
    }
    ctx.clear_stop()?;
    let now = now_ms();
    let id = format!("{}-{}", stamp(now), short_hex());
    let branch = format!("fix/movie-maker-improve-{id}");
    let worktree = ctx.cfg.worktrees_dir.join(&id);
    git::ensure_branch(&ctx.cfg.repo, &ctx.cfg.mainline, &ctx.cfg.upstream)?;
    let base_commit = git::git(&ctx.cfg.repo, &["rev-parse", &ctx.cfg.mainline])?;
    git::worktree_add(&ctx.cfg.repo, &worktree, &branch, &base_commit)?;
    let settings = ctx.settings()?;
    let run = RunState {
        id,
        session_id: uuid_v4(),
        branch,
        worktree,
        base_commit,
        items: pending.into_iter().map(|i| i.id).collect(),
        started_at: now,
        phase: Phase::Agent,
        calls: 0,
        nudges: 0,
        agent_pid: None,
        limit_resets_at: None,
        last_message: None,
        budget_usd: Some(settings.budget()),
        wall_ms: Some(u64::from(settings.wall_minutes) * MS_PER_MIN),
        spent_usd: 0.0,
        agent_ms: 0,
    };
    st.run = Some(run.clone());
    ctx.save_state(st)?;
    if let Err(e) = take_baseline(ctx, &run) {
        return Ok(Err(fail(
            ctx,
            st,
            &run,
            RunOutcome::Failed {
                reason: format!("基準を取れない: {e}"),
            },
        )?));
    }
    Ok(Ok(run))
}

fn take_baseline(ctx: &Ctx, run: &RunState) -> Result<()> {
    let run_dir = ctx.cfg.run_dir(&run.id);
    let root = ctx.root_of(run);
    let threshold = ctx.settings()?.perf_threshold;
    let cx = CheckCx {
        root: &root,
        run_dir: &run_dir,
        changed: &[],
        perf_threshold: threshold,
    };
    let baseline = ctx.checks.baseline(&cx)?;
    write_json(&run_dir.join(BASELINE_FILE), &baseline)
}

fn persist(ctx: &Ctx, st: &mut ImproveState, run: &RunState) -> Result<()> {
    st.run = Some(run.clone());
    ctx.save_state(st)
}

/// 実装を任せる。Stop で区切ったら検査へ渡す。
fn drive(ctx: &Ctx, mut st: ImproveState, mut run: RunState) -> Result<Step> {
    // 前のプロセスが起こした `claude -p` の pid は番号が使い回されうるので、このプロセスでは使わない。
    run.agent_pid = None;
    fill_limits(ctx, &mut run)?;
    recover_stop(ctx, &mut run);
    let mut prompt = first_prompt(ctx, &run)?;
    while run.phase == Phase::Agent {
        if ctx.stop_requested() {
            return fail(ctx, &mut st, &run, RunOutcome::Stopped);
        }
        if let Some(reason) = exhausted(&run) {
            return fail(ctx, &mut st, &run, RunOutcome::Failed { reason });
        }
        // 呼ぶ前に回数を書く。呼んでいる間にプロセスが落ちても、次は同じセッションを続ける。
        let resume = run.calls > 0;
        run.calls += 1;
        run.limit_resets_at = None;
        persist(ctx, &mut st, &run)?;
        let end = call_agent(ctx, &mut run, &prompt, resume)?;
        match end {
            AgentEnd::UsageLimit { resets_at } => {
                run.limit_resets_at = Some(resets_at);
                persist(ctx, &mut st, &run)?;
                return Ok(Step::Limited { resets_at });
            }
            AgentEnd::Failed { reason } => {
                let outcome = if ctx.stop_requested() {
                    RunOutcome::Stopped
                } else {
                    RunOutcome::Failed { reason }
                };
                return fail(ctx, &mut st, &run, outcome);
            }
            AgentEnd::Finished { last_message } => after_turn(&mut run, last_message, &mut prompt),
        }
        persist(ctx, &mut st, &run)?;
    }
    crate::gate::gate(ctx, st, run)
}

/// ターンの終わり。印があれば検査へ、無ければ続きを頼む（上限まで。超えたら印の無いまま検査で落とす）。
fn after_turn(run: &mut RunState, last_message: String, prompt: &mut String) {
    if !last_message.contains(DONE_MARK) && run.nudges < MAX_NUDGES {
        run.nudges += 1;
        *prompt = NUDGE_PROMPT.to_owned();
        return;
    }
    run.last_message = Some(last_message);
    run.phase = Phase::Gate;
}

/// 予算と時間の上限を持たない回（回の途中で版が変わった状態のファイル）に、いまの設定の値を入れる。
fn fill_limits(ctx: &Ctx, run: &mut RunState) -> Result<()> {
    if run.budget_usd.is_some() && run.wall_ms.is_some() {
        return Ok(());
    }
    let s = ctx.settings()?;
    run.budget_usd.get_or_insert(s.budget());
    run.wall_ms
        .get_or_insert(u64::from(s.wall_minutes) * MS_PER_MIN);
    Ok(())
}

/// 回の予算、時間、呼び出しの回数を使い切っていれば理由。使用制限の後の続きと催促も同じ上限に数える。
fn exhausted(run: &RunState) -> Option<String> {
    let budget = run.budget_usd.unwrap_or(DEFAULT_BUDGET_USD);
    let wall = run.wall_ms.unwrap_or(0);
    if run.spent_usd >= budget {
        return Some(format!(
            "回の予算 {budget} ドルを使い切った（{:.2} ドル）",
            run.spent_usd
        ));
    }
    if run.agent_ms >= wall {
        return Some(format!(
            "回の時間の上限 {} 分を使い切った",
            wall / MS_PER_MIN
        ));
    }
    (run.calls >= MAX_CALLS).then(|| format!("claude -p を {MAX_CALLS} 回呼んでも回が終わらない"))
}

/// 回を動かしていたプロセスが、Stop の後で検査の前に落ちていたら、フックが残した最後の発言で区切る。
fn recover_stop(ctx: &Ctx, run: &mut RunState) {
    if run.phase != Phase::Agent || run.calls == 0 || run.limit_resets_at.is_some() {
        return;
    }
    if let Some(m) = stop_message(&ctx.cfg.run_dir(&run.id)).filter(|m| m.contains(DONE_MARK)) {
        run.last_message = Some(m);
        run.phase = Phase::Gate;
    }
}

fn first_prompt(ctx: &Ctx, run: &RunState) -> Result<String> {
    if run.calls > 0 {
        return Ok(CONTINUE_PROMPT.to_owned());
    }
    let items: Vec<_> = run
        .items
        .iter()
        .filter_map(|id| ctx.store.get(id).ok())
        .collect();
    let merge = ctx.areas()?.improve.merge;
    Ok(prompt_for(
        &items,
        &PromptContext {
            merge: &merge,
            store: &ctx.store,
        },
    ))
}

/// 1 回呼ぶ。予算と時間は回の残りだけを渡し、使った分を回に足す。
fn call_agent(ctx: &Ctx, run: &mut RunState, prompt: &str, resume: bool) -> Result<AgentEnd> {
    let mut settings = ctx.settings()?;
    settings.budget_usd = Some(run.budget_usd.unwrap_or(DEFAULT_BUDGET_USD) - run.spent_usd);
    let wall = Duration::from_millis(run.wall_ms.unwrap_or(0).saturating_sub(run.agent_ms));
    let run_dir = ctx.cfg.run_dir(&run.id);
    let root = ctx.root_of(run);
    let read_dirs = [ctx.store.dir().to_path_buf()];
    let record_pid = |pid: u32| {
        // すぐ止める手段がほかのプロセスからも pid を引けるよう、状態に書く。書けなくても回は続ける。
        if let Ok(mut st) = ctx.state() {
            if let Some(r) = st.run.as_mut() {
                r.agent_pid = Some(pid);
                drop(ctx.save_state(&st));
            }
        }
    };
    let cost = Mutex::new(None::<f64>);
    let record_cost = |usd: f64| *cost.lock().unwrap_or_else(|e| e.into_inner()) = Some(usd);
    let req = AgentRequest {
        session_id: &run.session_id,
        resume,
        prompt,
        cwd: &root,
        run_dir: &run_dir,
        read_dirs: &read_dirs,
        settings: &settings,
        wall,
        on_spawn: &record_pid,
        on_cost: &record_cost,
    };
    let started = Instant::now();
    let end = ctx.agent.run(&req);
    run.agent_ms += u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if let Some(total) = cost.into_inner().unwrap_or_else(|e| e.into_inner()) {
        run.spent_usd = run.spent_usd.max(total);
    }
    Ok(end)
}

/// 回を終える（失敗、止めた）。worktree とブランチを消し、要望は未対応に残して待機する。
pub(crate) fn fail(
    ctx: &Ctx,
    st: &mut ImproveState,
    run: &RunState,
    outcome: RunOutcome,
) -> Result<Step> {
    st.waiting = Some(match &outcome {
        RunOutcome::Stopped => "人が止めた".to_owned(),
        RunOutcome::Failed { reason } | RunOutcome::Rejected { reason } => reason.clone(),
        _ => "回が止まった".to_owned(),
    });
    if outcome == RunOutcome::Stopped {
        ctx.clear_stop()?;
    }
    let report = RunReport {
        run_id: run.id.clone(),
        outcome,
        items: run.items.clone(),
        done: Vec::new(),
        changed: Vec::new(),
        checks: Vec::new(),
        finished_at: now_ms(),
    };
    finish_run(ctx, st, run, &report)?;
    git::worktree_remove(&ctx.cfg.repo, &run.worktree, &run.branch, true)?;
    Ok(Step::Finished(report))
}

/// 状態から回を外し、記録を書く。
pub(crate) fn finish_run(
    ctx: &Ctx,
    st: &mut ImproveState,
    run: &RunState,
    report: &RunReport,
) -> Result<()> {
    write_json(&ctx.cfg.run_dir(&run.id).join(REPORT_FILE), report)?;
    st.run = None;
    st.last = Some(report.clone());
    ctx.save_state(st)
}
