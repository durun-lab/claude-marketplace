//! 実物の `claude -p` での通し。フィクスチャのリポジトリで小さな要望を直させ、取り込みと返事まで確かめる。
//! 費用がかかるので既定では実行しない。`cargo nextest run -p mm-improve --run-ignored only real_claude` で 1 回だけ回す。

use std::sync::Arc;

use mm_improve::{
    ClaudeCli, FeedbackStatus, Improver, NewFeedback, RunOutcome, SettingsPatch, Step,
};

use crate::support::{FakeChecks, FakeForge, Fixture};

const BUDGET_USD: f64 = 1.0;
const WALL_MINUTES: u32 = 15;

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
