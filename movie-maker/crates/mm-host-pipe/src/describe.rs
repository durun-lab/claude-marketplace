//! `<ホスト> describe <パス>` を 1 ファイル 1 プロセスで実行し、1 行の JSON を受け取る（`mm-host-ofx`、`mm-host-clap`、
//! `mm-host-vst3` で共通）。プラグインの describe がクラッシュしても、このプロセスには `HostCrashed` で返る。

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use mm_types::{MmError, Result};
use serde::de::DeserializeOwned;

use crate::job::{kill_tree, wait_exit, Job};

/// 1 つのプラグインのファイルを describe して JSON を 1 行で出すサブコマンド。
pub const DESCRIBE_COMMAND: &str = "describe";
/// describe の出力の上限。壊れたプラグインが出力を止めなくても、このプロセスのメモリを使い切らない。
const DESCRIBE_LIMIT: u64 = 16 * 1024 * 1024;
/// describe の出力が閉じてからプロセスが終わるまでの期限。
const EXIT_WAIT: Duration = Duration::from_secs(2);

/// `executable describe <path>` を別プロセスで実行し、出力の JSON を `T` で読む。
/// `name` は時間切れの `what` の頭（`host_clap` なら `host_clap.describe`）。
pub fn run_describe<T: DeserializeOwned>(
    executable: &Path,
    path: &Path,
    timeout: Duration,
    name: &str,
) -> Result<T> {
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
        return Err(timed_out(
            &mut child,
            job.as_ref(),
            name,
            "describe",
            timeout,
        ));
    };
    let text = read?;
    // 出力を閉じてから終わらないプロセスでも、ここで待ち続けない。
    let Some(status) = wait_exit(&mut child, EXIT_WAIT)? else {
        return Err(timed_out(
            &mut child,
            job.as_ref(),
            name,
            "describe_exit",
            EXIT_WAIT,
        ));
    };
    if !status.success() {
        return Err(MmError::HostCrashed(format!(
            "{} の describe が {status} で終わった",
            path.display()
        )));
    }
    Ok(serde_json::from_str(text.trim())?)
}

/// 時間切れの子を止めて `Timeout` を作る。止まらないプロセスは捨て、時間切れを返すのを優先する。
fn timed_out(
    child: &mut Child,
    job: Option<&Job>,
    name: &str,
    step: &str,
    limit: Duration,
) -> MmError {
    if let Err(e) = kill_tree(child, job) {
        tracing::warn!(%e, "{name} の describe のプロセスを止められない");
    }
    MmError::Timeout {
        what: format!("{name}.{step}"),
        limit,
    }
}
