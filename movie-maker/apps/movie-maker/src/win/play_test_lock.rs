//! `--play-test` を同じデスクトップで 1 つずつ動かす PC 全体のロック。
//!
//! 試験は実際のウィンドウへマウスとキーを送り、前面とマウスのキャプチャを取る。別の movie-maker が同じデスクトップで前面やキャプチャを
//! 取ると、ドラッグが離す前に取り消され、工程が実行ごとに通ったり落ちたりする。そこで試験は始める前に名前付きのミューテックスを取り、
//! 終わるまで持つ。持ち主のプロセスがクラッシュしても OS が放す（abandoned も取れたものとして進める）。ふだんの起動は取らない。
//!
//! ミューテックスは取ったスレッドしか放せない。取ったあとで待つ専用のスレッドが持ち、`Lock` の破棄で放す。

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use windows::core::HSTRING;
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, WAIT_ABANDONED_0, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcessId, ReleaseMutex, WaitForSingleObject,
};

/// ログオンのセッション（= 同じデスクトップ）の中で 1 つだけ。
const MUTEX_NAME: &str = "Local\\movie-maker-play-test";
const IMAGE_NAME: &str = "movie-maker.exe";

/// 待ちの上限と、待っていることを知らせる間隔。
#[derive(Clone, Copy)]
struct Timing {
    limit: Duration,
    notice: Duration,
}

/// 15 分。通しは 1 回に数分かかるので、数本の順番待ちを許す。
const TIMING: Timing = Timing {
    limit: Duration::from_secs(15 * 60),
    notice: Duration::from_secs(10),
};

enum Event {
    Waiting(Duration),
    Acquired { abandoned: bool },
    TimedOut,
    Failed(String),
}

/// 取ったロック。破棄すると放す。ミューテックスを作れなかったときは何も持たず、待たせずに進める。
struct Lock {
    held: Option<(Sender<()>, JoinHandle<()>)>,
    waited: Duration,
    abandoned: bool,
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some((release, thread)) = self.held.take() {
            drop(release);
            let _ = thread.join();
        }
    }
}

/// 取れるまで待つ専用のスレッドの本体。取れたら `release` の送り手が落ちるまで持つ。
fn hold(name: &str, timing: Timing, events: &Sender<Event>, release: &Receiver<()>) {
    let name = HSTRING::from(name);
    // SAFETY: 名前は呼ぶ間だけ生きていればよく、返ったハンドルはこの関数の出口で必ず閉じる。
    let handle = match unsafe { CreateMutexW(None, false, &name) } {
        Ok(h) => h,
        Err(e) => {
            let _ = events.send(Event::Failed(e.to_string()));
            return;
        }
    };
    if let Some(abandoned) = wait(handle, timing, events) {
        let _ = events.send(Event::Acquired { abandoned });
        let _ = release.recv();
        // SAFETY: `wait` で持ったミューテックスを、持ったスレッドから 1 回だけ放す。
        let _ = unsafe { ReleaseMutex(handle) };
    }
    // SAFETY: 自分で作ったハンドルを 1 回だけ閉じる。
    let _ = unsafe { CloseHandle(handle) };
}

/// 持てたら `Some(前の持ち主が落ちて放されたか)`。上限か失敗なら結果を送って `None`。
fn wait(handle: HANDLE, timing: Timing, events: &Sender<Event>) -> Option<bool> {
    let start = Instant::now();
    loop {
        let left = timing.limit.saturating_sub(start.elapsed());
        if left.is_zero() {
            let _ = events.send(Event::TimedOut);
            return None;
        }
        let slice = left.min(timing.notice).as_millis() as u32;
        // SAFETY: ハンドルは `hold` が作った生きているミューテックス。
        let result = unsafe { WaitForSingleObject(handle, slice) };
        match result {
            WAIT_OBJECT_0 => return Some(false),
            WAIT_ABANDONED_0 => return Some(true),
            WAIT_TIMEOUT => {
                let _ = events.send(Event::Waiting(start.elapsed()));
            }
            other => {
                let _ = events.send(Event::Failed(format!("待ちが失敗した: {other:?}")));
                return None;
            }
        }
    }
}

