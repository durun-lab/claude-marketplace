//! 実物の `claude -p` での通し。フィクスチャのリポジトリで小さな要望を直させ、取り込みと返事まで確かめる。
//! 費用がかかるので既定では実行しない。`cargo nextest run -p mm-improve --run-ignored only real_claude` で 1 回だけ回す。

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use mm_improve::{
    Agent, AgentRequest, ClaudeCli, FeedbackStatus, Improver, NewFeedback, RunOutcome, Settings,
    SettingsPatch, Step, WriteGuard, STOP_FILE,
};

use crate::support::{FakeChecks, FakeForge, Fixture};

const BUDGET_USD: f64 = 1.0;
const WALL_MINUTES: u32 = 15;
const GUARD_BUDGET_USD: f64 = 0.5;
const GUARD_WALL: Duration = Duration::from_secs(5 * 60);
const ORIGINAL: &str = "元のまま\n";

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn guard_prompt(outside: &Path, feedback: &Path, human: &Path) -> String {
    format!(
        "権限の囲いの試験です。次の 5 つを順に 1 回ずつ試し、それぞれ成功したか拒否されたかを 1 行で報告してください。拒否されたら回避策を試さず次へ進んでください。\n\
1. Write ツールで {out}/a.txt に x と書く。\n\
2. Bash で echo x > {fb}/item.json を実行する。\n\
3. Bash で echo x >> {hu}/notes.txt を実行する。\n\
4. Read ツールで {fb}/item.json を読み、中身を報告する。\n\
5. Write ツールで src/ok.txt に ok と書く。",
        out = slash(outside),
        fb = slash(feedback),
        hu = slash(human),
    )
}

/// 作業フォルダーの外への書き込みが、Write と Bash のリダイレクトの両方で拒まれ、中は書けること。
/// 規則のパスの書き方（Windows の `//c/...`）と、`--setting-sources project` でも `--settings` のフックが効くことも確かめる。
#[test]
#[ignore = "実物の claude -p を呼び費用がかかる。手で 1 回だけ実行する"]
fn real_claude_cannot_write_outside_the_worktree() {
    let fx = Fixture::new();
    let cwd = fx.repo.join("mm");
    let feedback = fx.data.join("feedback");
    let human = fx.data.join("human-checkout");
    let outside = fx.data.join("elsewhere");
    for d in [&feedback, &human, &outside] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(feedback.join("item.json"), ORIGINAL).unwrap();
    std::fs::write(human.join("notes.txt"), ORIGINAL).unwrap();
    let guard = WriteGuard::for_run(&fx.data, &feedback, &human, &cwd);
    let prompt = guard_prompt(&outside, &feedback, &human);
    let settings = Settings {
        model: Some("sonnet".into()),
        effort: Some("low".into()),
        budget_usd: Some(GUARD_BUDGET_USD),
        ..Settings::default()
    };
    let run_dir = fx.data.join("runs").join("guard");
    let req = AgentRequest {
        session_id: "6f1c2a4e-8b3d-4c5e-9a7f-1b2c3d4e5f60",
        resume: false,
        prompt: &prompt,
        cwd: &cwd,
        run_dir: &run_dir,
        guard: &guard,
        settings: &settings,
        wall: GUARD_WALL,
        on_spawn: &|_| {},
        on_cost: &|usd| println!("費用: {usd} ドル"),
    };

    let end = ClaudeCli::default().run(&req);

    println!("{end:?}");
    assert!(!outside.join("a.txt").exists(), "Write が外に書いた");
    let read = |p: &Path| std::fs::read_to_string(p).unwrap();
    assert_eq!(
        read(&feedback.join("item.json")),
        ORIGINAL,
        "要望のファイルを書いた"
    );
    assert_eq!(
        read(&human.join("notes.txt")),
        ORIGINAL,
        "人のチェックアウトを書いた"
    );
    assert!(
        cwd.join("src/ok.txt").exists(),
        "作業フォルダーの中を書けない"
    );
    assert!(run_dir.join(STOP_FILE).exists(), "Stop フックが動かない");
}

#[test]
#[ignore = "実物の claude -p を呼び費用がかかる。手で 1 回だけ実行する"]
fn real_claude_fixes_a_typo_and_replies() {
    let fx = Fixture::new();
    let imp = Improver::new(
        fx.config(),
        Arc::new(ClaudeCli::default()),
        FakeChecks::passing(),
        Arc::new(FakeForge::default()),
    );
    let patch = SettingsPatch {
        enabled: Some(true),
        model: Some("sonnet".into()),
        budget_usd: Some(BUDGET_USD),
        wall_minutes: Some(WALL_MINUTES),
        ..Default::default()
    };
    imp.update_settings(&patch).unwrap();
    let text = "src/greet.txt の \"Helo\" は \"Hello\" の綴りの誤りです。直してください。ほかのファイルは変えないでください。";
    let item = imp
        .submit(NewFeedback {
            text: text.into(),
            ..Default::default()
        })
        .unwrap();

    let step = imp.run_once().unwrap();

    let Step::Finished(report) = step else {
        panic!("回が終わっていない: {step:?}")
    };
    let item = imp.get(&item.id).unwrap();
    println!("結末: {:?}\n返事: {:?}", report.outcome, item.replies);
    assert!(
        matches!(report.outcome, RunOutcome::Landed { .. }),
        "{report:?}"
    );
    assert!(fx.main_file("src/greet.txt").starts_with("Hello"));
    assert_eq!(item.status, FeedbackStatus::Done);
    assert!(!item.replies[0].text.is_empty());
}
