//! 回の実装を任せる相手。[`Agent`] を差し替えられる層にし、本物は [`ClaudeCli`]（`claude -p`）、試験は決まった差分を書く偽物にする。
//!
//! `claude -p` の区切りは Stop フックで取る。フックが最後の発言と会話ログの場所を回のフォルダーの `stop.json` に書くので、
//! アプリが途中で落ちても、次に起きたときに回を終えられる。

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use mm_types::Result;
use serde::Deserialize;
use serde_json::Value;

use crate::config::Settings;
use crate::files::read_json;
use crate::proc::kill_tree;

/// Stop フックが書くファイル。
pub const STOP_FILE: &str = "stop.json";
/// 回の印。`claude -p` とその子（`mm` など）に回の ID を渡し、費用のかかる操作（`gen.ai` の生成）は回の中では断る。
pub const ROUND_ENV: &str = "MM_IMPROVE_RUN";
/// 回のフォルダーの名前が読めないときの回の印の値。印そのものは必ず付ける。
const UNKNOWN_ROUND: &str = "unknown";
const OUT_FILE: &str = "agent-out.json";
const LOG_FILE: &str = "agent.log";
/// Bash のコマンドを 2 分でバックグラウンドへ移させる。人が見ていない回が、入力を待つコマンドで長く止まらないようにする。
const BASH_TIMEOUT_MS: u64 = 2 * 60 * 1000;
const LOG_TAIL_LINES: usize = 5;
const SECS_PER_MIN: u64 = 60;
const MS_PER_SEC: u64 = 1000;

/// 1 回の呼び出し。
pub struct AgentRequest<'a> {
    /// セッションの ID（UUID）。
    pub session_id: &'a str,
    /// 同じセッションを続けるか（使用制限の後と、印を書かずに止まったとき）。
    pub resume: bool,
    /// 頼む文。
    pub prompt: &'a str,
    /// 作業フォルダー（回の worktree の movie-maker）。
    pub cwd: &'a Path,
    /// 回のフォルダー（`stop.json` とログ）。
    pub run_dir: &'a Path,
    /// 読ませるフォルダー（要望のファイル）。
    pub read_dirs: &'a [PathBuf],
    /// モデル、予算、時間の上限。
    pub settings: &'a Settings,
    /// 子プロセスを起こしたら pid を渡す（すぐ止める手段が使う）。
    pub on_spawn: &'a (dyn Fn(u32) + Sync),
}

/// 1 回の呼び出しの終わり方。
#[derive(Clone, Debug, PartialEq)]
pub enum AgentEnd {
    /// ターンを終えた。
    Finished {
        /// 最後の発言。
        last_message: String,
    },
    /// 使用制限で止まった。明けたら同じセッションで続ける。
    UsageLimit {
        /// 明ける時刻（ミリ秒）。
        resets_at: u64,
    },
    /// 失敗した（終了コード、時間切れ、止められた）。
    Failed {
        /// 理由。
        reason: String,
    },
}

/// 回の実装を任せる相手。
pub trait Agent: Send + Sync {
    /// 1 回の呼び出しを、終わるまで待って返す。
    fn run(&self, req: &AgentRequest<'_>) -> AgentEnd;
}

/// `claude -p` を起こす本物。
#[derive(Clone, Debug)]
pub struct ClaudeCli {
    /// 実行ファイル。
    pub program: PathBuf,
    /// `--permission-mode`。人が見ていない間も止まらないよう、既定は auto に判断させる。
    pub permission_mode: String,
    /// 子に足す環境変数（`CARGO_TARGET_DIR` など）。
    pub env: Vec<(String, String)>,
    /// 足す引数。
    pub extra_args: Vec<String>,
}

impl Default for ClaudeCli {
    fn default() -> Self {
        Self {
            program: PathBuf::from("claude"),
            permission_mode: "auto".into(),
            env: Vec::new(),
            extra_args: Vec::new(),
        }
    }
}

/// Stop フックが標準入力で渡す JSON のうち、使うもの。
#[derive(Debug, Default, Deserialize)]
struct StopHook {
    #[serde(default)]
    transcript_path: Option<PathBuf>,
    #[serde(default)]
    last_assistant_message: Option<String>,
}

/// Stop フックの設定。標準入力の JSON をそのまま `stop.json` に書く。Windows でも Claude Code はフックを Git Bash で走らせる。
pub fn stop_hook_settings(run_dir: &Path) -> String {
    let file = run_dir
        .join(STOP_FILE)
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''");
    serde_json::json!({
        "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": format!("cat > '{file}'") }] }] },
        "env": { "BASH_DEFAULT_TIMEOUT_MS": BASH_TIMEOUT_MS.to_string() },
    })
    .to_string()
}

