//! 起票 → 回 → 囲い → 取り込み / PR 止め → 返事、の通し。`claude -p` は決まった差分を書く偽物にする。

use std::sync::Arc;

use mm_improve::{
    CheckOutcome, FeedbackStatus, ImproveState, ListFilter, NewFeedback, Outcome, RunOutcome,
    SettingsPatch, Step, Verdict,
};

use crate::support::{FakeAgent, FakeChecks, FakeForge, Fixture, Script};

fn feedback(text: &str) -> NewFeedback {
    NewFeedback {
        text: text.into(),
        ..Default::default()
    }
}

fn finished(step: Step) -> mm_improve::RunReport {
    match step {
        Step::Finished(r) => r,
        other => panic!("回が終わっていない: {other:?}"),
    }
}

fn state(fx: &Fixture) -> ImproveState {
    serde_json::from_slice(&std::fs::read(fx.data.join("state.json")).unwrap()).unwrap()
}

#[test]
fn feedback_in_merge_area_lands_on_mainline_with_reply() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Write(vec![(
        "src/greet.txt",
        "Hello, world\n",
    )])]);
    let checks = FakeChecks::passing();
    let imp = fx.improver(agent.clone(), checks.clone(), Arc::default());
    let item = imp.submit(feedback("greet の綴りを直して")).unwrap();

    let report = finished(imp.run_once().unwrap());

    assert!(
        matches!(report.outcome, RunOutcome::Landed { .. }),
        "{report:?}"
    );
    assert_eq!(fx.main_file("src/greet.txt"), "Hello, world");
    assert_eq!(
        fx.file_at("main", "src/greet.txt"),
        "Helo, world",
        "人の main は動かさない"
    );
    assert_eq!(
        *checks.seen.lock().unwrap(),
        vec!["src/greet.txt".to_owned()]
    );
    let item = imp.get(&item.id).unwrap();
    assert_eq!(item.status, FeedbackStatus::Done);
    assert_eq!(item.replies[0].text, "直しました");
    assert!(matches!(item.replies[0].outcome, Outcome::Landed { .. }));
    assert_eq!(fx.worktrees(), 1, "回の worktree が消えている");
    assert!(!fx.branch_exists(&format!("fix/movie-maker-improve-{}", report.run_id)));
    assert_eq!(imp.status().unwrap().pending, 0);
}

#[test]
fn change_outside_merge_area_stops_at_pull_request() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Write(vec![(
        "core/engine.txt",
        "faster engine\n",
    )])]);
    let forge = Arc::new(FakeForge::default());
    let imp = fx.improver(agent, FakeChecks::passing(), forge.clone());
    let item = imp.submit(feedback("エンジンを速く")).unwrap();

    let report = finished(imp.run_once().unwrap());

    let RunOutcome::PullRequest { url, reason } = &report.outcome else {
        panic!("{report:?}")
    };
    assert_eq!(url, "https://example.invalid/pr/1");
    assert!(reason.contains("core/engine.txt"), "{reason}");
    assert_eq!(
        fx.main_file("core/engine.txt"),
        "engine",
        "本線は変わらない"
    );
    let branch = format!("fix/movie-maker-improve-{}", report.run_id);
    assert!(fx.branch_exists(&branch), "PR のブランチは残る");
    assert_eq!(forge.prs.lock().unwrap()[0].0, branch);
    assert_eq!(imp.get(&item.id).unwrap().status, FeedbackStatus::Held);
}

#[test]
fn pull_request_carries_only_the_round_commit_on_origin_upstream() {
    let fx = Fixture::new();
    fx.local_commit("src/local.txt", "local only\n");
    let agent = FakeAgent::new(vec![Script::Write(vec![("core/engine.txt", "faster\n")])]);
    let forge = Arc::new(FakeForge::default());
    let imp = fx.improver(agent, FakeChecks::passing(), forge.clone());
    imp.submit(feedback("エンジンを速く")).unwrap();

    let report = finished(imp.run_once().unwrap());

    assert!(
        matches!(report.outcome, RunOutcome::PullRequest { .. }),
        "{report:?}"
    );
    let branch = forge.prs.lock().unwrap()[0].0.clone();
    assert_eq!(
        fx.diff_names("origin/main", &branch),
        vec!["mm/core/engine.txt".to_owned()],
        "取り込み先の先行分は PR に載らない"
    );
}

