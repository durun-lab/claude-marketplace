//! 回の実装を任せる相手。[`Agent`] を差し替えられる層にし、実際の実装は [`ClaudeCli`]（`claude -p`）、テストは決まった差分を書くモックにする。
//!
//! `claude -p` の区切りは Stop フックで取る。フックが最後の発言と会話ログの場所を回のフォルダーの `stop.json` に書くので、
//! アプリが途中でクラッシュしても、次に起きたときに回を終えられる。

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use mm_types::Result;
use serde::Deserialize;
use serde_json::Value;

use crate::config::Settings;
use crate::files::read_json;
use crate::job::Job;
use crate::proc::kill_tree;
use crate::time::now_ms;
use crate::write_guard::WriteGuard;

/// Stop フックが書くファイル。
pub const STOP_FILE: &str = "stop.json";
/// 回の環境変数。`claude -p` とその子（`mm` など）に回の ID を渡し、費用のかかる操作（`gen.ai` の生成）は回の中では拒否する。
pub const ROUND_ENV: &str = "MM_IMPROVE_RUN";
/// 回のフォルダーの名前が読めないときの回の環境変数の値。環境変数そのものは必ず付ける。
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
    /// 同じセッションを続けるか（使用制限の後と、完了のマーカーを書かずに止まったとき）。
    pub resume: bool,
    /// 頼む文。
    pub prompt: &'a str,
    /// 作業フォルダー（回の worktree の movie-maker）。
    pub cwd: &'a Path,
    /// 回のフォルダー（`stop.json` とログ）。
    pub run_dir: &'a Path,
    /// 書き込みの囲い（読むだけの要望のファイルと、書かせない所）。
    pub guard: &'a WriteGuard,
    /// モデル、effort、予算（回の残り）。
    pub settings: &'a Settings,
    /// この呼び出しの時間の上限（回の残り）。過ぎたら木ごと止める。
    pub wall: Duration,
    /// 子プロセスを起こしたら pid を渡す（すぐ止める手段が使う）。
    pub on_spawn: &'a (dyn Fn(u32) + Sync),
    /// セッションの累計の費用（ドル）が分かったら渡す。回は予算の残りを次の呼び出しに渡す。
    pub on_cost: &'a (dyn Fn(f64) + Sync),
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

/// `claude -p` を起こす実際の実装。
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

/// 回の `claude -p` に拒否させるコマンド。回のブランチ以外の参照（本線の `local/movie-maker` など）を動かすと
/// 囲いの検査を通らずに取り込まれるので、ブランチを切り替える・参照を書く・遠隔へ出す操作を止める。
/// 確認の要る操作は `--permission-prompts none` で拒否されるが、auto のモードが通しうるものをここで決まった形で拒否する。
pub const DENIED_COMMANDS: &[&str] = &[
    "Bash(gh:*)",
    "Bash(git -C:*)",
    "Bash(git branch:*)",
    "Bash(git checkout:*)",
    "Bash(git push:*)",
    "Bash(git switch:*)",
    "Bash(git symbolic-ref:*)",
    "Bash(git tag:*)",
    "Bash(git update-ref:*)",
    "Bash(git worktree:*)",
];

/// 回の `claude -p` の設定。Stop フックは標準入力の JSON をそのまま `stop.json` に書く
/// （Windows でも Claude Code はフックを Git Bash で実行する）。[`DENIED_COMMANDS`] と `guard` の書かせない所を拒否させ、
/// 作業フォルダーを worktree だけにして、Write と Edit が外を書くのをどのモードでも拒否させる（[`WriteGuard`]）。
pub fn run_settings(run_dir: &Path, guard: &WriteGuard) -> String {
    let file = run_dir
        .join(STOP_FILE)
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''");
    let (allow, no_write) = guard.rules();
    let deny: Vec<String> = DENIED_COMMANDS
        .iter()
        .map(|r| (*r).to_owned())
        .chain(no_write)
        .collect();
    serde_json::json!({
        "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": format!("cat > '{file}'") }] }] },
        "env": { "BASH_DEFAULT_TIMEOUT_MS": BASH_TIMEOUT_MS.to_string() },
        "permissions": {
            "allow": allow,
            "deny": deny,
            "additionalDirectories": [],
            "blockWritesOutsideWorkingDirectories": true,
        },
    })
    .to_string()
}

