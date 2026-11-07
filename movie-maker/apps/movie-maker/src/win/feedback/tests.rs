//! 段階 7 の終わりの条件「要望を起票すると、自動で直って返事が付く」の通し。
//! アプリと同じ窓口（`Feedback`）と裏の回し手で、フィクスチャの git に要望を起票し、偽の `claude -p` が直して、
//! パネルが読む形（`feedback_list` の JSON）に返事が付くまでを確かめる。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mm_improve::{
    AgentEnd, AgentRequest, Baseline, CheckCx, CheckOutcome, Checks, ClaudeCli, ErrorInput,
    ErrorSource, FeedbackItem, FeedbackKind, FeedbackStatus, Forge, ImproveConfig, Improver,
    ListFilter, NewFeedback, PullRequest, SettingsPatch,
};
use serde_json::{json, Value};

use super::Feedback;

/// フィクスチャの areas.toml。`src/` は取り込んでよい。
const AREAS: &str = "[improve]\nmerge = [\"src/\"]\nhold = []\n";
/// Feedback パネルの試験とデモが使う、返事の付いた要望。アプリが返す JSON と同じ形であることをここで確かめる。
const PANEL_FIXTURE: &str = include_str!("../../../../../ui/panels/feedback/fixtures/replied.json");
/// 知らせの間を待つ上限（偽物の回）。
const WAIT: Duration = Duration::from_secs(120);
/// 実物の回の時間の上限（分）。知らせは回の始めと終わりにしか来ないので、待つ上限もこれに合わせる。
const REAL_WALL_MINUTES: u32 = 15;

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// 一時フォルダーの git。根の下の `mm/` を movie-maker に見立てる。
struct Fixture {
    _dir: tempfile::TempDir,
    repo: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let mm = repo.join("mm");
        std::fs::create_dir_all(mm.join("docs")).unwrap();
        std::fs::create_dir_all(mm.join("src")).unwrap();
        std::fs::write(mm.join("docs/areas.toml"), AREAS).unwrap();
        std::fs::write(mm.join("src/greet.txt"), "Helo, world\n").unwrap();
        run_git(dir.path(), &["init", "-q", "-b", "main", "repo"]);
        for (k, v) in [
            ("user.name", "movie-maker test"),
            ("user.email", "test@example.invalid"),
            ("core.autocrlf", "false"),
        ] {
            run_git(&repo, &["config", k, v]);
        }
        run_git(&repo, &["add", "-A"]);
        run_git(&repo, &["commit", "-q", "-m", "init"]);
        run_git(
            dir.path(),
            &["init", "-q", "--bare", "-b", "main", "origin.git"],
        );
        let origin = dir.path().join("origin.git");
        run_git(
            &repo,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        run_git(&repo, &["push", "-q", "origin", "main"]);
        let data = dir.path().join("data");
        Self {
            _dir: dir,
            repo,
            data,
        }
    }

    fn config(&self) -> ImproveConfig {
        ImproveConfig::new(&self.data, &self.repo).with_subdir("mm")
    }

    fn mainline_file(&self, path: &str) -> String {
        let rev = format!("{}:mm/{path}", mm_improve::DEFAULT_MAINLINE);
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            .args(["show", &rev])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    }
}

/// 綴りを直して、渡された要望の全部に返事をする `claude -p` の偽物。
struct FakeClaude;

impl mm_improve::Agent for FakeClaude {
    fn run(&self, req: &AgentRequest<'_>) -> AgentEnd {
        std::fs::write(req.cwd.join("src/greet.txt"), "Hello, world\n").unwrap();
        let ids: Vec<String> = req
            .prompt
            .lines()
            .filter_map(|l| l.strip_prefix("### "))
            .filter_map(|l| l.split('（').next())
            .map(|id| format!("- {id}: greet.txt の綴りを直しました"))
            .collect();
        AgentEnd::Finished {
            last_message: format!("直した。\n\n対応した要望:\n{}", ids.join("\n")),
        }
    }
}

struct PassingChecks;

impl Checks for PassingChecks {
    fn baseline(&self, _cx: &CheckCx<'_>) -> mm_types::Result<Baseline> {
        Ok(Baseline::default())
    }

    fn verify(&self, _cx: &CheckCx<'_>, _baseline: &Baseline) -> Vec<CheckOutcome> {
        Vec::new()
    }
}

#[derive(Default)]
struct NoForge(Mutex<usize>);

impl Forge for NoForge {
    fn open_pr(&self, _pr: &PullRequest<'_>) -> mm_types::Result<String> {
        *self.0.lock().unwrap() += 1;
        Ok("https://example.invalid/pr/1".into())
    }
}

/// アプリと同じく裏の回し手を有効にした窓口と、変化の知らせを受ける口。
fn service(imp: Improver) -> (Arc<Feedback>, Receiver<()>) {
    let svc = Feedback::new(imp.with_background(), json!({ "project": "fixture" }));
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    svc.set_notify(Arc::new(move || {
        let _ = tx.lock().unwrap().send(());
    }));
    (svc, rx)
}

/// 知らせが届くたびに `done` を確かめ、満たしたら返す。
fn wait_for<T>(rx: &Receiver<()>, timeout: Duration, mut done: impl FnMut() -> Option<T>) -> T {
    loop {
        if let Some(v) = done() {
            return v;
        }
        rx.recv_timeout(timeout).expect("知らせが来ない");
    }
}

fn enable(svc: &Feedback, budget: Option<f64>) {
    let patch = SettingsPatch {
        enabled: Some(true),
        budget_usd: budget,
        model: budget.map(|_| "sonnet".to_owned()),
        ..Default::default()
    };
    svc.update_settings(&patch).unwrap();
}

