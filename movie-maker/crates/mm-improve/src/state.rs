//! 回の状態（`state.json`）と、回を 1 つに限るロック（`run.lock`）。
//! ロックは OS のファイルのロックなので、回を動かしていたプロセスがクラッシュすれば外れる。状態は残るので、次に起きたプロセスが続きから回す。

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use mm_types::Result;
use serde::{Deserialize, Serialize};

use crate::checks::CheckOutcome;

/// 回の工程。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// `claude -p` が実装している。
    #[default]
    Agent,
    /// 囲いの検査と取り込み。
    Gate,
}

/// 動いている回。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunState {
    /// 回の ID（時刻から始まる）。
    pub id: String,
    /// `claude -p` のセッションの ID。
    pub session_id: String,
    /// 回のブランチ。
    pub branch: String,
    /// 回の worktree のルート。
    pub worktree: PathBuf,
    /// 回を切った本線のコミット。
    pub base_commit: String,
    /// 回に渡した要望の ID。
    pub items: Vec<String>,
    /// 始めた時刻（ミリ秒）。
    pub started_at: u64,
    /// 工程。
    #[serde(default)]
    pub phase: Phase,
    /// `claude -p` を呼んだ回数。1 回目は新しいセッション、2 回目からは同じセッションを続ける。
    #[serde(default)]
    pub calls: u32,
    /// 完了のマーカーを書かずに止まったときに続きを頼んだ回数。
    #[serde(default)]
    pub nudges: u32,
    /// `claude -p` の pid（すぐ止める手段が使う）。
    #[serde(default)]
    pub agent_pid: Option<u32>,
    /// 使用制限が明ける時刻（ミリ秒）。
    #[serde(default)]
    pub limit_resets_at: Option<u64>,
    /// 最後の発言（区切った後の検査で使う）。
    #[serde(default)]
    pub last_message: Option<String>,
    /// 回を起こしたときの予算（ドル）。回の途中で設定のファイルを書き換えても、この回の上限は変わらない。
    #[serde(default)]
    pub budget_usd: Option<f64>,
    /// 回を起こしたときの時間の上限（ミリ秒）。
    #[serde(default)]
    pub wall_ms: Option<u64>,
    /// `claude -p` のセッションの累計の費用（ドル）。続きの呼び出しには予算の残りだけを渡す。
    #[serde(default)]
    pub spent_usd: f64,
    /// `claude -p` を動かした時間の累計（ミリ秒）。使用制限で待った時間は数えない。
    #[serde(default)]
    pub agent_ms: u64,
}

/// 回の結末。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunOutcome {
    /// 本線に取り込んだ。
    Landed {
        /// 取り込んだコミット。
        commit: String,
    },
    /// コードを変えずに返事だけした。
    NoChange,
    /// PR までで止めた。
    PullRequest {
        /// PR の URL。
        url: String,
        /// 止めた理由。
        reason: String,
    },
    /// 囲いの検査に通らなかった。
    Rejected {
        /// 通らなかった理由。
        reason: String,
    },
    /// 済んだ要望を書かずに止まったか、失敗した。
    Failed {
        /// 理由。
        reason: String,
    },
    /// 人が止めた。
    Stopped,
}

/// 終えた回の記録（`runs/<ID>/report.json` と状態の `last`）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    /// 回の ID。
    pub run_id: String,
    /// 結末。
    pub outcome: RunOutcome,
    /// 回に渡した要望。
    pub items: Vec<String>,
    /// 済んだ要望。
    pub done: Vec<String>,
    /// 変えたファイル（movie-maker からの相対）。
    pub changed: Vec<String>,
    /// 検査の結果。
    pub checks: Vec<CheckOutcome>,
    /// 終えた時刻（ミリ秒）。
    pub finished_at: u64,
}

/// `state.json`。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImproveState {
    /// 動いている回（使用制限で止まっているものも含む）。
    pub run: Option<RunState>,
    /// 待機の理由。あれば新しい回を起こさない。新しい要望か、ON にし直すと消える。
    pub waiting: Option<String>,
    /// 最後に取り込んだ回の ID と時刻。直後のエラーをその回の疑いとして結びつける。
    pub last_landed: Option<(String, u64)>,
    /// 最後に終えた回。
    pub last: Option<RunReport>,
}

/// 回のロック。持っている間だけ回を動かせる。破棄すると外れる。
pub struct RunLock {
    _file: File,
}

impl RunLock {
    /// 取れたら `Some`。ほかのプロセスかスレッドが回を動かしていれば `None`。
    pub fn try_acquire(path: &Path) -> Result<Option<Self>> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(e)) => Err(e.into()),
        }
    }
}
