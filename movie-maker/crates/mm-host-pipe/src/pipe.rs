//! 子プロセスのホストとの 1 本の接続。
//!
//! 期限を過ぎた要求の応答は後から届くので、その数を数えておき、次の要求の前に読み捨てる
//! （ホストは要求を 1 つずつ順に処理し、どの要求にも 1 行で答える）。
//!
//! 一時フォルダーはランチャーが作って渡し、ホストを止めた後に消す。kill で止めたホストは自分で片付けられないため。

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mm_host_api::{
    HostProcess, HostRequest, HostResponse, HostSpec, SharedFrame, PROTOCOL_VERSION,
};
use mm_types::{MmError, Result};

use crate::job::{kill_tree, Job};

/// ホストへ起動の指定（`HostSpec` の JSON）を渡す環境変数。
pub const SPEC_ENV: &str = "MM_HOST_SPEC";
/// プロトコルで応答するモードのサブコマンド。
pub const SERVE_COMMAND: &str = "serve";
/// 止めたホストの一時フォルダーを消し終えるまでの期限。ホストの子のプロセスが終わってファイルを放すのを待つ。
const CLEANUP_LIMIT: Duration = Duration::from_secs(5);
/// 一時フォルダーを消し直す間隔。
const CLEANUP_STEP: Duration = Duration::from_millis(50);
/// `Shutdown` の応答と、その後にプロセスが終わるのを待つ期限。
const SHUTDOWN_WAIT: Duration = Duration::from_secs(2);
/// `Shutdown` の後に終わったかを見直す間隔。
const SHUTDOWN_STEP: Duration = Duration::from_millis(10);

/// 期限を過ぎて読み捨てる応答に共有ハンドルが入っていたときの扱い。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleHandles {
    /// ホストがこのプロセスへ複製したハンドルなので閉じる（`mm-host-cef`）。
    Close,
    /// このホストは共有テクスチャを返さない約束なので、触らずに漏れとして数える（`mm-host-ofx`）。
    /// 複製されていない値を閉じると、このプロセスの別のハンドルを閉じてしまう。
    Count,
}

/// ホストの実装ごとの起こし方。各ホストのクレートが定数で 1 つ持つ。
#[derive(Debug, Clone, Copy)]
pub struct PipeKind {
    /// 時間切れの `what` と一時フォルダーの名前に使う（`host_cef` など）。
    pub name: &'static str,
    /// 起動直後の `Ping` の期限。
    pub startup_timeout: Duration,
    /// 一時フォルダーを渡す環境変数。`None` なら一時フォルダーを作らない。
    pub work_dir_env: Option<&'static str>,
    /// 読み捨てる応答の共有ハンドルの扱い。
    pub stale_handles: StaleHandles,
    /// 起こす前にコマンドへ足す設定（子の `PATH` など）。
    pub configure: fn(&mut Command),
}

/// 起動した 1 プロセスとの接続。
struct Connection {
    child: Mutex<Child>,
    /// ホストとホストが起こしたプロセスの木。作れなかった環境ではホストだけを止める。
    job: Option<Job>,
    stdin: ChildStdin,
    lines: Receiver<String>,
    pid: u32,
    work_dir: Option<PathBuf>,
}

/// 子プロセスのホスト。
pub struct PipeHost {
    kind: PipeKind,
    spec: HostSpec,
    conn: Option<Connection>,
    /// 期限を過ぎて、まだ読み捨てていない応答の数。
    stale: usize,
    /// 読み捨てた応答のうち、閉じられなかった共有ハンドルの数。
    open: usize,
}

impl PipeHost {
    /// 起動して `Ping` が通るまで待つ。
    pub fn start(kind: PipeKind, spec: HostSpec) -> Result<Self> {
        let mut host = Self {
            conn: Some(launch(&kind, &spec)?),
            kind,
            spec,
            stale: 0,
            open: 0,
        };
        match host.request(&HostRequest::Ping, kind.startup_timeout) {
            Ok(HostResponse::Pong { protocol_version }) if protocol_version == PROTOCOL_VERSION => {
                Ok(host)
            }
            other => {
                // 接続が落ちたのなら request が数えている。
                if !matches!(other, Err(MmError::HostCrashed(_))) {
                    host.count_crash();
                }
                // 起動の失敗を返すのが先で、止められなかったことは Drop の後片付けに任せる。
                let _ = host.kill();
                Err(MmError::HostCrashed(format!(
                    "起動直後の Ping にプロトコルのバージョン {PROTOCOL_VERSION} の Pong が来ない: {other:?}"
                )))
            }
        }
    }

