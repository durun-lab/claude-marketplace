//! 落ちた回の後片付け。回を動かしていたプロセスが worktree やブランチを消す前に落ちても、次の起動で片付く。
//! ロック（`run.lock`）は OS のファイルのロックなので、プロセスが落ちれば外れる（tree.rs の試験が次の回を起こせることを確かめる）。

use std::sync::Arc;

use mm_improve::{CheckOutcome, NewFeedback, RunOutcome, Step, Verdict};

use crate::support::{run_git, FakeAgent, FakeChecks, Fixture, Script};

const BRANCH_PREFIX: &str = "fix/movie-maker-improve-";

fn feedback(text: &str) -> NewFeedback {
    NewFeedback {
        text: text.into(),
        ..Default::default()
    }
}

fn report(step: Step) -> mm_improve::RunReport {
    match step {
        Step::Finished(r) => r,
        other => panic!("回が終わっていない: {other:?}"),
    }
}

#[test]
fn worktree_left_by_a_crash_is_removed_on_next_start() {
    let fx = Fixture::new();
    let old = fx.data.join("worktrees").join("old");
    let branch = format!("{BRANCH_PREFIX}old");
    run_git(
        &fx.repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &branch,
            old.to_str().unwrap(),
            "main",
        ],
    );
    let stray = fx.data.join("worktrees").join("stray");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("half.txt"), "消しかけ").unwrap();
    let imp = fx.improver(
        FakeAgent::new(vec![]),
        FakeChecks::passing(),
        Arc::default(),
    );

    let step = imp.run_once().unwrap();

    assert!(matches!(step, Step::Idle(_)), "{step:?}");
    assert_eq!(fx.worktrees(), 1, "回の worktree が残った");
    assert!(!fx.branch_exists(&branch), "回のブランチが残った");
    assert!(!old.exists() && !stray.exists());
}

#[test]
fn branches_of_finished_runs_follow_their_report() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Write(vec![("src/greet.txt", "Hello\n")])]);
    let fail = CheckOutcome::new("check", Verdict::Fail, "cargo check が通らない");
    let imp = fx.improver(agent, FakeChecks::with(vec![fail]), Arc::default());
    imp.submit(feedback("greet")).unwrap();
    let rejected = report(imp.run_once().unwrap());
    assert!(matches!(rejected.outcome, RunOutcome::Rejected { .. }));
    imp.submit(feedback("次")).unwrap();
    let failed = report(imp.run_once().unwrap());
    assert!(matches!(failed.outcome, RunOutcome::Failed { .. }));
    // worktree を消してからブランチを消すまでの間に落ちた回を作る。
    let left = format!("{BRANCH_PREFIX}{}", failed.run_id);
    run_git(&fx.repo, &["branch", &left, "main"]);
    let foreign = format!("{BRANCH_PREFIX}foreign");
    run_git(&fx.repo, &["branch", &foreign, "main"]);

    imp.run_once().unwrap();

    assert!(!fx.branch_exists(&left), "失敗した回のブランチが残った");
    let kept = format!("{BRANCH_PREFIX}{}", rejected.run_id);
    assert!(
        fx.branch_exists(&kept),
        "取り込めなかった回のブランチは人が見るので残す"
    );
    assert!(
        fx.branch_exists(&foreign),
        "この置き場所の回でないブランチは触らない"
    );
}