impl ClaudeCli {
    /// 子に渡す環境変数。足す環境変数に回の印（[`ROUND_ENV`]。値は回のフォルダーの名前 = 回の ID）を重ねる。
    pub fn child_env(&self, req: &AgentRequest<'_>) -> Vec<(String, String)> {
        let round = req.run_dir.file_name().map_or_else(
            || UNKNOWN_ROUND.to_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
        let mut env = self.env.clone();
        env.push((ROUND_ENV.into(), round));
        env
    }

    /// `claude` に渡す引数。頼む文は標準入力で渡す。
    pub fn args(&self, req: &AgentRequest<'_>) -> Vec<String> {
        let s = req.settings;
        let session = if req.resume {
            "--resume"
        } else {
            "--session-id"
        };
        let mut args: Vec<String> = ["-p", session, req.session_id, "--output-format", "json"]
            .map(str::to_owned)
            .to_vec();
        args.extend(["--permission-mode".into(), self.permission_mode.clone()]);
        // 確認の要る操作は誰も答えないので断る。
        args.extend(["--permission-prompts".into(), "none".into()]);
        args.extend(["--settings".into(), stop_hook_settings(req.run_dir)]);
        for d in req.read_dirs {
            args.extend(["--add-dir".into(), d.to_string_lossy().into_owned()]);
        }
        let opt = |flag: &str, v: Option<String>| v.map(|v| [flag.to_owned(), v]);
        args.extend(opt("--model", s.model.clone()).into_iter().flatten());
        args.extend(opt("--effort", s.effort.clone()).into_iter().flatten());
        args.extend(
            opt("--max-budget-usd", s.budget_usd.map(|b| b.to_string()))
                .into_iter()
                .flatten(),
        );
        args.extend(self.extra_args.iter().cloned());
        args
    }

    fn spawn(&self, req: &AgentRequest<'_>) -> Result<Child> {
        std::fs::create_dir_all(req.run_dir)?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(req.run_dir.join(LOG_FILE))?;
        let out = File::create(req.run_dir.join(OUT_FILE))?;
        let mut child = Command::new(&self.program)
            .args(self.args(req))
            .current_dir(req.cwd)
            .envs(self.child_env(req))
            .stdin(Stdio::piped())
            .stdout(out)
            .stderr(log)
            .spawn()?;
        (req.on_spawn)(child.id());
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(req.prompt.as_bytes())?;
        }
        Ok(child)
    }

    fn try_run(&self, req: &AgentRequest<'_>) -> Result<AgentEnd> {
        let stop_path = req.run_dir.join(STOP_FILE);
        if stop_path.exists() {
            std::fs::remove_file(&stop_path)?;
        }
        let mut child = self.spawn(req)?;
        let limit = Duration::from_secs(u64::from(req.settings.wall_minutes) * SECS_PER_MIN);
        let (status, timed_out) = wait_with_deadline(&mut child, limit)?;
        let stop: StopHook = read_json(&stop_path).ok().flatten().unwrap_or_default();
        let transcript = stop
            .transcript_path
            .clone()
            .or_else(|| find_transcript(req.session_id));
        if let Some(resets_at) = transcript.as_deref().and_then(usage_limit) {
            return Ok(AgentEnd::UsageLimit { resets_at });
        }
        if timed_out {
            let reason = format!(
                "回の時間の上限 {} 分を超えたので止めた",
                req.settings.wall_minutes
            );
            return Ok(AgentEnd::Failed { reason });
        }
        Ok(finish(req.run_dir, status, stop.last_assistant_message))
    }
}

impl Agent for ClaudeCli {
    fn run(&self, req: &AgentRequest<'_>) -> AgentEnd {
        self.try_run(req).unwrap_or_else(|e| AgentEnd::Failed {
            reason: format!("claude -p を起こせない: {e}"),
        })
    }
}