    /// ホストが落ちたことを数える（`mm_trace::names::host_crash_counter`）。アプリはこのカウンタで自動の起票をする。
    /// こちらから止めたホスト（時間切れ、Shutdown）は数えない。
    fn count_crash(&self) {
        mm_trace::counter!(
            mm_trace::names::host_crash_counter(self.kind.name).as_str(),
            1
        );
    }

    /// 動いているホストの一時フォルダー。止めた後や、一時フォルダーを使わないホストは `None`。
    pub fn work_dir(&self) -> Option<&Path> {
        self.conn.as_ref().and_then(|c| c.work_dir.as_deref())
    }

    fn send(conn: &mut Connection, req: &HostRequest) -> Result<()> {
        let mut line = serde_json::to_string(req)?;
        line.push('\n');
        conn.stdin
            .write_all(line.as_bytes())
            .and_then(|()| conn.stdin.flush())
            .map_err(|e| MmError::HostCrashed(format!("要求を書けない: {e}")))
    }

    /// 次の 1 行を `deadline` まで待って読む。
    fn next_response(&mut self, deadline: Instant, timeout: Duration) -> Result<HostResponse> {
        let Some(conn) = self.conn.as_ref() else {
            return Err(MmError::HostCrashed("ホストが動いていない".into()));
        };
        let left = deadline.saturating_duration_since(Instant::now());
        let line = match conn.lines.recv_timeout(left) {
            Ok(line) => line,
            Err(RecvTimeoutError::Timeout) => {
                self.stale += 1;
                return Err(MmError::Timeout {
                    what: format!("{}.request", self.kind.name),
                    limit: timeout,
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                // 出力が閉じたのはプロセスが終わる途中なので、終わるまで待って is_alive を偽にそろえる。
                self.count_crash();
                self.kill()?;
                return Err(MmError::HostCrashed("ホストの出力が閉じた".into()));
            }
        };
        // 読めない行をスキップすると、読み捨てる数が合わなくなり、以後の要求が前の要求の応答を受け取り続ける。
        serde_json::from_str(&line).or_else(|e| {
            self.count_crash();
            self.kill()?;
            Err(MmError::HostCrashed(format!("ホストの応答が読めない: {e}")))
        })
    }

    /// `Shutdown` を送って終わるのを少し待ち、終わらなければ止める。
    fn shutdown(&mut self) {
        if self.is_alive() && self.request(&HostRequest::Shutdown, SHUTDOWN_WAIT).is_ok() {
            let start = Instant::now();
            while self.is_alive() && start.elapsed() < SHUTDOWN_WAIT {
                std::thread::sleep(SHUTDOWN_STEP);
            }
        }
        // Drop の中なので返す先が無い。kill_tree の失敗は既に終わったプロセスの wait だけで、木はジョブで止まる。
        let _ = self.kill();
    }
}

/// このプロセスが起こすホストごとの一時フォルダーを作る。
fn create_work_dir(name: &str) -> Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mm-{name}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| MmError::Internal(format!("{} を作れない: {e}", dir.display())))?;
    Ok(dir)
}

/// 止めたホストの一時フォルダーを消す。ホストの子のプロセスがファイルを放すまで `limit` まで消し直す。
/// 消せたら（元から無ければ）真。
pub fn remove_work_dir(dir: &Path, limit: Duration) -> bool {
    let start = Instant::now();
    loop {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
            Err(_) if start.elapsed() < limit => std::thread::sleep(CLEANUP_STEP),
            Err(_) => return false,
        }
    }
}

