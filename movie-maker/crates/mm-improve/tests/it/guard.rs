//! 囲いの抜けを塞いだことの確かめ。名前の変更、取り込み先を動かす回、検査の最中の停止、取り込めない取り込み先、
//! 回の予算の累計、自動の起票の同時の記録。

use std::sync::Arc;

use mm_improve::{
    AgentEnd, AgentRequest, ErrorInput, ErrorSource, FeedbackStatus, ListFilter, NewFeedback,
    RunOutcome, Step,
};

use crate::support::{done_for, run_git, FakeAgent, FakeChecks, Fixture, Script};

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

fn say(text: &str) -> AgentEnd {
    AgentEnd::Finished {
        last_message: text.into(),
    }
}

#[test]
fn rename_from_outside_into_merge_area_stops_at_pull_request() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Run(|req: &AgentRequest<'_>, first: &str| {
        std::fs::rename(
            req.cwd.join("core/engine.txt"),
            req.cwd.join("src/engine.txt"),
        )
        .unwrap();
        done_for(first)
    })]);
    let imp = fx.improver(agent, FakeChecks::passing(), Arc::default());
    imp.submit(feedback("エンジンを src に移して")).unwrap();

    let report = finished(imp.run_once().unwrap());

    let RunOutcome::PullRequest { reason, .. } = &report.outcome else {
        panic!("{report:?}")
    };
    assert!(
        reason.contains("core/engine.txt"),
        "消した側も判定する: {reason}"
    );
    assert_eq!(fx.main_file("core/engine.txt"), "engine");
}

#[test]
fn mainline_moved_during_round_stops_at_pull_request() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Run(|req: &AgentRequest<'_>, first: &str| {
        std::fs::write(req.cwd.join("src/greet.txt"), "Hello\n").unwrap();
        run_git(req.cwd, &["commit", "-q", "--allow-empty", "-m", "sneak"]);
        run_git(
            req.cwd,
            &["branch", "-f", mm_improve::DEFAULT_MAINLINE, "HEAD"],
        );
        done_for(first)
    })]);
    let imp = fx.improver(agent, FakeChecks::passing(), Arc::default());
    imp.submit(feedback("greet")).unwrap();

    let report = finished(imp.run_once().unwrap());

    let RunOutcome::PullRequest { reason, .. } = &report.outcome else {
        panic!("{report:?}")
    };
    assert!(reason.contains("が動いた"), "{reason}");
    assert_eq!(fx.main_file("src/greet.txt"), "Helo, world");
}

#[test]
fn stop_during_checks_does_not_land() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![Script::Write(vec![("src/greet.txt", "Hello\n")])]);
    let checks = FakeChecks::stopped_during_verify(&fx.data);
    let imp = fx.improver(agent, checks, Arc::default());
    let item = imp.submit(feedback("greet")).unwrap();

    let report = finished(imp.run_once().unwrap());

    assert_eq!(report.outcome, RunOutcome::Stopped);
    assert_eq!(fx.main_file("src/greet.txt"), "Helo, world");
    assert_eq!(imp.get(&item.id).unwrap().status, FeedbackStatus::Pending);
    assert!(!fx.data.join("stop").exists(), "止めた印は片付ける");
}

#[test]
fn landing_on_a_dirty_checkout_is_rejected_and_keeps_human_changes() {
    let fx = Fixture::new();
    run_git(
        &fx.repo,
        &["checkout", "-q", "-b", mm_improve::DEFAULT_MAINLINE],
    );
    let wip = fx.repo.join("mm/src/greet.txt");
    std::fs::write(&wip, "human WIP\n").unwrap();
    let agent = FakeAgent::new(vec![Script::Write(vec![("src/greet.txt", "Hello\n")])]);
    let imp = fx.improver(agent, FakeChecks::passing(), Arc::default());
    let item = imp.submit(feedback("greet")).unwrap();

    let report = finished(imp.run_once().unwrap());

    let RunOutcome::Rejected { reason } = &report.outcome else {
        panic!("{report:?}")
    };
    assert!(reason.contains("取り込めない"), "{reason}");
    assert_eq!(std::fs::read_to_string(&wip).unwrap(), "human WIP\n");
    assert_eq!(imp.get(&item.id).unwrap().status, FeedbackStatus::Pending);
    assert!(
        imp.status().unwrap().run.is_none(),
        "回は検査の段に残らない"
    );
}

#[test]
fn later_calls_get_only_the_rest_of_the_budget() {
    let fx = Fixture::new();
    let agent = FakeAgent::new(vec![
        Script::Run(|req: &AgentRequest<'_>, _: &str| {
            (req.on_cost)(2.0);
            say("途中")
        }),
        Script::Run(|req: &AgentRequest<'_>, _: &str| {
            (req.on_cost)(6.0);
            say("まだ途中")
        }),
    ]);
    let imp = fx.improver(agent.clone(), FakeChecks::passing(), Arc::default());
    imp.submit(feedback("greet")).unwrap();

    let report = finished(imp.run_once().unwrap());

    let RunOutcome::Failed { reason } = &report.outcome else {
        panic!("{report:?}")
    };
    assert!(reason.contains("予算"), "{reason}");
    assert_eq!(
        *agent.budgets.lock().unwrap(),
        vec![Some(5.0), Some(3.0)],
        "続きの呼び出しには残りだけを渡す"
    );
}

#[test]
fn concurrent_records_of_the_same_error_file_one_item() {
    let fx = Fixture::new();
    let imp = fx.improver(
        FakeAgent::new(vec![]),
        FakeChecks::passing(),
        Arc::default(),
    );
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let imp = imp.clone();
            std::thread::spawn(move || {
                imp.record(&ErrorInput::error(ErrorSource::App, "boom"))
                    .unwrap()
            })
        })
        .collect();
    let filed = threads
        .into_iter()
        .filter_map(|t| t.join().unwrap())
        .count();

    assert_eq!(filed, 1);
    assert_eq!(imp.list(ListFilter::Pending).unwrap().len(), 1);
}