fn submit_typo(svc: &Feedback) -> FeedbackItem {
    let target = json!({ "selector": "section.mm-feedback textarea", "panel": "Feedback" });
    svc.submit(NewFeedback {
        text: "src/greet.txt の \"Helo\" は \"Hello\" の綴りの誤りです。直してください。ほかのファイルは変えないでください。".into(),
        targets: vec![target],
        context: json!({ "viewport": { "width": 322, "height": 800 } }),
    })
    .unwrap()
}

/// オブジェクトのキーの組（配列は最初の要素）。値の違いは見ず、形だけを比べる。
fn shape(v: &Value, prefix: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                let path = format!("{prefix}.{k}");
                out.push(path.clone());
                if k != "context" && k != "targets" {
                    shape(child, &path, out);
                }
            }
        }
        Value::Array(items) => {
            if let Some(first) = items.first() {
                shape(first, &format!("{prefix}[]"), out);
            }
        }
        _ => {}
    }
}

fn shape_of(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    shape(v, "", &mut out);
    out.sort();
    out
}

#[test]
fn 要望を起票すると裏の回が直して返事がパネルの形で付く() {
    let fx = Fixture::new();
    let imp = Improver::new(
        fx.config(),
        Arc::new(FakeClaude),
        Arc::new(PassingChecks),
        Arc::new(NoForge::default()),
    );
    let (svc, rx) = service(imp);
    enable(&svc, None);
    let item = submit_typo(&svc);

    let done = wait_for(&rx, WAIT, || {
        let got = svc.get(&item.id).unwrap();
        (got.status != FeedbackStatus::Pending).then_some(got)
    });

    assert_eq!(done.status, FeedbackStatus::Done, "{done:?}");
    assert_eq!(done.replies[0].text, "greet.txt の綴りを直しました");
    assert_eq!(fx.mainline_file("src/greet.txt"), "Hello, world\n");
    assert_eq!(done.context["app"]["project"], "fixture");
    assert_eq!(done.context["viewport"]["width"], 322);
    let listed = serde_json::to_value(svc.list(ListFilter::All).unwrap()).unwrap();
    let fixture: Value = serde_json::from_str(PANEL_FIXTURE).unwrap();
    assert_eq!(
        shape_of(&listed[0]),
        shape_of(&fixture),
        "パネルの試験の JSON とアプリの JSON の形が揃っている"
    );
    assert_eq!(listed[0]["replies"][0]["outcome"]["kind"], "landed");
    assert!(!svc.status().unwrap().running);
}

#[test]
fn 画面の例外は自動の起票になり一覧に出る() {
    let fx = Fixture::new();
    let imp = Improver::new(
        fx.config(),
        Arc::new(FakeClaude),
        Arc::new(PassingChecks),
        Arc::new(NoForge::default()),
    );
    let (svc, rx) = service(imp);
    let mut input = ErrorInput::error(ErrorSource::Webview, "TypeError: x is undefined");
    input.where_ = Some("panels/story".into());
    svc.report(input);

    let items = wait_for(&rx, WAIT, || {
        let items = svc.list(ListFilter::Pending).unwrap();
        (!items.is_empty()).then_some(items)
    });

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, FeedbackKind::Error);
    assert!(items[0].text.contains("TypeError"), "{}", items[0].text);
    assert!(
        !svc.status().unwrap().settings.enabled,
        "OFF のままなので回は起きない"
    );
}

#[test]
fn panic_の中身を文にする() {
    let s: Box<dyn std::any::Any + Send> = Box::new("固定の文");
    assert_eq!(super::panic_message(s.as_ref()), "固定の文");
    let s: Box<dyn std::any::Any + Send> = Box::new(String::from("作った文 1"));
    assert_eq!(super::panic_message(s.as_ref()), "作った文 1");
    let s: Box<dyn std::any::Any + Send> = Box::new(3_u8);
    assert!(super::panic_message(s.as_ref()).starts_with("panic"));
}

/// 実物の `claude -p` での通し（予算 1 ドル）。費用がかかるので既定では走らせない。
/// `cargo nextest run -p movie-maker --run-ignored only real_claude` で 1 回だけ回す。
#[test]
#[ignore = "実物の claude -p を呼び費用がかかる。手で 1 回だけ走らせる"]
fn real_claude_が要望を直して返事が付く() {
    let fx = Fixture::new();
    let imp = Improver::new(
        fx.config(),
        Arc::new(ClaudeCli::default()),
        Arc::new(PassingChecks),
        Arc::new(NoForge::default()),
    );
    let (svc, rx) = service(imp);
    enable(&svc, Some(1.0));
    let wall = SettingsPatch {
        wall_minutes: Some(REAL_WALL_MINUTES),
        ..Default::default()
    };
    svc.update_settings(&wall).unwrap();
    let item = submit_typo(&svc);

    let done = wait_for(
        &rx,
        Duration::from_secs(u64::from(REAL_WALL_MINUTES + 1) * 60),
        || {
            let got = svc.get(&item.id).unwrap();
            let idle = !svc.status().unwrap().running;
            (idle && (got.status != FeedbackStatus::Pending || !got.replies.is_empty()))
                .then_some(got)
        },
    );

    println!("状態: {:?}\n返事: {:#?}", done.status, done.replies);
    println!("最後の回: {:#?}", svc.status().unwrap().last);
    assert_eq!(done.status, FeedbackStatus::Done);
    assert!(fx.mainline_file("src/greet.txt").starts_with("Hello"));
    assert!(!done.replies[0].text.is_empty());
}