fn command(kind: &PipeKind, spec: &HostSpec, work_dir: Option<&Path>) -> Result<Command> {
    let mut cmd = Command::new(&spec.executable);
    cmd.arg(SERVE_COMMAND)
        .args(&spec.args)
        .env(SPEC_ENV, serde_json::to_string(spec)?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let (Some(env), Some(dir)) = (kind.work_dir_env, work_dir) {
        cmd.env(env, dir);
    }
    (kind.configure)(&mut cmd);
    Ok(cmd)
}

fn launch(kind: &PipeKind, spec: &HostSpec) -> Result<Connection> {
    let work_dir = kind
        .work_dir_env
        .map(|_| create_work_dir(kind.name))
        .transpose()?;
    let spawned = command(kind, spec, work_dir.as_deref()).and_then(|mut cmd| {
        cmd.spawn().map_err(|e| {
            MmError::HostCrashed(format!("{} を起動できない: {e}", spec.executable.display()))
        })
    });
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            if let Some(dir) = &work_dir {
                remove_work_dir(dir, Duration::ZERO);
            }
            return Err(e);
        }
    };
    // ジョブに入れておくと、アプリがクラッシュしてジョブのハンドルが閉じたときにホストの木も止まる。
    let job = Job::attach(&child);
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        // 接続にならない子を残さない。止めた失敗より、パイプが無いことを返す。
        let _ = kill_tree(&mut child, job.as_ref());
        if let Some(dir) = &work_dir {
            remove_work_dir(dir, CLEANUP_LIMIT);
        }
        return Err(MmError::Internal("子プロセスのパイプが無い".into()));
    };
    Ok(Connection {
        pid: child.id(),
        child: Mutex::new(child),
        job,
        stdin,
        lines: read_lines(stdout),
        work_dir,
    })
}

/// 読む側を別スレッドにして、期限つきで待てるようにする。パイプが閉じるとスレッドも終わる。
fn read_lines(stdout: std::process::ChildStdout) -> Receiver<String> {
    let (tx, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout)
            .lines()
            .map_while(std::io::Result::ok)
        {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    lines
}

/// 読み捨てる応答の共有ハンドルを扱い、閉じられなかった数を返す。
fn discard(response: &HostResponse, policy: StaleHandles) -> usize {
    let HostResponse::Frame(SharedFrame::SharedTexture { handle, fence, .. }) = response else {
        return 0;
    };
    let handles = std::iter::once(*handle).chain(fence.map(|f| f.handle));
    match policy {
        StaleHandles::Close => handles.filter(|h| !close_handle(*h)).count(),
        StaleHandles::Count => handles.count(),
    }
}

#[cfg(windows)]
fn close_handle(handle: u64) -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    // SAFETY: ホストがこのプロセスへ複製した NT ハンドルで、ほかに使う所は無い。
    unsafe { CloseHandle(HANDLE(handle as usize as *mut _)) }.is_ok()
}

#[cfg(not(windows))]
fn close_handle(_handle: u64) -> bool {
    false
}

impl HostProcess for PipeHost {
    fn is_alive(&self) -> bool {
        self.conn.as_ref().is_some_and(|c| {
            let mut child = c.child.lock().unwrap_or_else(|e| e.into_inner());
            matches!(child.try_wait(), Ok(None))
        })
    }

    fn pid(&self) -> Option<u32> {
        self.conn.as_ref().map(|c| c.pid)
    }

    fn request(&mut self, req: &HostRequest, timeout: Duration) -> Result<HostResponse> {
        if self.conn.is_some() && !self.is_alive() {
            // 要求の合間に終わっていた。止めて接続を外し、同じクラッシュを 2 度数えない。
            self.count_crash();
            self.kill()?;
        }
        let conn = self
            .conn
            .as_mut()
            .ok_or_else(|| MmError::HostCrashed("ホストが動いていない".into()))?;
        if let Err(e) = Self::send(conn, req) {
            self.count_crash();
            return Err(e);
        }
        let deadline = Instant::now() + timeout;
        loop {
            let response = self.next_response(deadline, timeout)?;
            if self.stale == 0 {
                return Ok(response);
            }
            self.stale -= 1;
            self.open += discard(&response, self.kind.stale_handles);
        }
    }

