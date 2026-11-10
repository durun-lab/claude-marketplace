//! `--play-test` を同じデスクトップで 1 つずつ動かす、再生テストのロック。
//!
//! 再生テストは実際のウィンドウへマウスとキーを送り、前面とマウスのキャプチャーを取る。同じデスクトップの別の再生テストが
//! 前面やキャプチャーを取ると、ドラッグが離す前に取り消され、工程が実行ごとに通ったり失敗したりする。そこで再生テストは
//! ウィンドウとプロジェクトを開く前に名前付きのミューテックスを取り、工程が終わったら放す。持っていたプロセスがクラッシュ
//! しても OS が放す（放棄されたミューテックスも取れたものとして進める）。ふだんの起動は取らない。
//!
//! ミューテックスは持ったスレッドしか放せないので、待って取り、持ち、放すのは専用のスレッドにする。待ちは 1 回の
//! `WaitForSingleObject` にする（待ち直すと待っているスレッドの並びの後ろに回り、先に待ち始めたテストが後になる）。

mod desktop;

use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use windows::core::HSTRING;
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, WAIT_ABANDONED_0, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

/// 同じログオンのセッション（同じデスクトップ）の中で 1 つだけ。
const MUTEX_NAME: &str = "Local\\movie-maker-play-test";

/// 待ちの上限と、待っていることを標準エラーと結果の JSON に書く間隔。
#[derive(Clone, Copy)]
struct Timing {
    limit: Duration,
    notice: Duration,
}

/// 上限は工程の上限の合計（verify.rs。約 10 分と再生の秒数）より長くし、前の 1 本が上限いっぱいまでかかっても待ちきる。
const TIMING: Timing = Timing {
    limit: Duration::from_secs(15 * 60),
    notice: Duration::from_secs(10),
};

/// 専用のスレッドから待つ側へ送るイベント。
enum Event {
    /// すぐには取れず、待ち始めた。
    Busy,
    /// 取れた。`abandoned` は、前に持っていたスレッドが放さずに終わっていたか。
    Acquired {
        abandoned: bool,
    },
    TimedOut,
    /// ミューテックスを作れないか、待ちが失敗した。
    Failed(String),
}

/// 取ったロック。破棄すると放す。ロックなしで進めるときは何も持たない。
struct Lock {
    held: Option<(Sender<()>, JoinHandle<()>)>,
    waited: Duration,
    abandoned: bool,
}