/// 名前 `name` のロックを、取れるまで待って取る。待つ間は `notice` ごとに `on_wait`（待った時間）を呼ぶ。
/// 上限を超えたら `Err(待った時間)`。
fn acquire_named(
    name: &str,
    timing: Timing,
    mut on_wait: impl FnMut(Duration),
) -> Result<Lock, Duration> {
    let start = Instant::now();
    let (events_tx, events) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let name = name.to_owned();
    let thread = std::thread::Builder::new()
        .name("play-test-lock".into())
        .spawn(move || hold(&name, timing, &events_tx, &release_rx))
        .expect("play-test-lock thread");
    loop {
        let event = events
            .recv()
            .unwrap_or_else(|_| Event::Failed("スレッドが落ちた".into()));
        match event {
            Event::Waiting(waited) => on_wait(waited),
            Event::Acquired { abandoned } => {
                let held = Some((release, thread));
                return Ok(Lock {
                    held,
                    waited: start.elapsed(),
                    abandoned,
                });
            }
            Event::TimedOut => {
                let _ = thread.join();
                return Err(start.elapsed());
            }
            Event::Failed(why) => {
                let _ = thread.join();
                tracing::warn!("--play-test のロックを作れない（絞らずに進める）: {why}");
                return Ok(Lock {
                    held: None,
                    waited: start.elapsed(),
                    abandoned: false,
                });
            }
        }
    }
}

/// 同じセッションで動いている、自分以外の movie-maker.exe の数。数えられなければ `None`。
fn other_instances() -> Option<usize> {
    // SAFETY: スナップショットはこの関数の出口で閉じ、エントリーは `dwSize` を入れてから渡す。
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let me = GetCurrentProcessId();
        let mut my_session = 0;
        if ProcessIdToSessionId(me, &mut my_session).is_err() {
            let _ = CloseHandle(snap);
            return None;
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut count = 0;
        let mut more = Process32FirstW(snap, &mut entry).is_ok();
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            let image = String::from_utf16_lossy(&entry.szExeFile[..len]);
            let mut session = u32::MAX;
            let same = ProcessIdToSessionId(entry.th32ProcessID, &mut session).is_ok()
                && session == my_session;
            if entry.th32ProcessID != me && same && image.eq_ignore_ascii_case(IMAGE_NAME) {
                count += 1;
            }
            more = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
        Some(count)
    }
}

/// 取ったあとの状態。報告の JSON に書く値の元。
struct Entered {
    lock: Option<Lock>,
    waited: Duration,
    abandoned: bool,
    others_at_start: Option<usize>,
}

static ENTERED: Mutex<Option<Entered>> = Mutex::new(None);

/// ロックの状態の JSON。終わりの時点の台数も測り直す（待っている間や試験の間に始まったものも見分けるため）。
pub fn report() -> Value {
    let guard = ENTERED.lock().expect("play-test lock state");
    let Some(e) = guard.as_ref() else {
        return Value::Null;
    };
    json!({
        "waited_secs": e.waited.as_secs_f64(),
        "abandoned": e.abandoned,
        "other_movie_makers": { "at_start": e.others_at_start, "at_end": other_instances() },
    })
}

/// 試験の工程が終わったら放す（次の試験がウィンドウを閉じる間を待たずに始められる）。
pub fn release() {
    if let Some(e) = ENTERED.lock().expect("play-test lock state").as_mut() {
        e.lock = None;
    }
}

fn write_report(path: &Path, value: &Value) {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    if let Err(e) = std::fs::write(path, text) {
        eprintln!("結果を書けない: {e}");
    }
}

