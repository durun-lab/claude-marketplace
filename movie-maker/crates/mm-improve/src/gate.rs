//! 囲いと取り込み。回の変更を 1 コミットにまとめて本線の先端に載せ直し、領域と検査で判定して、
//! 取り込むか、PR までで止めるか、取り込まずに差し戻す。済んだ要望には返事を書く。

use mm_types::Result;

use crate::areas::ImproveVerdict;
use crate::checks::{Baseline, CheckCx, CheckOutcome, Verdict};
use crate::errors::ErrorBook;
use crate::files::read_or_default;
use crate::forge::PullRequest;
use crate::git;
use crate::prompt::{commit_message, parse_done};
use crate::round::{fail, finish_run, Ctx, Step, BASELINE_FILE, ERRORS_FILE};
use crate::state::{ImproveState, RunOutcome, RunReport, RunState};
use crate::store::{FeedbackStatus, Outcome, Reply};
use crate::time::now_ms;

/// 検査の材料。
struct Gathered {
    done: Vec<(String, String)>,
    changed: Vec<String>,
    checks: Vec<CheckOutcome>,
    held: Vec<String>,
}

/// 区切った回を検査して、取り込むか止める。
pub(crate) fn gate(ctx: &Ctx, mut st: ImproveState, run: RunState) -> Result<Step> {
    let message = run.last_message.clone().unwrap_or_default();
    let done = parse_done(&message, &run.items);
    if done.is_empty() {
        let reason = "回が、対応を終えた要望を「対応した要望:」に書かずに止まった".to_owned();
        return fail(ctx, &mut st, &run, RunOutcome::Failed { reason });
    }
    let commit = git::squash(
        &run.worktree,
        &run.base_commit,
        &commit_message(&run.id, &done),
    )?;
    let empty = Gathered {
        done,
        changed: Vec::new(),
        checks: Vec::new(),
        held: Vec::new(),
    };
    if commit.is_none() {
        return conclude(ctx, &mut st, &run, empty, RunOutcome::NoChange);
    }
    if !git::rebase(&run.worktree, &ctx.cfg.mainline)? {
        let reason = format!(
            "{} の先端と衝突した（ブランチ {} に残した）",
            ctx.cfg.mainline, run.branch
        );
        return conclude(ctx, &mut st, &run, empty, RunOutcome::Rejected { reason });
    }
    let g = gather(ctx, &run, empty.done)?;
    let outcome = decide(ctx, &run, &g)?;
    conclude(ctx, &mut st, &run, g, outcome)
}

/// 検査で落ちたら差し戻し、人の承認が要れば PR までで止め、全部通れば本線へ取り込む。
fn decide(ctx: &Ctx, run: &RunState, g: &Gathered) -> Result<RunOutcome> {
    let fails: Vec<String> = g
        .checks
        .iter()
        .filter(|c| c.verdict == Verdict::Fail)
        .map(|c| format!("{}: {}", c.name, c.detail))
        .collect();
    if !fails.is_empty() {
        let reason = format!(
            "囲いの検査で落ちた（ブランチ {} に残した）\n{}",
            run.branch,
            fails.join("\n")
        );
        return Ok(RunOutcome::Rejected { reason });
    }
    if !g.held.is_empty() {
        return Ok(open_pr(ctx, run, g));
    }
    let commit = git::land(&ctx.cfg.repo, &ctx.cfg.mainline, &run.branch)?;
    Ok(RunOutcome::Landed { commit })
}

/// 変えたファイル、領域の判定、検査を集める。
fn gather(ctx: &Ctx, run: &RunState, done: Vec<(String, String)>) -> Result<Gathered> {
    let changed = to_subdir(git::changed_files(&run.worktree, &ctx.cfg.mainline)?, ctx);
    let mut held = match ctx.areas()?.improve_verdict(&changed) {
        ImproveVerdict::Merge => Vec::new(),
        ImproveVerdict::Hold(files) => {
            vec![format!("人の承認が要る所に触れた: {}", files.join("、"))]
        }
    };
    let run_dir = ctx.cfg.run_dir(&run.id);
    let baseline: Baseline = read_or_default(&run_dir.join(BASELINE_FILE))?;
    let root = ctx.root_of(run);
    let threshold = ctx.settings()?.perf_threshold;
    let cx = CheckCx {
        root: &root,
        run_dir: &run_dir,
        changed: &changed,
        perf_threshold: threshold,
    };
    let checks = ctx.checks.verify(&cx, &baseline);
    held.extend(
        checks
            .iter()
            .filter(|c| c.verdict == Verdict::Hold)
            .map(|c| format!("{}: {}", c.name, c.detail)),
    );
    Ok(Gathered {
        done,
        changed,
        checks,
        held,
    })
}

