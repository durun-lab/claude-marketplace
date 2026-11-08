//! ホストと、ホストが起こすプロセス（CEF の GPU やレンダラ）をまとめて止めるための Windows のジョブオブジェクト。
//!
//! ホストだけを `TerminateProcess` で止めると、ホストの子のプロセスがしばらく残って一時フォルダーを掴み続ける。
//! ジョブに入れておけば、止めるときに木ごと止められ、アプリがクラッシュしてジョブのハンドルが閉じたときも一緒に止まる。
//!
//! ドライバーの中で止まったプロセスは `TerminateProcess` でも終わらないので、止めた後に待つのは期限までにする。

use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

/// 止めたプロセスが終わるのを待つ期限。過ぎたら待つのをやめ、プロセスのハンドルだけを手放す。
const KILL_WAIT: Duration = Duration::from_secs(5);
/// 終わったかを見直す間隔。
const EXIT_STEP: Duration = Duration::from_millis(10);

#[cfg(windows)]
mod imp {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// 1 つのホストの木。
    pub struct Job(HANDLE);

    // SAFETY: ジョブのハンドルはプロセスの中のどのスレッドからでも使える。
    unsafe impl Send for Job {}
    // SAFETY: 同上。ハンドルを書き換える操作は無い。
    unsafe impl Sync for Job {}

    impl Job {
        /// ジョブを作って `child` を入れる。作れなければ `None`（止めるときはホストだけを止める）。
        pub fn attach(child: &Child) -> Option<Self> {
            // SAFETY: 名前の無いジョブを作る。失敗は Err で返る。
            let job = Self(unsafe { CreateJobObjectW(None, None) }.ok()?);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = std::mem::size_of_val(&info) as u32;
            let process = HANDLE(child.as_raw_handle());
            // SAFETY: info はこの呼び出しの間だけ有効な、記述どおりの大きさの構造体。process は生きている子のハンドル。
            unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    std::ptr::from_ref(&info).cast(),
                    size,
                )
                .ok()?;
                AssignProcessToJobObject(job.0, process).ok()?;
            }
            Some(job)
        }

        /// ジョブの中のプロセスを全部止める。
        pub fn terminate(&self) {
            // SAFETY: 自分で作ったジョブのハンドル。止まっているプロセスしか無ければ何もしない。
            let _ = unsafe { TerminateJobObject(self.0, 1) };
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: CreateJobObjectW で作ったハンドル。閉じると KILL_ON_JOB_CLOSE で残りも止まる。
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::process::Child;

    /// ジョブの無い環境の代わり。止めるときはホストだけを止める。
    pub struct Job;

    impl Job {
        pub fn attach(_child: &Child) -> Option<Self> {
            None
        }

        pub fn terminate(&self) {}
    }
}

pub use imp::Job;

/// `child` を、ジョブがあればジョブごと止めて、[`KILL_WAIT`] まで終わるのを待つ。
/// 期限までに終わらなければ `TimedOut` を返す。呼び出し側はそのプロセスを捨てて先へ進む。
pub fn kill_tree(child: &mut Child, job: Option<&Job>) -> std::io::Result<()> {
    if let Some(job) = job {
        job.terminate();
    }
    // 既に終わっていれば kill は失敗するが、待つ所で後片付けだけする。
    let _ = child.kill();
    match wait_exit(child, KILL_WAIT)? {
        Some(_) => Ok(()),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!(
                "プロセス {} が止めても {KILL_WAIT:?} の間に終わらない",
                child.id()
            ),
        )),
    }
}

/// `limit` まで終わるのを待つ。終わらなければ `None`。
pub fn wait_exit(child: &mut Child, limit: Duration) -> std::io::Result<Option<ExitStatus>> {
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
    use std::process::Stdio;

    /// 標準入力が閉じるまで終わらない子。
    fn waiting_child() -> Child {
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/c", "more"])
        } else {
            ("cat", &[])
        };
        mm_types::process::command(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .expect("子を起こせる")
    }

    #[test]
    fn 終わらない子は期限で待つのをやめる() {
        let mut child = waiting_child();
        let start = Instant::now();
        assert!(wait_exit(&mut child, Duration::from_millis(50))
            .unwrap()
            .is_none());
        assert!(start.elapsed() < Duration::from_secs(5));
        let job = Job::attach(&child);
        kill_tree(&mut child, job.as_ref()).expect("止められる子は止まる");
        assert!(child.try_wait().unwrap().is_some());
    }
}