    fn kill(&mut self) -> Result<()> {
        self.stale = 0;
        let Some(conn) = self.conn.take() else {
            return Ok(());
        };
        let mut child = conn.child.lock().unwrap_or_else(|e| e.into_inner());
        let killed = kill_tree(&mut child, conn.job.as_ref());
        drop(child);
        drop(conn.job);
        if let Some(dir) = &conn.work_dir {
            // 消せなくても次のホストは別のフォルダーを使うので、止めたことを優先して続ける。
            remove_work_dir(dir, CLEANUP_LIMIT);
        }
        Ok(killed?)
    }

    fn restart(&mut self) -> Result<()> {
        self.kill()?;
        *self = Self::start(self.kind, self.spec.clone())?;
        Ok(())
    }

    fn open_handles(&self) -> usize {
        self.open
    }
}

impl Drop for PipeHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIND: PipeKind = PipeKind {
        name: "host_test",
        startup_timeout: Duration::from_secs(1),
        work_dir_env: Some("MM_HOST_WORK_DIR"),
        stale_handles: StaleHandles::Count,
        configure: |_| {},
    };

    fn spec(executable: &str) -> HostSpec {
        HostSpec {
            executable: executable.into(),
            args: Vec::new(),
            default_timeout: Duration::from_secs(1),
            gpu: None,
            client_pid: None,
        }
    }

    #[test]
    fn 実行ファイルが無ければ起動の失敗を返し一時フォルダーを残さない() {
        let before = std::fs::read_dir(std::env::temp_dir())
            .map(|d| d.flatten().filter(is_ours).count())
            .unwrap_or(0);
        let err = PipeHost::start(KIND, spec("mm-host-が無い場所/none.exe"))
            .err()
            .expect("起動できないはず");
        assert!(matches!(err, MmError::HostCrashed(_)), "{err:?}");
        let after = std::fs::read_dir(std::env::temp_dir())
            .map(|d| d.flatten().filter(is_ours).count())
            .unwrap_or(0);
        assert!(after <= before, "一時フォルダーが残った");
    }

    #[test]
    fn 起動直後に落ちたホストを_1_回だけ数える() {
        use tracing_subscriber::prelude::*;
        let (layer, metrics) = mm_trace::MetricsLayer::new();
        let subscriber = tracing_subscriber::registry().with(layer);
        // git は `serve` を知らないので、何も答えずに終わる（出力が閉じる）。
        let started = tracing::subscriber::with_default(subscriber, || {
            PipeHost::start(KIND, spec("git")).err()
        });
        assert!(
            matches!(started, Some(MmError::HostCrashed(_))),
            "{started:?}"
        );
        let name = mm_trace::names::host_crash_counter(KIND.name);
        assert_eq!(metrics.snapshot().counters.get(&name), Some(&1.0));
    }

    fn is_ours(entry: &std::fs::DirEntry) -> bool {
        let prefix = format!("mm-{}-{}-", KIND.name, std::process::id());
        entry.file_name().to_string_lossy().starts_with(&prefix)
    }

    #[test]
    fn cpu_の画像の応答は閉じるハンドルが無い() {
        let frame = SharedFrame::Cpu {
            size: mm_types::Size::new(1, 1),
            data: vec![0.0; 4],
        };
        for policy in [StaleHandles::Close, StaleHandles::Count] {
            assert_eq!(discard(&HostResponse::Frame(frame.clone()), policy), 0);
            assert_eq!(discard(&HostResponse::Prepared, policy), 0);
        }
    }

    #[test]
    fn 数えるだけの扱いはハンドルとフェンスを漏れに数える() {
        let frame = SharedFrame::SharedTexture {
            size: mm_types::Size::new(1, 1),
            handle: 0x1234,
            fence: Some(mm_host_api::SharedFence {
                handle: 0x5678,
                value: 1,
            }),
        };
        assert_eq!(discard(&HostResponse::Frame(frame), StaleHandles::Count), 2);
    }
}
