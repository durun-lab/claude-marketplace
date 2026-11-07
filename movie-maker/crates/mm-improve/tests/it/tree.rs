//! `claude -p` のプロセスの木が残らないことの確かめ。プロセスの木を作る偽の claude（examples/fake_claude.rs）を使う。
//! アプリが落ちたとき（ジョブのハンドルが OS に閉じられる）と、回の呼び出しが終わったとき（後ろに回したコマンドが残る）。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mm_improve::{
    Agent, AgentEnd, AgentRequest, ClaudeCli, NewFeedback, Settings, Step, WriteGuard,
};

use crate::support::{FakeAgent, FakeChecks, Fixture, Script};

/// 木ができるまで待つ上限。host は worktree を作ってから偽の claude を起こす。
const SPAWN_LIMIT: Duration = Duration::from_secs(60);
/// 止めた木が消えるまで待つ上限。
const EXIT_LIMIT: Duration = Duration::from_secs(15);
const TICK: Duration = Duration::from_millis(100);

/// cargo が試験と一緒にビルドする偽の claude。試験の実行ファイル（target/<profile>/deps/）の隣の examples/ にある。
fn fake_claude() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().and_then(Path::parent).unwrap();
    let path = dir
        .join("examples")
        .join(format!("fake_claude{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.exists(),
        "偽の claude がビルドされていない: {}",
        path.display()
    );
    path
}

/// 偽の claude が書いた自分と孫の pid。書かれるまで待つ。
fn wait_pids(path: &Path) -> Vec<u32> {
    let start = Instant::now();
    while start.elapsed() < SPAWN_LIMIT {
        if let Ok(text) = std::fs::read_to_string(path) {
            return text
                .split_whitespace()
                .map(|p| p.parse().unwrap())
                .collect();
        }
        std::thread::sleep(TICK);
    }
    panic!("偽の claude が起きない");
}

fn alive(pid: u32) -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
}

/// 全部が消えるまで待ち、残った pid を返す。残ったものは後の試験のために止める。
fn survivors(pids: &[u32]) -> Vec<u32> {
    let start = Instant::now();
    while start.elapsed() < EXIT_LIMIT && pids.iter().any(|p| alive(*p)) {
        std::thread::sleep(TICK);
    }
    let left: Vec<u32> = pids.iter().copied().filter(|p| alive(*p)).collect();
    for p in &left {
        drop(
            Command::new("taskkill")
                .args(["/PID", &p.to_string(), "/T", "/F"])
                .output(),
        );
    }
    left
}

#[test]
fn app_crash_takes_the_claude_tree_down() {
    let fx = Fixture::new();
    let resumed = FakeAgent::new(vec![Script::Say("続けた")]);
    let imp = fx.improver(resumed.clone(), FakeChecks::passing(), Arc::default());
    imp.submit(NewFeedback {
        text: "木を作る".into(),
        ..Default::default()
    })
    .unwrap();
    let pids_file = fx.data.join("pids.txt");
    let mut host = Command::new(fake_claude())
        .arg("host")
        .arg(&fx.data)
        .arg(&fx.repo)
        .env("MM_FAKE_CLAUDE_PIDS", &pids_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let pids = wait_pids(&pids_file);
    // TerminateProcess はアプリのクラッシュと同じく、後片付けのコードを走らせずにハンドルを閉じる。
    host.kill().unwrap();
    host.wait().unwrap();

    assert_eq!(survivors(&pids), Vec::<u32>::new(), "claude と孫が残った");
    let step = imp.run_once().unwrap();
    assert!(matches!(step, Step::Finished(_)), "{step:?}");
    assert!(resumed.calls()[0].0, "同じセッションを続ける");
}

#[test]
fn background_children_do_not_outlive_the_call() {
    let dir = tempfile::tempdir().unwrap();
    let pids_file = dir.path().join("pids.txt");
    let agent = ClaudeCli {
        program: fake_claude(),
        env: vec![
            (
                "MM_FAKE_CLAUDE_PIDS".into(),
                pids_file.to_string_lossy().into_owned(),
            ),
            ("MM_FAKE_CLAUDE_EXIT".into(), "1".into()),
        ],
        ..ClaudeCli::default()
    };
    let settings = Settings::default();
    let guard = WriteGuard::default();
    let req = AgentRequest {
        session_id: "00000000-0000-4000-8000-000000000000",
        resume: false,
        prompt: "木を作って終わる",
        cwd: dir.path(),
        run_dir: &dir.path().join("run"),
        guard: &guard,
        settings: &settings,
        wall: SPAWN_LIMIT,
        on_spawn: &|_| {},
        on_cost: &|_| {},
    };

    let end = agent.run(&req);

    assert!(matches!(end, AgentEnd::Finished { .. }), "{end:?}");
    let pids = wait_pids(&pids_file);
    assert_eq!(survivors(&pids[1..]), Vec::<u32>::new(), "孫が残った");
}