/// 終了の状態と出力から終わり方を決める。Stop の時点の最後の発言を優先し、無ければ `--output-format json` の `result` を使う。
fn finish(run_dir: &Path, status: ExitStatus, hook_message: Option<String>) -> AgentEnd {
    let out: Value = read_json(&run_dir.join(OUT_FILE))
        .ok()
        .flatten()
        .unwrap_or(Value::Null);
    if out["is_error"].as_bool() == Some(true) {
        let reason = format!(
            "claude -p のエラー（{}）: {}",
            out["subtype"], out["result"]
        );
        return AgentEnd::Failed { reason };
    }
    let message = hook_message
        .filter(|m| !m.trim().is_empty())
        .or_else(|| out["result"].as_str().map(str::to_owned));
    match message {
        Some(last_message) if status.success() || !last_message.is_empty() => {
            AgentEnd::Finished { last_message }
        }
        _ => {
            let log = std::fs::read_to_string(run_dir.join(LOG_FILE)).unwrap_or_default();
            let tail: Vec<&str> = log.lines().rev().take(LOG_TAIL_LINES).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            AgentEnd::Failed {
                reason: format!("claude -p が {status} で止まった: {}", tail.join(" / ")),
            }
        }
    }
}

/// 終わるまで待つ。`limit` を過ぎたら木ごと止める。見張りはタイマー 1 つで、終わったら起こして畳む。
fn wait_with_deadline(child: &mut Child, limit: Duration) -> Result<(ExitStatus, bool)> {
    let (tx, rx) = mpsc::channel::<()>();
    let pid = child.id();
    let guard = std::thread::spawn(move || match rx.recv_timeout(limit) {
        Err(RecvTimeoutError::Timeout) => {
            kill_tree(pid);
            true
        }
        _ => false,
    });
    let status = child.wait();
    drop(tx);
    let timed_out = guard.join().unwrap_or(false);
    Ok((status?, timed_out))
}

/// Stop フックが残した最後の発言。回を動かしていたプロセスが落ちた後に、回を区切れるかを見るのに使う。
pub fn stop_message(run_dir: &Path) -> Option<String> {
    let stop: StopHook = read_json(&run_dir.join(STOP_FILE)).ok().flatten()?;
    stop.last_assistant_message.filter(|m| !m.trim().is_empty())
}

/// 会話ログの場所。Stop フックが動かずに止まったとき（使用制限など）に使う。
pub fn find_transcript(session_id: &str) -> Option<PathBuf> {
    let base = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|h| PathBuf::from(h).join(".claude"))
        })?;
    let file = format!("{session_id}.jsonl");
    std::fs::read_dir(base.join("projects"))
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path().join(&file))
        .find(|p| p.exists())
}

/// 会話ログが使用制限で終わっていれば、明ける時刻（ミリ秒）。
/// Claude Code は断られたときに合成の発言を足し、`apiError` と `apiErrorParams.rate_limit_info.resetsAt`（秒）を添える。
pub fn usage_limit(transcript: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(transcript).ok()?;
    let mut limit = None;
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["type"] != "assistant" {
            continue;
        }
        limit = (v["apiError"] == "usage_limit_reached")
            .then(|| v["apiErrorParams"]["rate_limit_info"]["resetsAt"].as_u64())
            .flatten()
            .map(|s| s * MS_PER_SEC);
    }
    limit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_limit_reads_last_assistant_entry() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.jsonl");
        let limited = r#"{"type":"assistant","apiError":"usage_limit_reached","apiErrorParams":{"rate_limit_info":{"resetsAt":1800000000}}}"#;
        std::fs::write(&p, format!("{{\"type\":\"user\"}}\n{limited}\n")).unwrap();
        assert_eq!(usage_limit(&p), Some(1_800_000_000_000));
        std::fs::write(&p, format!("{limited}\n{{\"type\":\"assistant\"}}\n")).unwrap();
        assert_eq!(usage_limit(&p), None);
    }

    #[test]
    fn stop_hook_writes_to_run_dir_with_forward_slashes() {
        let s = stop_hook_settings(Path::new(r"C:\data\runs\r1"));
        assert!(s.contains("cat > 'C:/data/runs/r1/stop.json'"), "{s}");
    }

    #[test]
    fn 回の_claude_には回の印を渡す() {
        let cli = ClaudeCli {
            env: vec![("CARGO_TARGET_DIR".into(), "t".into())],
            ..ClaudeCli::default()
        };
        let settings = Settings::default();
        let req = AgentRequest {
            session_id: "s",
            resume: false,
            prompt: "",
            cwd: Path::new("."),
            run_dir: Path::new("runs/r42"),
            read_dirs: &[],
            settings: &settings,
            on_spawn: &|_| {},
        };
        let env = cli.child_env(&req);
        assert!(env.contains(&("CARGO_TARGET_DIR".into(), "t".into())));
        assert!(env.contains(&(ROUND_ENV.into(), "r42".into())), "{env:?}");
    }
}
