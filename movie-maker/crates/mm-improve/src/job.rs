//! `claude -p` の木をまとめて止める Windows のジョブオブジェクト（`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`）。
//!
//! アプリがクラッシュすると OS がジョブのハンドルを閉じ、木の全部が止まる。呼び出しが終わってハンドルを閉じたときも、
//! claude が後ろに回したコマンドが残らない。mm-host-pipe の `Job` と同じ形だが、操作経路は実装のクレートに依存しない
//! （docs/design/14-dev-structure.md の規則 4）ので、ここに持つ。

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

    /// 1 つの `claude -p` の木。破棄すると木の残りも止まる。
    pub struct Job(HANDLE);

    // SAFETY: ジョブのハンドルはプロセスの中のどのスレッドからでも使える。
    unsafe impl Send for Job {}
    // SAFETY: 同上。ハンドルを書き換える操作は無い。
    unsafe impl Sync for Job {}

    impl Job {
        /// ジョブを作って `child` を入れる。作れなければ `None`（止めるときは taskkill で木を止める）。
        pub fn attach(child: &Child) -> Option<Self> {
            // SAFETY: 名前の無いジョブを作る。セキュリティ属性を渡さないので、ハンドルは子に継承されない。
            let job = Self(unsafe { CreateJobObjectW(None, None) }.ok()?);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = u32::try_from(std::mem::size_of_val(&info)).ok()?;
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
            drop(unsafe { TerminateJobObject(self.0, 1) });
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: CreateJobObjectW で作ったハンドル。閉じると KILL_ON_JOB_CLOSE で残りも止まる。
            drop(unsafe { CloseHandle(self.0) });
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::process::Child;

    /// ジョブの無い環境の代わり。止めるときは kill で止める。
    pub struct Job;

    impl Job {
        /// 常に `None`。
        pub fn attach(_child: &Child) -> Option<Self> {
            None
        }

        /// 何もしない。
        pub fn terminate(&self) {}
    }
}

pub(crate) use imp::Job;