#[test]
fn cherry_pick_conflict_replies_without_pull_request() {
    let fx = Fixture::new();
    fx.local_commit("core/local.txt", "local only\n");
    let agent = FakeAgent::new(vec![Script::Write(vec![("core/local.txt", "changed\n")])]);
    let forge = Arc::new(FakeForge::default());
    let imp = fx.improver(agent, FakeChecks::passing(), forge.clone());
    let item = imp.submit(feedback("local を直して")).unwrap();

    let report = finished(imp.run_once().unwrap());

    let RunOutcome::Rejected { reason } = &report.outcome else {
        panic!("{report:?}")
    };
    assert!(
        reason.contains("cherry-pick") && reason.contains("PR は作っていない"),
        "{reason}"
    );
    assert!(forge.prs.lock().unwrap().is_empty());
    let item = imp.get(&item.id).unwrap();
    assert_eq!(item.status, FeedbackStatus::Pending);
    assert!(
        matches!(&item.replies[0].outcome, Outcome::Rejected { reason } if reason.contains("cherry-pick"))
    );
    let branch = format!("fix/movie-maker-improve-{}", report.run_id);
    assert_eq!(
        fx.file_at(&branch, "core/local.txt"),
        "changed",
        "回の変更はブランチに残る"
    );
}

#[test]
fn hold_path_inside_merge_area_stops_at_pull_request() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Write(vec![(
        "src/golden/a.txt",
        "new golden\n",
    )])]);
    let imp = fx.improver(agent, FakeChecks::passing(), Arc::default());
    imp.submit(feedback("期待画像を更新")).unwrap();

    let report = finished(imp.run_once().unwrap());

    assert!(
        matches!(report.outcome, RunOutcome::PullRequest { .. }),
        "{report:?}"
    );
}

#[test]
fn perf_regression_is_not_landed_and_reply_names_the_item() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Write(vec![("src/greet.txt", "Hello\n")])]);
    let slow = CheckOutcome::new(
        "perf",
        Verdict::Fail,
        "render.frame  12.0  13.5  +12.5% 回帰",
    );
    let imp = fx.improver(agent, FakeChecks::with(vec![slow]), Arc::default());
    let item = imp.submit(feedback("greet")).unwrap();

    let report = finished(imp.run_once().unwrap());

    let RunOutcome::Rejected { reason } = &report.outcome else {
        panic!("{report:?}")
    };
    assert!(
        reason.contains("render.frame") && reason.contains("+12.5%"),
        "{reason}"
    );
    assert_eq!(fx.main_file("src/greet.txt"), "Helo, world");
    let item = imp.get(&item.id).unwrap();
    assert_eq!(item.status, FeedbackStatus::Pending, "未対応に残る");
    assert!(
        matches!(&item.replies[0].outcome, Outcome::Rejected { reason } if reason.contains("render.frame"))
    );
    assert!(state(&fx).waiting.is_some());
    assert!(
        matches!(imp.run_once().unwrap(), Step::Idle(_)),
        "待機中は回を起こさない"
    );
    imp.submit(feedback("次の要望")).unwrap();
    assert!(state(&fx).waiting.is_none(), "新しい要望で待機を解く");
}

#[test]
fn export_hash_change_stops_at_pull_request() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Write(vec![("src/greet.txt", "Hello\n")])]);
    let changed = CheckOutcome::new("export", Verdict::Hold, "書き出しのハッシュが変わった");
    let imp = fx.improver(agent, FakeChecks::with(vec![changed]), Arc::default());
    imp.submit(feedback("greet")).unwrap();

    let report = finished(imp.run_once().unwrap());

    let RunOutcome::PullRequest { reason, .. } = &report.outcome else {
        panic!("{report:?}")
    };
    assert!(reason.contains("ハッシュ"), "{reason}");
}

#[test]
fn reply_without_change_is_done_without_commit() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Write(vec![])]);
    let imp = fx.improver(agent, FakeChecks::passing(), Arc::default());
    let item = imp.submit(feedback("直さなくてよいもの")).unwrap();

    let report = finished(imp.run_once().unwrap());

    assert_eq!(report.outcome, RunOutcome::NoChange);
    let item = imp.get(&item.id).unwrap();
    assert_eq!(item.status, FeedbackStatus::Done);
    assert_eq!(item.replies[0].outcome, Outcome::NoChange);
}

