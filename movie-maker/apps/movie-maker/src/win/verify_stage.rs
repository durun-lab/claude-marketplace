//! `--play-test` の工程（再生、入力、Inspector、Timeline の編集）を 1 つずつ別のスレッドで実行し、上限の時間を
//! 過ぎたら時間切れにする。ウィンドウへの呼び出しが返らずに工程が止まっても、テストは結果の JSON を書いて終わる。
//!
//! 時間切れにした工程のスレッドは、次の [`wait_until`] か [`checkpoint`] でスタックを巻き戻して止まり、それより後の
//! 入力をウィンドウへ送らない。巻き戻しは salsa の `Cancelled` と同じく `resume_unwind` で起こし、panic のフック
//! （不具合の自動の起票）を通さない。

use std::cell::RefCell;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::win::feedback::panic_message;

/// 状態を見に行く間隔。
const POLL: Duration = Duration::from_millis(25);
/// 時間切れにした工程のスレッドが止まるのを待つ上限。返らない呼び出しの中にいて止まらなければ、待つのをやめて次へ進む。
const STOP_GRACE: Duration = Duration::from_secs(10);

thread_local! {
    /// このスレッドで実行している工程の時間切れのフラグ。工程のスレッドの外では `None`。
    static TIMED_OUT: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// 時間切れでスタックを巻き戻すときの payload。
struct TimedOut;

/// 工程が結果を返さなかった理由。
#[derive(Debug)]
pub enum StageError {
    /// 上限の時間を過ぎた。
    TimedOut(Duration),
    /// パニックした、またはスレッドを作れなかった。
    Failed(String),
}

impl std::fmt::Display for StageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut(limit) => {
                write!(
                    f,
                    "上限 {} 秒を過ぎたので時間切れにした",
                    limit.as_secs_f64()
                )
            }
            Self::Failed(e) => f.write_str(e),
        }
    }
}

/// 工程のスレッドで時間切れのフラグが立っていれば、スタックを巻き戻して止まる。返るまで長くかかりうる呼び出し
/// （スクリーンショット）の後に呼ぶ。
pub fn checkpoint() {
    let timed_out = TIMED_OUT.with(|f| {
        f.borrow()
            .as_ref()
            .is_some_and(|f| f.load(Ordering::SeqCst))
    });
    if timed_out {
        resume_unwind(Box::new(TimedOut));
    }
}

/// `done` が真になるまで待ち、`limit` を過ぎたら偽を返す。工程のスレッドで時間切れになったら止まる（[`checkpoint`]）。
pub fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        checkpoint();
        if done() {
            return true;
        }
        std::thread::sleep(POLL);
    }
    false
}

/// `f` を工程 `name` のスレッドで実行し、`limit` までに返らなければ時間切れにする。
pub fn run<T: Send + 'static>(
    name: &str,
    limit: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, StageError> {
    let flag = Arc::new(AtomicBool::new(false));
    let thread_flag = flag.clone();
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name(format!("play-test-{name}"))
        .spawn(move || {
            TIMED_OUT.with(|f| *f.borrow_mut() = Some(thread_flag));
            let _ = tx.send(catch_unwind(AssertUnwindSafe(f)));
        })
        .map_err(|e| StageError::Failed(format!("工程のスレッドを作れない: {e}")))?;
    match rx.recv_timeout(limit) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(payload)) => Err(StageError::Failed(format!(
            "パニックした: {}",
            panic_message(&*payload)
        ))),
        Err(RecvTimeoutError::Disconnected) => Err(StageError::Failed(
            "工程のスレッドが結果を返さずに終わった".into(),
        )),
        Err(RecvTimeoutError::Timeout) => {
            flag.store(true, Ordering::SeqCst);
            let _ = rx.recv_timeout(STOP_GRACE);
            Err(StageError::TimedOut(limit))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    #[test]
    fn 上限までに返った工程は値を返す() {
        assert_eq!(run("ok", Duration::from_secs(5), || 7).unwrap(), 7);
    }

    #[test]
    fn 時間切れの工程は次の待ちで止まり_後の処理を行わない() {
        let after = Arc::new(AtomicUsize::new(0));
        let seen = after.clone();
        let r = run("slow", Duration::from_millis(50), move || {
            wait_until(Duration::from_secs(5), || false);
            seen.fetch_add(1, Ordering::SeqCst);
        });
        assert!(matches!(r, Err(StageError::TimedOut(_))));
        assert_eq!(
            after.load(Ordering::SeqCst),
            0,
            "巻き戻った後の入力は送らない"
        );
    }

    #[test]
    fn パニックした工程は失敗として返す() {
        let r = run("panic", Duration::from_secs(5), || -> u8 {
            panic!("壊れた")
        });
        let Err(StageError::Failed(message)) = r else {
            panic!("失敗として返る: {r:?}");
        };
        assert!(message.contains("壊れた"), "{message}");
    }

    #[test]
    fn 工程の外の待ちは巻き戻らない() {
        assert!(!wait_until(Duration::from_millis(30), || false));
        checkpoint();
    }
}