impl ClaudeCli {
    /// 子に渡す環境変数。足す環境変数に回の環境変数（[`ROUND_ENV`]。値は回のフォルダーの名前 = 回の ID）を重ねる。
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
        // 確認の要る操作は誰も答えないので拒否する。
        args.extend(["--permission-prompts".into(), "none".into()]);
        args.extend(["--settings".into(), run_settings(req.run_dir, req.guard)]);
        // 人の設定（ほかのリポジトリの additionalDirectories、広い allow、プラグイン）と MCP のサーバーを読ませない。
        // 設定の一覧は層をまたいで足し合わされ、--settings では消せないため、読む元から外す。
        args.extend(["--setting-sources", "project", "--strict-mcp-config"].map(str::to_owned));
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

    /// 起こしてすぐジョブに入れる。頼む文を書く前に入れ、claude が子を起こす前に木をジョブに収める。
    fn spawn(&self, req: &AgentRequest<'_>) -> Result<(Child, Option<Job>)> {
        std::fs::create_dir_all(req.run_dir)?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(req.run_dir.join(LOG_FILE))?;
        let out = File::create(req.run_dir.join(OUT_FILE))?;
        let mut child = mm_types::process::command(&self.program)
            .args(self.args(req))
            .current_dir(req.cwd)
            .envs(self.child_env(req))
            .stdin(Stdio::piped())
            .stdout(out)
            .stderr(log)
            .spawn()?;
        let job = Job::attach(&child);
        (req.on_spawn)(child.id());
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(req.prompt.as_bytes())?;
        }
        Ok((child, job))
    }

    fn try_run(&self, req: &AgentRequest<'_>) -> Result<AgentEnd> {
        let stop_path = req.run_dir.join(STOP_FILE);
        if stop_path.exists() {
            std::fs::remove_file(&stop_path)?;
        }
        let started = now_ms();
        let (mut child, job) = self.spawn(req)?;
        let (status, timed_out) = wait_with_deadline(&mut child, job.as_ref(), req.wall)?;
        // claude が後ろに回したコマンドが残っていれば、ジョブを閉じて止める。
        drop(job);
        let out: Value = read_json(&req.run_dir.join(OUT_FILE))
            .ok()
            .flatten()
            .unwrap_or(Value::Null);
        if let Some(total) = out["total_cost_usd"].as_f64() {
            (req.on_cost)(total);
        }
        let stop: StopHook = read_json(&stop_path).ok().flatten().unwrap_or_default();
        let transcript = stop
            .transcript_path
            .clone()
            .or_else(|| find_transcript(req.session_id));
        // 前の呼び出しの使用制限が会話ログの最後に残っていても、明ける時刻はこの呼び出しより前なので数えない。
        let limit = transcript.as_deref().and_then(usage_limit);
        if let Some(resets_at) = limit.filter(|t| *t > started) {
            return Ok(AgentEnd::UsageLimit { resets_at });
        }
        if timed_out {
            let reason = format!(
                "回の時間の上限を超えたので止めた（この呼び出しの残り {} 分）",
                req.wall.as_secs() / SECS_PER_MIN
            );
            return Ok(AgentEnd::Failed { reason });
        }
        Ok(finish(
            req.run_dir,
            &out,
            status,
            stop.last_assistant_message,
        ))
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
fn finish(
    run_dir: &Path,
    out: &Value,
    status: ExitStatus,
    hook_message: Option<String>,
) -> AgentEnd {
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

/// 終わるまで待つ。`limit` を過ぎたら木ごと止める（ジョブがあればジョブで、無ければ taskkill で）。
/// 見張りはタイマー 1 つで、終わったら起こして片付ける。
fn wait_with_deadline(
    child: &mut Child,
    job: Option<&Job>,
    limit: Duration,
) -> Result<(ExitStatus, bool)> {
    let (tx, rx) = mpsc::channel::<()>();
    let pid = child.id();
    std::thread::scope(|s| {
        let guard = s.spawn(move || match rx.recv_timeout(limit) {
            Err(RecvTimeoutError::Timeout) => {
                match job {
                    Some(job) => job.terminate(),
                    None => kill_tree(pid),
                }
                true
            }
            _ => false,
        });
        let status = child.wait();
        drop(tx);
        let timed_out = guard.join().unwrap_or(false);
        Ok((status?, timed_out))
    })
}

/// Stop フックが残した最後の発言。回を動かしていたプロセスがクラッシュした後に、回を区切れるかを見るのに使う。
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
/// Claude Code は拒否されたときに合成の発言を足し、`apiError` と `apiErrorParams.rate_limit_info.resetsAt`（秒）を添える。
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
        let s = run_settings(Path::new(r"C:\data\runs\r1"), &WriteGuard::default());
        assert!(s.contains("cat > 'C:/data/runs/r1/stop.json'"), "{s}");
    }

    fn strings(v: &Value) -> Vec<&str> {
        v.as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect()
    }

    #[test]
    fn run_settings_deny_moving_other_refs() {
        let v: Value =
            serde_json::from_str(&run_settings(Path::new("/r"), &WriteGuard::default())).unwrap();
        let deny = strings(&v["permissions"]["deny"]);
        for rule in ["Bash(git push:*)", "Bash(git update-ref:*)", "Bash(gh:*)"] {
            assert!(deny.contains(&rule), "{rule}");
        }
    }

    #[test]
    fn run_settings_block_writes_outside_the_worktree() {
        let guard = WriteGuard::for_run(
            Path::new(r"C:\mm"),
            Path::new(r"C:\mm\feedback"),
            Path::new(r"E:\repo"),
            Path::new(r"C:\mm\worktrees\1"),
        );
        let v: Value = serde_json::from_str(&run_settings(Path::new("/r"), &guard)).unwrap();
        let p = &v["permissions"];
        assert_eq!(p["blockWritesOutsideWorkingDirectories"], true);
        assert_eq!(p["additionalDirectories"], serde_json::json!([]));
        assert!(strings(&p["allow"]).contains(&"Read(//c/mm/feedback/**)"));
        let deny = strings(&p["deny"]);
        for rule in [
            "Edit(//c/mm/feedback/**)",
            "Edit(//e/repo/**)",
            "Edit(//c/mm/*.json)",
        ] {
            assert!(deny.contains(&rule), "{rule}: {deny:?}");
        }
    }

    #[test]
    fn args_leave_out_add_dir_and_human_settings() {
        let settings = Settings::default();
        let guard = WriteGuard::default();
        let req = AgentRequest {
            session_id: "s",
            resume: false,
            prompt: "",
            cwd: Path::new("/w"),
            run_dir: Path::new("/r"),
            guard: &guard,
            settings: &settings,
            wall: Duration::ZERO,
            on_spawn: &|_| {},
            on_cost: &|_| {},
        };
        let args = ClaudeCli::default().args(&req);
        assert!(!args.iter().any(|a| a == "--add-dir"), "{args:?}");
        let at = args.iter().position(|a| a == "--setting-sources").unwrap();
        assert_eq!(args[at + 1], "project");
        assert!(args.iter().any(|a| a == "--strict-mcp-config"));
    }

    #[test]
    fn 回の_claude_には回の環境変数を渡す() {
        let cli = ClaudeCli {
            env: vec![("CARGO_TARGET_DIR".into(), "t".into())],
            ..ClaudeCli::default()
        };
        let settings = Settings::default();
        let guard = WriteGuard::default();
        let req = AgentRequest {
            session_id: "s",
            resume: false,
            prompt: "",
            cwd: Path::new("."),
            run_dir: Path::new("runs/r42"),
            guard: &guard,
            settings: &settings,
            wall: Duration::from_secs(1),
            on_spawn: &|_| {},
            on_cost: &|_| {},
        };
        let env = cli.child_env(&req);
        assert!(env.contains(&("CARGO_TARGET_DIR".into(), "t".into())));
        assert!(env.contains(&(ROUND_ENV.into(), "r42".into())), "{env:?}");
    }
}
