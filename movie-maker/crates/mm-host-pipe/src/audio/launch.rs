//! アプリ側の起こし方。ホストの実行ファイルを探し、`<ホスト> describe` を 1 ファイル 1 プロセスで実行する。
//! `serve` は [`super::AudioEffect`] が [`crate::PipeHost`] で起こす。
//!
//! 音のホストは共有テクスチャを返さないので、一時フォルダーを作らず、期限切れの応答に共有テクスチャが
//! 入っていたら閉じずに漏れとして数える（各規格の `PIPE`）。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mm_types::{MmError, Result};

use super::desc::BundleDesc;
use super::PluginStandard;
use crate::{kill_tree, Job};

/// 1 つのプラグインのファイルを describe して JSON を 1 行で出すサブコマンド。
pub const DESCRIBE_COMMAND: &str = "describe";
/// describe の出力の上限。壊れたプラグインが出力を止めなくても、このプロセスのメモリを使い切らない。
const DESCRIBE_LIMIT: u64 = 16 * 1024 * 1024;
/// describe の出力が閉じてからプロセスが終わるまでの期限。
const EXIT_WAIT: Duration = Duration::from_secs(2);
/// 終わったかを見直す間隔。
const EXIT_STEP: Duration = Duration::from_millis(10);

/// 起こすホストの実行ファイル。`S::HOST_ENV` があればそれ、無ければ今の実行ファイルと同じフォルダー
/// （テストの実行ファイルなら 1 つ上も）の `S::HOST_STEM`。
pub fn find_host<S: PluginStandard>() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(S::HOST_ENV) {
        return Some(PathBuf::from(path)).filter(|p| p.is_file());
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = format!("{}{}", S::HOST_STEM, std::env::consts::EXE_SUFFIX);
    let found = [Some(dir), dir.parent()]
        .into_iter()
        .flatten()
        .map(|d: &Path| d.join(&name))
        .find(|p| p.is_file());
    found
}

/// `executable describe <path>` を別プロセスで実行し、ファイルの記述を受け取る。
/// プラグインの describe がクラッシュしても、このプロセスには `HostCrashed` で返る。
pub fn describe_bundle<S: PluginStandard>(
    executable: &Path,
    path: &Path,
    timeout: Duration,
) -> Result<BundleDesc> {
    let what = |step: &str| format!("{}.{step}", S::PIPE.name);
    let mut child = Command::new(executable)
        .arg(DESCRIBE_COMMAND)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            MmError::HostCrashed(format!("{} を起動できない: {e}", executable.display()))
        })?;
    // プラグインが起こしたプロセスも、時間切れのときに一緒に止める。
    let job = Job::attach(&child);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| MmError::Internal("子プロセスのパイプが無い".into()))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let read = stdout.take(DESCRIBE_LIMIT).read_to_string(&mut text);
        // 受け取る側が時間切れで先に戻っていれば、読んだ内容は要らない。
        let _ = tx.send(read.map(|_| text));
    });
    let Ok(read) = rx.recv_timeout(timeout) else {
        kill_tree(&mut child, job.as_ref())?;
        return Err(MmError::Timeout {
            what: what("describe"),
            limit: timeout,
        });
    };
    let text = read?;
    let Some(status) = wait_exit(&mut child, EXIT_WAIT)? else {
        kill_tree(&mut child, job.as_ref())?;
        return Err(MmError::Timeout {
            what: what("describe_exit"),
            limit: EXIT_WAIT,
        });
    };
    if !status.success() {
        return Err(MmError::HostCrashed(format!(
            "{} の describe が {status} で終わった",
            path.display()
        )));
    }
    Ok(serde_json::from_str(text.trim())?)
}

/// `limit` まで終わるのを待つ。終わらなければ `None`。
fn wait_exit(child: &mut Child, limit: Duration) -> Result<Option<ExitStatus>> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if start.elapsed() >= limit {
            return Ok(None);
        }
        std::thread::sleep(EXIT_STEP);
    }
}