#[test]
fn usage_limit_resumes_the_same_session_after_reset() {
    let fx = Fixture::new();
    let past = mm_improve_now() - 120_000;
    let agent = FakeAgent::new(vec![
        Script::Limit(past),
        Script::Write(vec![("src/greet.txt", "Hello\n")]),
    ]);
    let imp = fx.improver(agent.clone(), FakeChecks::passing(), Arc::default());
    imp.submit(feedback("greet")).unwrap();

    assert_eq!(imp.run_once().unwrap(), Step::Limited { resets_at: past });
    assert!(state(&fx).run.is_some(), "使用制限では回を区切らない");
    let report = finished(imp.run_once().unwrap());

    assert!(matches!(report.outcome, RunOutcome::Landed { .. }));
    let calls = agent.calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls[0].0 && calls[1].0, "2 回目は同じセッションを続ける");
    assert_eq!(calls[1].1, "続けて");
}

#[test]
fn usage_limit_waits_until_reset() {
    let fx = Fixture::new();
    let future = mm_improve_now() + 3_600_000;
    let agent = FakeAgent::new(vec![Script::Limit(future)]);
    let imp = fx.improver(agent.clone(), FakeChecks::passing(), Arc::default());
    imp.submit(feedback("greet")).unwrap();

    imp.run_once().unwrap();
    assert_eq!(imp.run_once().unwrap(), Step::Limited { resets_at: future });
    assert_eq!(agent.calls().len(), 1, "明けるまで呼ばない");
    assert_eq!(
        imp.status().unwrap().run.unwrap().limit_resets_at,
        Some(future)
    );
}

#[test]
fn stop_ends_a_limited_run_and_turns_off() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Limit(mm_improve_now() + 3_600_000)]);
    let imp = fx.improver(agent, FakeChecks::passing(), Arc::default());
    let item = imp.submit(feedback("greet")).unwrap();
    imp.run_once().unwrap();

    let status = imp.stop().unwrap();

    assert!(!status.settings.enabled);
    assert!(status.run.is_none());
    assert_eq!(status.waiting.as_deref(), Some("人が止めた"));
    assert_eq!(fx.worktrees(), 1);
    assert_eq!(imp.get(&item.id).unwrap().status, FeedbackStatus::Pending);
    assert!(matches!(status.last.unwrap().outcome, RunOutcome::Stopped));
}

#[test]
fn missing_done_mark_nudges_twice_then_waits() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![
        Script::Say("やった"),
        Script::Say("やった"),
        Script::Say("やった"),
    ]);
    let imp = fx.improver(agent.clone(), FakeChecks::passing(), Arc::default());
    imp.submit(feedback("greet")).unwrap();

    let report = finished(imp.run_once().unwrap());

    assert!(
        matches!(report.outcome, RunOutcome::Failed { .. }),
        "{report:?}"
    );
    let calls = agent.calls();
    assert_eq!(calls.len(), 3);
    assert!(calls[1].1.contains("対応した要望:") && calls[1].0);
    assert!(state(&fx).waiting.is_some());
    assert_eq!(imp.list(ListFilter::Pending).unwrap().len(), 1);
}

#[test]
fn off_does_not_start_a_round() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![]);
    let imp = fx.improver(agent.clone(), FakeChecks::passing(), Arc::default());
    imp.update_settings(&SettingsPatch {
        enabled: Some(false),
        ..Default::default()
    })
    .unwrap();
    imp.submit(feedback("greet")).unwrap();

    assert_eq!(imp.run_once().unwrap(), Step::Idle("OFF".into()));
    assert!(agent.calls().is_empty());
}

#[test]
fn background_round_starts_when_feedback_is_saved() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Write(vec![("src/greet.txt", "Hello\n")])]);
    let imp = fx
        .improver(agent, FakeChecks::passing(), Arc::default())
        .with_background();

    let item = imp.submit(feedback("greet")).unwrap();
    imp.wait_idle();

    assert_eq!(imp.get(&item.id).unwrap().status, FeedbackStatus::Done);
    assert_eq!(fx.main_file("src/greet.txt"), "Hello");
}

fn mm_improve_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