impl Lock {
    /// ミューテックスを作れないときは、再生テストを止めずにロックなしで進める。
    fn unavailable(waited: Duration, why: &str) -> Self {
        tracing::warn!("--play-test のロックを取れない（ロックなしで進める）: {why}");
        Self {
            held: None,
            waited,
            abandoned: false,
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some((release, thread)) = self.held.take() {
            drop(release);
            let _ = thread.join();
        }
    }
}

/// 専用のスレッドの本体。取れたら `release` の送り手が破棄されるまで持ち、持ったこのスレッドから放す。
fn hold(name: &str, limit: Duration, events: &Sender<Event>, release: &Receiver<()>) {
    let name = HSTRING::from(name);
    // SAFETY: 名前は呼ぶ間だけ生きていればよく、返ったハンドルはこの関数の出口で必ず閉じる。
    let handle = match unsafe { CreateMutexW(None, false, &name) } {
        Ok(h) => h,
        Err(e) => {
            let _ = events.send(Event::Failed(format!("ミューテックスを作れない: {e}")));
            return;
        }
    };
    let event = wait(handle, limit, events);
    let held = matches!(event, Event::Acquired { .. });
    let _ = events.send(event);
    if held {
        let _ = release.recv();
        // SAFETY: `wait` で持ったミューテックスを、持ったスレッドから 1 回だけ放す。
        let _ = unsafe { ReleaseMutex(handle) };
    }
    // SAFETY: 自分で作ったハンドルを 1 回だけ閉じる。
    let _ = unsafe { CloseHandle(handle) };
}

/// 待たずに 1 回試し、取れなければ `Busy` を送ってから上限まで 1 回で待つ。
fn wait(handle: HANDLE, limit: Duration, events: &Sender<Event>) -> Event {
    // SAFETY: ハンドルは `hold` が作った生きているミューテックス。
    let mut result = unsafe { WaitForSingleObject(handle, 0) };
    if result == WAIT_TIMEOUT {
        let _ = events.send(Event::Busy);
        // u32::MAX は INFINITE なので、その 1 つ手前で止める。
        let ms = limit.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        // SAFETY: 同じハンドル。
        result = unsafe { WaitForSingleObject(handle, ms) };
    }
    match result {
        WAIT_OBJECT_0 => Event::Acquired { abandoned: false },
        WAIT_ABANDONED_0 => Event::Acquired { abandoned: true },
        WAIT_TIMEOUT => Event::TimedOut,
        other => Event::Failed(format!("待ちが失敗した: {other:?}")),
    }
}

/// 専用のスレッドの次のイベントを待つ。`notice` の間に届かなければ、`on_wait`（待った時間）を呼んで待ち続ける。
fn next_event(
    events: &Receiver<Event>,
    notice: Duration,
    start: Instant,
    on_wait: &mut impl FnMut(Duration),
) -> Event {
    loop {
        match events.recv_timeout(notice) {
            Ok(event) => return event,
            Err(RecvTimeoutError::Timeout) => on_wait(start.elapsed()),
            Err(RecvTimeoutError::Disconnected) => {
                return Event::Failed("スレッドが結果を返さずに終わった".into())
            }
        }
    }
}

/// 名前 `name` のロックを取れるまで待つ。すぐに取れなければ、待ち始めと `notice` ごとに `on_wait`（待った時間）を呼ぶ。
/// 上限を超えたら `Err(待った時間)`。
fn acquire_named(
    name: &str,
    timing: Timing,
    mut on_wait: impl FnMut(Duration),
) -> Result<Lock, Duration> {
    let start = Instant::now();
    let (events_tx, events) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let owned = name.to_owned();
    let spawned = std::thread::Builder::new()
        .name("play-test-lock".into())
        .spawn(move || hold(&owned, timing.limit, &events_tx, &release_rx));
    let thread = match spawned {
        Ok(thread) => thread,
        Err(e) => {
            let why = format!("スレッドを作れない: {e}");
            return Ok(Lock::unavailable(start.elapsed(), &why));
        }
    };
    loop {
        match next_event(&events, timing.notice, start, &mut on_wait) {
            Event::Busy => on_wait(start.elapsed()),
            Event::Acquired { abandoned } => {
                let held = Some((release, thread));
                let waited = start.elapsed();
                return Ok(Lock {
                    held,
                    waited,
                    abandoned,
                });
            }
            Event::TimedOut => {
                let _ = thread.join();
                return Err(start.elapsed());
            }
            Event::Failed(why) => {
                let _ = thread.join();
                return Ok(Lock::unavailable(start.elapsed(), &why));
            }
        }
    }
}

/// 結果の JSON に書く値と、工程が終わるまで持つロック。
struct LockState {
    lock: Option<Lock>,
    /// ロックを持って始めたか。偽ならロックなしで進めた。
    held: bool,
    waited: Duration,
    abandoned: bool,
    others_at_start: Option<usize>,
}

static STATE: Mutex<Option<LockState>> = Mutex::new(None);

/// 放すのはパニックの後にも呼ぶので、ほかのスレッドがパニックして毒になった Mutex もそのまま使う。
fn state() -> MutexGuard<'static, Option<LockState>> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// ロックを取ったときに決まる値。
fn taken_json(s: &LockState) -> Value {
    json!({
        "state": if s.held { "held" } else { "unavailable" },
        "waited_secs": s.waited.as_secs_f64(),
        "abandoned": s.abandoned,
        "other_movie_makers": { "at_start": s.others_at_start },
    })
}

/// 結果の JSON の `lock`。`--play-test` でなければ null。ほかの movie-maker のウィンドウの数を工程の後に測り直し（テストの間に
/// 開いたウィンドウを見分ける）、前面のウィンドウを持つプロセスも工程の後に測る（前面を取ったウィンドウを見分ける）。
pub fn report() -> Value {
    let guard = state();
    let Some(s) = guard.as_ref() else {
        return Value::Null;
    };
    let mut value = taken_json(s);
    value["other_movie_makers"]["at_end"] = json!(desktop::other_movie_maker_windows());
    value["foreground_at_end"] = desktop::foreground_owner();
    value
}

/// 工程が終わり、ウィンドウに入力を送らなくなったら放す。ウィンドウが閉じるのは待たない（次のテストはウィンドウを作って
/// 前面に出すまでに数秒かかり、その間に閉じ終わる）。開く前に失敗して終わるときにも呼ぶ（放さずに終わると、次のテストが
/// 放棄されたミューテックスと読む）。
pub fn release() {
    if let Some(s) = state().as_mut() {
        s.lock = None;
    }
}

fn write_report(path: &Path, value: &Value) {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    if let Err(e) = std::fs::write(path, text) {
        eprintln!("結果を書けない: {e}");
    }
}

/// 上限まで待っても取れなかった。結果の JSON に理由を書き、標準エラーに出す文を返す。
fn give_up(report_path: &Path, waited: Duration) -> String {
    let why = format!(
        "ほかの --play-test が上限の {} 分を過ぎても終わらないので、始めずに終わる",
        TIMING.limit.as_secs() / 60
    );
    let lock = json!({ "state": "timed_out", "waited_secs": waited.as_secs_f64() });
    write_report(report_path, &json!({ "failed": why, "lock": lock }));
    why
}

/// ふだんの起動は待たないので、同じデスクトップにほかの movie-maker のウィンドウがあれば、工程が失敗しうることを出す。
fn warn_other_windows(count: Option<usize>) {
    if let Some(n @ 1..) = count {
        eprintln!(
            "[play-test] 同じデスクトップにほかの movie-maker のウィンドウが {n} 個ある。\
             前面やマウスのキャプチャーを取られると、工程が失敗することがある"
        );
    }
}

/// `--play-test` の始めに、ウィンドウとプロジェクトを開く前に呼ぶ。ほかの再生テストが持っていれば終わるまで待ち、待つ間は
/// 標準エラーと結果の JSON に書く。上限を超えたら、結果の JSON に理由を書いて `Err`。
pub fn enter(report_path: &Path) -> Result<(), String> {
    let mut waited = false;
    let acquired = acquire_named(MUTEX_NAME, TIMING, |elapsed| {
        if std::mem::replace(&mut waited, true) {
            let secs = elapsed.as_secs();
            eprintln!("[play-test] ほかの --play-test の終わりを待っている: {secs} 秒");
        } else {
            let limit = TIMING.limit.as_secs() / 60;
            eprintln!("[play-test] ほかの --play-test が動いているので、終わるまで待つ（上限 {limit} 分）");
        }
        let lock = json!({ "state": "waiting", "waited_secs": elapsed.as_secs_f64() });
        write_report(report_path, &json!({ "lock": lock }));
    });
    let lock = acquired.map_err(|elapsed| give_up(report_path, elapsed))?;
    let others = desktop::other_movie_maker_windows();
    let s = LockState {
        held: lock.held.is_some(),
        waited: lock.waited,
        abandoned: lock.abandoned,
        others_at_start: others,
        lock: Some(lock),
    };
    if waited {
        eprintln!("[play-test] {} 秒待って始める", s.waited.as_secs());
        // 待つ間に書いた waiting を書き換え、テストが結果を書く前に止まっても待っていたようには残さない。
        write_report(report_path, &json!({ "lock": taken_json(&s) }));
    }
    warn_other_windows(others);
    *state() = Some(s);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const FAST: Timing = Timing {
        limit: Duration::from_millis(400),
        notice: Duration::from_millis(100),
    };

    /// 放す側のスレッドが遅れても上限に届かない長さ。
    const PATIENT: Timing = Timing {
        limit: Duration::from_secs(10),
        notice: Duration::from_millis(100),
    };

    /// テストごとに別の名前（並べて回しても、実際の再生テストのロックとも干渉しない）。
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
        assert!(notices >= 1, "待ち始めに on_wait を呼ぶ: {notices}");
        drop(first);
        assert!(acquire_named(&name, FAST, |_| {}).is_ok());
    }

    #[test]
    fn 放されるまで待ってから取る() {
        let name = unique_name();
        let first = acquire_named(&name, PATIENT, |_| {}).unwrap();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(first);
        });
        let second = acquire_named(&name, PATIENT, |_| {}).unwrap();
        releaser.join().unwrap();
        assert!(second.held.is_some());
        assert!(second.waited >= Duration::from_millis(100));
    }

    #[test]
    fn 持っていたスレッドが放さずに終わっても取れる() {
        let name = unique_name();
        let hname = HSTRING::from(name.as_str());
        // 持ったスレッドが放さずに終わると、ミューテックスは放棄される（ハンドルは開いたままにして、名前を残す）。
        std::thread::spawn(move || {
            // SAFETY: 名前は呼ぶ間だけ生きていればよく、放棄されたミューテックスを作るため、わざと放さず閉じない。
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
    fn ミューテックスを作れなければロックなしで進める() {
        // 名前空間（Local\ など）でない「\」を含む名前は作れない。
        let lock = acquire_named("mm-no-such-namespace\\lock", FAST, |_| {}).unwrap();
        assert!(lock.held.is_none());
    }
}
