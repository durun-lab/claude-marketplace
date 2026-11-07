//! アプリ側のランチャー。`mm-host-ofx serve` を `mm_host_pipe::PipeHost` で起こす。
//!
//! このホストは CPU の画像だけを返すので、一時フォルダーを作らず、期限切れの応答に共有テクスチャが
//! 入っていたら閉じずに漏れとして数える。

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mm_host_api::{HostLauncher, HostProcess, HostSpec};
use mm_host_pipe::{kill_tree, Job, PipeHost, PipeKind, StaleHandles};
use mm_types::{MmError, Result};

use crate::desc::BundleDesc;

/// 1 つのバンドルを describe して JSON を 1 行で出すサブコマンド。
pub const DESCRIBE_COMMAND: &str = "describe";
/// 起動直後の `Ping` の期限。プラグインはまだ読み込まないので短くてよい。
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
/// describe の出力の上限。壊れたプラグインが出力を止めなくても、このプロセスのメモリを使い切らない。
const DESCRIBE_LIMIT: u64 = 16 * 1024 * 1024;
/// describe の出力が閉じてからプロセスが終わるまでの期限。
const EXIT_WAIT: Duration = Duration::from_secs(2);
/// 終わったかを見直す間隔。
const EXIT_STEP: Duration = Duration::from_millis(10);

/// `mm-host-ofx` の起こし方。
const KIND: PipeKind = PipeKind {
    name: "host_ofx",
    startup_timeout: STARTUP_TIMEOUT,
    work_dir_env: None,
    stale_handles: StaleHandles::Count,
    configure: |_| {},
};

/// `mm-host-ofx` を起こすランチャー。
#[derive(Debug, Clone, Default)]
pub struct OfxLauncher;

impl OfxLauncher {
    /// 起動して `Ping` が通るまで待つ。
    pub fn start(spec: HostSpec) -> Result<PipeHost> {
        PipeHost::start(KIND, spec)
    }
}

impl HostLauncher for OfxLauncher {
    fn id(&self) -> &'static str {
        "ofx"
    }

    fn spawn(&self, spec: &HostSpec) -> Result<Box<dyn HostProcess>> {
        Ok(Box::new(Self::start(spec.clone())?))
    }
}

/// `executable describe <bundle>` を別プロセスで走らせ、バンドルの記述を受け取る。
/// プラグインの describe がクラッシュしても、このプロセスには `HostCrashed` で返る。
pub fn describe_bundle(executable: &Path, bundle: &Path, timeout: Duration) -> Result<BundleDesc> {
    let mut child = Command::new(executable)
        .arg(DESCRIBE_COMMAND)
        .arg(bundle)
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
            what: "host_ofx.describe".into(),
            limit: timeout,
        });
    };
    let text = read?;
    let Some(status) = wait_exit(&mut child, EXIT_WAIT)? else {
        // 出力を閉じてから終わらないプロセスでも、ここで待ち続けない。
        kill_tree(&mut child, job.as_ref())?;
        return Err(MmError::Timeout {
            what: "host_ofx.describe_exit".into(),
            limit: EXIT_WAIT,
        });
    };
    if !status.success() {
        return Err(MmError::HostCrashed(format!(
            "{} の describe が {status} で終わった",
            bundle.display()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 実行ファイルが無ければ起動の失敗を返す() {
        let spec = HostSpec {
            executable: "mm-host-ofx-が無い場所/none.exe".into(),
            args: Vec::new(),
            default_timeout: Duration::from_secs(1),
            gpu: None,
            client_pid: None,
        };
        let err = OfxLauncher.spawn(&spec).err().expect("起動できないはず");
        assert!(matches!(err, MmError::HostCrashed(_)), "{err:?}");
    }
}