/// `--play-test` の始めに呼ぶ。取れるまで待つ（待っている間は標準エラーと報告に書く）。上限を超えたら報告に理由を書いて `Err`。
pub fn enter(report_path: &Path) -> Result<(), String> {
    eprintln!("[play-test] ほかの --play-test の終わりを待つ（ロック {MUTEX_NAME}）");
    let lock = acquire_named(MUTEX_NAME, TIMING, |waited| {
        let secs = waited.as_secs();
        eprintln!("[play-test] 待っている: {secs} 秒");
        let state = json!({ "lock": { "state": "waiting", "waited_secs": secs } });
        write_report(report_path, &state);
    });
    match lock {
        Ok(lock) => {
            let (waited, abandoned) = (lock.waited, lock.abandoned);
            if waited >= Duration::from_secs(1) {
                eprintln!("[play-test] {} 秒待って始める", waited.as_secs());
            }
            *ENTERED.lock().expect("play-test lock state") = Some(Entered {
                lock: Some(lock),
                waited,
                abandoned,
                others_at_start: other_instances(),
            });
            Ok(())
        }
        Err(waited) => {
            let why = format!(
                "ほかの --play-test が {} 秒たっても終わらず、始められない（上限 {} 秒）",
                waited.as_secs(),
                TIMING.limit.as_secs()
            );
            let lock = json!({ "state": "timed_out", "waited_secs": waited.as_secs_f64() });
            write_report(report_path, &json!({ "failed": why, "lock": lock }));
            Err(why)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const FAST: Timing = Timing {
        limit: Duration::from_millis(400),
        notice: Duration::from_millis(100),
    };

    /// テストごとに別の名前（並べて回しても、本物の試験のロックとも干渉しない）。
    fn unique_name() -> String {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        format!("Local\\mm-play-test-lock-test-{}-{n}", std::process::id())
    }

    #[test]
    fn 空いていれば待たずに取れる() {
        let name = unique_name();
        let lock = acquire_named(&name, FAST, |_| panic!("待たないはず")).unwrap();
        assert!(!lock.abandoned && lock.held.is_some());
    }

    #[test]
    fn 持っている間は待ち_上限で失敗し_放せば取れる() {
        let name = unique_name();
        let first = acquire_named(&name, FAST, |_| {}).unwrap();
        let mut notices = 0;
        let second = acquire_named(&name, FAST, |_| notices += 1);
        assert!(matches!(second, Err(w) if w >= FAST.limit));
        assert!(notices >= 2, "待っている間に知らせる: {notices}");
        drop(first);
        assert!(acquire_named(&name, FAST, |_| {}).is_ok());
    }

    #[test]
    fn 放されるまで待ってから取る() {
        let name = unique_name();
        let first = acquire_named(&name, FAST, |_| {}).unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(first);
        });
        let second = acquire_named(&name, FAST, |_| {}).unwrap();
        releaser.join().unwrap();
        assert!(second.waited >= Duration::from_millis(100));
    }

    #[test]
    fn 持ち主が放さずに落ちても取れる() {
        let name = unique_name();
        let hname = HSTRING::from(name.as_str());
        // 取ったスレッドが放さずに終わると、ミューテックスは abandoned になる（ハンドルは開いたまま）。
        std::thread::spawn(move || {
            // SAFETY: 名前は呼ぶ間だけ生きていればよく、ハンドルは abandoned を作るためにわざと放さず閉じない。
            unsafe {
                let h = CreateMutexW(None, false, &hname).unwrap();
                assert_eq!(WaitForSingleObject(h, 1000), WAIT_OBJECT_0);
            }
        })
        .join()
        .unwrap();
        let lock = acquire_named(&name, FAST, |_| {}).unwrap();
        assert!(lock.abandoned);
    }

    #[test]
    fn 自分以外の台数を数えられる() {
        assert!(other_instances().is_some());
    }
}