/// 根からの相対を movie-maker からの相対にする。movie-maker の外は `../` を付け、どの領域にも当たらないようにする。
fn to_subdir(files: Vec<String>, ctx: &Ctx) -> Vec<String> {
    let sub = ctx.cfg.subdir.to_string_lossy().replace('\\', "/");
    if sub.is_empty() {
        return files;
    }
    let prefix = format!("{sub}/");
    files
        .into_iter()
        .map(|f| {
            f.strip_prefix(&prefix)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("../{f}"))
        })
        .collect()
}

fn open_pr(ctx: &Ctx, run: &RunState, g: &Gathered) -> RunOutcome {
    let title = commit_message(&run.id, &g.done)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    let reason = g.held.join("\n");
    let body = pr_body(run, g, &reason);
    let pr = PullRequest {
        repo: &ctx.cfg.repo,
        branch: &run.branch,
        base: &ctx.cfg.upstream,
        title: &title,
        body: &body,
    };
    match ctx.forge.open_pr(&pr) {
        Ok(url) => RunOutcome::PullRequest { url, reason },
        Err(e) => RunOutcome::Rejected {
            reason: format!(
                "PR までで止めるはずが、PR を作れない（ブランチ {} に残した）: {e}",
                run.branch
            ),
        },
    }
}

fn pr_body(run: &RunState, g: &Gathered, reason: &str) -> String {
    let mut s = format!("自動の改善の回 {} の変更。人の承認が要るので取り込まずに止めた。\n\n## 止めた理由\n{reason}\n\n## 要望と返事\n", run.id);
    for (id, reply) in &g.done {
        s.push_str(&format!("- {id}: {reply}\n"));
    }
    s.push_str("\n## 検査\n");
    for c in &g.checks {
        s.push_str(&format!(
            "- {}: {:?} {}\n",
            c.name,
            c.verdict,
            c.detail.lines().next().unwrap_or("")
        ));
    }
    s
}

/// 回の結末から、要望の状態と要望に付ける結末を決める。差し戻した要望は未対応に残す。
fn item_outcome(outcome: &RunOutcome) -> (FeedbackStatus, Outcome) {
    match outcome {
        RunOutcome::Landed { commit } => (
            FeedbackStatus::Done,
            Outcome::Landed {
                commit: commit.clone(),
            },
        ),
        RunOutcome::PullRequest { url, reason } => (
            FeedbackStatus::Held,
            Outcome::PullRequest {
                url: url.clone(),
                reason: reason.clone(),
            },
        ),
        RunOutcome::Rejected { reason } | RunOutcome::Failed { reason } => (
            FeedbackStatus::Pending,
            Outcome::Rejected {
                reason: reason.clone(),
            },
        ),
        RunOutcome::NoChange | RunOutcome::Stopped => (FeedbackStatus::Done, Outcome::NoChange),
    }
}

/// 済んだ要望に返事を書く。済んだ要望を起票した自動の記録は数え直す。
fn reply_items(
    ctx: &Ctx,
    run: &RunState,
    done: &[(String, String)],
    outcome: &RunOutcome,
    now: u64,
) -> Result<()> {
    let (status, item_outcome) = item_outcome(outcome);
    let errors_path = ctx.cfg.file(ERRORS_FILE);
    let mut errors = ErrorBook::load(&errors_path)?;
    for (id, text) in done {
        let mut item = ctx.store.get(id)?;
        item.replies.push(Reply {
            at: now,
            run_id: run.id.clone(),
            text: text.clone(),
            outcome: item_outcome.clone(),
        });
        item.status = status;
        ctx.store.put(&item)?;
        if status != FeedbackStatus::Pending {
            errors.reset_item(id);
        }
    }
    errors.save(&errors_path)
}

/// 返事を書いて回を終える。
fn conclude(
    ctx: &Ctx,
    st: &mut ImproveState,
    run: &RunState,
    g: Gathered,
    outcome: RunOutcome,
) -> Result<Step> {
    let now = now_ms();
    reply_items(ctx, run, &g.done, &outcome, now)?;
    match &outcome {
        RunOutcome::Landed { .. } => st.last_landed = Some((run.id.clone(), now)),
        RunOutcome::Rejected { reason } => st.waiting = Some(reason.clone()),
        _ => {}
    }
    let keep_branch = matches!(
        outcome,
        RunOutcome::Rejected { .. } | RunOutcome::PullRequest { .. }
    );
    let report = RunReport {
        run_id: run.id.clone(),
        outcome,
        items: run.items.clone(),
        done: g.done.into_iter().map(|(id, _)| id).collect(),
        changed: g.changed,
        checks: g.checks,
        finished_at: now,
    };
    finish_run(ctx, st, run, &report)?;
    git::worktree_remove(&ctx.cfg.repo, &run.worktree, &run.branch, !keep_branch)?;
    Ok(Step::Finished(report))
}
