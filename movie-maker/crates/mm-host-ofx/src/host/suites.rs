//! ホストの構造体（`OfxHost`）と `fetchSuite`、Memory・MultiThread・Message の Suite。
//! ホストのプロパティは「GUI の無い書き出し用のホスト」（`kOfxImageEffectHostPropIsBackground`）として名乗る。

use std::cell::Cell;
use std::ffi::{c_char, c_int, c_uint, c_void};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread::ThreadId;

use super::ffi::*;
use super::names::*;
use super::props::{read_str, PropSet};
use super::{effect, param_suite, props};

/// ホストの名前（`kOfxPropName`）。
const HOST_NAME: &str = "movie-maker";
/// 名乗る OpenFX の API のバージョン（1.4。1.5 の新しい suite はまだ無い）。
const API_VERSION: [c_int; 2] = [1, 4];

static MEMORY: MemorySuiteV1 = MemorySuiteV1 {
    alloc: memory_alloc,
    free: memory_free,
};
static MULTI_THREAD: MultiThreadSuiteV1 = MultiThreadSuiteV1 {
    multi_thread,
    num_cpus,
    index: thread_index,
    is_spawned_thread,
    mutex_create,
    mutex_destroy,
    mutex_lock,
    mutex_unlock,
    mutex_try_lock,
};
static MESSAGE: MessageSuiteV2 = MessageSuiteV2 {
    message: mm_ofx_message_v,
    set_persistent_message: mm_ofx_message_v,
    clear_persistent_message,
};

/// プラグインが最後に出したエラーの文（描画の失敗の理由に添える）。
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// プロセスで 1 つのホストの構造体。プラグインはポインタを持ち続けるので解放しない。
pub fn host() -> *mut OfxHost {
    static HOST: OnceLock<usize> = OnceLock::new();
    let addr = *HOST.get_or_init(|| {
        let props: &'static PropSet = Box::leak(Box::new(host_props()));
        let host = Box::leak(Box::new(OfxHost {
            host: props.handle(),
            fetch_suite,
        }));
        host as *mut OfxHost as usize
    });
    addr as *mut OfxHost
}

fn host_props() -> PropSet {
    let p = PropSet::default();
    p.set_str(PROP_TYPE, &[TYPE_HOST]);
    p.set_str(PROP_NAME, &[HOST_NAME]);
    p.set_str(PROP_LABEL, &[HOST_NAME]);
    p.set_int(PROP_API_VERSION, &API_VERSION);
    p.set_int(PROP_VERSION, &[0, 1, 0]);
    p.set_str(PROP_VERSION_LABEL, &[env!("CARGO_PKG_VERSION")]);
    p.set_int(FX_HOST_IS_BACKGROUND, &[1]);
    p.set_str(FX_HOST_NATIVE_ORIGIN, &[ORIGIN_BOTTOM_LEFT]);
    p.set_str(FX_SUPPORTED_CONTEXTS, &[CONTEXT_FILTER, CONTEXT_GENERATOR]);
    p.set_str(FX_SUPPORTED_DEPTHS, &[DEPTH_FLOAT, DEPTH_SHORT, DEPTH_BYTE]);
    p.set_str(FX_SUPPORTED_COMPONENTS, &[COMPONENT_RGBA]);
    p.set_str(FX_RENDER_THREAD_SAFETY, &[RENDER_INSTANCE_SAFE]);
    let zero = [
        FX_SUPPORTS_OVERLAYS,
        FX_SUPPORTS_TILES,
        FX_TEMPORAL_CLIP_ACCESS,
        FX_MULTIPLE_CLIP_DEPTHS,
        FX_MULTIPLE_CLIP_PARS,
        FX_SETABLE_FRAME_RATE,
        FX_SETABLE_FIELDING,
        FX_SEQUENTIAL_RENDER,
        HOST_SUPPORTS_CUSTOM_INTERACT,
        HOST_SUPPORTS_STRING_ANIMATION,
        HOST_SUPPORTS_CHOICE_ANIMATION,
        HOST_SUPPORTS_BOOLEAN_ANIMATION,
        HOST_SUPPORTS_CUSTOM_ANIMATION,
        HOST_MAX_PAGES,
    ];
    for key in zero {
        p.set_int(key, &[0]);
    }
    p.set_int(FX_SUPPORTS_MULTI_RESOLUTION, &[1]);
    p.set_int(HOST_MAX_PARAMETERS, &[-1]);
    p
}

unsafe extern "C" fn fetch_suite(
    _host: Handle,
    name: *const c_char,
    version: c_int,
) -> *const c_void {
    let name = unsafe { read_str(name) };
    let suite: *const c_void = match (name.as_str(), version) {
        (SUITE_PROPERTY, 1) => std::ptr::from_ref(&props::SUITE).cast(),
        (SUITE_IMAGE_EFFECT, 1) => std::ptr::from_ref(&effect::SUITE).cast(),
        (SUITE_PARAMETER, 1) => std::ptr::from_ref(&param_suite::SUITE).cast(),
        (SUITE_MEMORY, 1) => std::ptr::from_ref(&MEMORY).cast(),
        (SUITE_MULTI_THREAD, 1) => std::ptr::from_ref(&MULTI_THREAD).cast(),
        (SUITE_MESSAGE, 1 | 2) => std::ptr::from_ref(&MESSAGE).cast(),
        _ => std::ptr::null(),
    };
    suite
}

/// プラグインが最後に出したエラーの文を取り出す。
pub fn take_last_error() -> Option<String> {
    LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// `csrc/shim.c` から: 書式を展開したメッセージ。応答の JSON を汚さないよう標準エラーに出す。
#[no_mangle]
unsafe extern "C" fn mm_ofx_message(
    _h: *mut c_void,
    ty: *const c_char,
    _id: *const c_char,
    text: *const c_char,
) -> OfxStatus {
    let (ty, text) = unsafe { (read_str(ty), read_str(text)) };
    eprintln!("mm-host-ofx: [{ty}] {text}");
    if ty == MESSAGE_ERROR || ty == MESSAGE_FATAL {
        *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = Some(text);
    }
    STAT_OK
}

unsafe extern "C" fn clear_persistent_message(_h: *mut c_void) -> OfxStatus {
    STAT_OK
}

extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(p: *mut c_void);
}

unsafe extern "C" fn memory_alloc(
    _h: *mut c_void,
    bytes: usize,
    out: *mut *mut c_void,
) -> OfxStatus {
    if out.is_null() {
        return STAT_ERR_BAD_HANDLE;
    }
    // SAFETY: C の malloc。解放は memory_free の free。
    let p = unsafe { malloc(bytes.max(1)) };
    unsafe { out.write(p) };
    if p.is_null() {
        STAT_ERR_MEMORY
    } else {
        STAT_OK
    }
}

unsafe extern "C" fn memory_free(p: *mut c_void) -> OfxStatus {
    // SAFETY: memory_alloc が malloc で返したポインタ（NULL も free できる）。
    unsafe { free(p) };
    STAT_OK
}

thread_local! {
    /// multiThread の関数を実行している番号。その外では `None`。
    static THREAD_INDEX: Cell<Option<c_uint>> = const { Cell::new(None) };
}

/// 使える CPU の数（1 以上）。
fn cpu_count() -> c_uint {
    std::thread::available_parallelism()
        .map_or(1, |n| c_uint::try_from(n.get()).unwrap_or(c_uint::MAX))
}

/// プラグインが頼む数に依らず、CPU の数までのスレッドで `0..n` の番号を分け合う（OpenFX の multiThread は
/// 各番号で `func` が 1 回呼ばれればよく、スレッドの数を CPU の数に絞ってよい）。
unsafe extern "C" fn multi_thread(
    func: Option<ThreadFunction>,
    n: c_uint,
    arg: *mut c_void,
) -> OfxStatus {
    let Some(func) = func else {
        return STAT_ERR_BAD_HANDLE;
    };
    let arg = arg as usize;
    let next = &std::sync::atomic::AtomicU32::new(0);
    let work = move || {
        let saved = THREAD_INDEX.with(Cell::get);
        loop {
            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if i >= n {
                break;
            }
            THREAD_INDEX.with(|c| c.set(Some(i)));
            // SAFETY: プラグインの関数に、プラグインが渡した引数をそのまま返す。
            unsafe { func(i, n, arg as *mut c_void) };
        }
        THREAD_INDEX.with(|c| c.set(saved));
    };
    std::thread::scope(|s| {
        for _ in 1..n.min(cpu_count()) {
            // 起こせなければ、残りの番号は呼んだスレッドが実行する。
            if std::thread::Builder::new().spawn_scoped(s, work).is_err() {
                break;
            }
        }
        work();
    });
    STAT_OK
}

unsafe extern "C" fn num_cpus(out: *mut c_uint) -> OfxStatus {
    if !out.is_null() {
        unsafe { out.write(cpu_count()) };
    }
    STAT_OK
}

unsafe extern "C" fn thread_index(out: *mut c_uint) -> OfxStatus {
    if !out.is_null() {
        unsafe { out.write(THREAD_INDEX.with(Cell::get).unwrap_or(0)) };
    }
    STAT_OK
}

unsafe extern "C" fn is_spawned_thread() -> c_int {
    c_int::from(THREAD_INDEX.with(Cell::get).is_some())
}

/// 同じスレッドなら重ねて取れる mutex（OpenFX の mutex は再帰的）。
#[derive(Default)]
struct RecursiveMutex {
    state: Mutex<(Option<ThreadId>, c_int)>,
    released: Condvar,
}

impl RecursiveMutex {
    fn acquire(&self, wait: bool) -> bool {
        let me = std::thread::current().id();
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while st.0.is_some_and(|owner| owner != me) {
            if !wait {
                return false;
            }
            st = self.released.wait(st).unwrap_or_else(|e| e.into_inner());
        }
        *st = (Some(me), st.1 + 1);
        true
    }

    fn release(&self) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.1 = (st.1 - 1).max(0);
        if st.1 == 0 {
            st.0 = None;
            self.released.notify_one();
        }
    }
}

unsafe fn mutex<'a>(h: Handle) -> Option<&'a RecursiveMutex> {
    // SAFETY: mutex_create が Box::into_raw で渡したハンドル。
    unsafe { h.cast::<RecursiveMutex>().as_ref() }
}

unsafe extern "C" fn mutex_create(out: *mut Handle, lock_count: c_int) -> OfxStatus {
    if out.is_null() {
        return STAT_ERR_BAD_HANDLE;
    }
    let m = RecursiveMutex::default();
    for _ in 0..lock_count.max(0) {
        m.acquire(true);
    }
    unsafe { out.write(Box::into_raw(Box::new(m)).cast()) };
    STAT_OK
}

unsafe extern "C" fn mutex_destroy(h: Handle) -> OfxStatus {
    if h.is_null() {
        return STAT_ERR_BAD_HANDLE;
    }
    // SAFETY: mutex_create が Box::into_raw で渡したハンドルで、プラグインは 1 度だけ壊す。
    drop(unsafe { Box::from_raw(h.cast::<RecursiveMutex>()) });
    STAT_OK
}

unsafe extern "C" fn mutex_lock(h: Handle) -> OfxStatus {
    match unsafe { mutex(h) } {
        Some(m) => {
            m.acquire(true);
            STAT_OK
        }
        None => STAT_ERR_BAD_HANDLE,
    }
}

unsafe extern "C" fn mutex_unlock(h: Handle) -> OfxStatus {
    match unsafe { mutex(h) } {
        Some(m) => {
            m.release();
            STAT_OK
        }
        None => STAT_ERR_BAD_HANDLE,
    }
}

unsafe extern "C" fn mutex_try_lock(h: Handle) -> OfxStatus {
    match unsafe { mutex(h) } {
        Some(m) if m.acquire(false) => STAT_OK,
        Some(_) => STAT_FAILED,
        None => STAT_ERR_BAD_HANDLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn 知っている_suite_だけを返す() {
        let host = host();
        // SAFETY: host() はプロセスの間ずっと生きている。
        let fetch = unsafe { (*host).fetch_suite };
        unsafe {
            assert!(!fetch(std::ptr::null_mut(), c"OfxPropertySuite".as_ptr(), 1).is_null());
            assert!(!fetch(std::ptr::null_mut(), c"OfxMessageSuite".as_ptr(), 2).is_null());
            assert!(fetch(std::ptr::null_mut(), c"OfxPropertySuite".as_ptr(), 2).is_null());
            assert!(fetch(std::ptr::null_mut(), c"OfxOpenGLRenderSuite".as_ptr(), 1).is_null());
        }
    }

    unsafe extern "C" fn count(i: c_uint, n: c_uint, arg: *mut c_void) {
        assert!(i < n);
        let hits = unsafe { &*(arg as *const AtomicU32) };
        hits.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn multi_thread_は全部の番号で呼ぶ() {
        let hits = AtomicU32::new(0);
        let st = unsafe { multi_thread(Some(count), 4, &hits as *const AtomicU32 as *mut c_void) };
        assert_eq!(st, STAT_OK);
        assert_eq!(hits.load(Ordering::SeqCst), 4);
    }

    /// 番号ごとの呼ばれた回数と、呼ばれたスレッド。
    struct Calls {
        hits: Vec<AtomicU32>,
        threads: Mutex<std::collections::HashSet<ThreadId>>,
    }

    unsafe extern "C" fn record(i: c_uint, _n: c_uint, arg: *mut c_void) {
        let calls = unsafe { &*(arg as *const Calls) };
        calls.hits[i as usize].fetch_add(1, Ordering::SeqCst);
        let mut threads = calls.threads.lock().unwrap();
        threads.insert(std::thread::current().id());
    }

    #[test]
    fn multi_thread_は多い番号を_cpu_の数までのスレッドで_1_回ずつ呼ぶ() {
        const MANY: c_uint = 10_000;
        let calls = Calls {
            hits: (0..MANY).map(|_| AtomicU32::new(0)).collect(),
            threads: Mutex::default(),
        };
        let arg = &calls as *const Calls as *mut c_void;
        assert_eq!(unsafe { multi_thread(Some(record), MANY, arg) }, STAT_OK);
        assert!(calls.hits.iter().all(|h| h.load(Ordering::SeqCst) == 1));
        assert!(calls.threads.lock().unwrap().len() <= cpu_count() as usize);
        assert_eq!(
            unsafe { is_spawned_thread() },
            0,
            "呼んだスレッドの番号は元に戻す"
        );
    }

    #[test]
    fn mutex_は同じスレッドで重ねて取れる() {
        let mut h: Handle = std::ptr::null_mut();
        unsafe {
            assert_eq!(mutex_create(&mut h, 0), STAT_OK);
            assert_eq!(mutex_lock(h), STAT_OK);
            assert_eq!(mutex_try_lock(h), STAT_OK);
            let other = h as usize;
            let busy = std::thread::spawn(move || mutex_try_lock(other as Handle))
                .join()
                .unwrap();
            assert_eq!(busy, STAT_FAILED, "ほかのスレッドは取れない");
            assert_eq!(mutex_unlock(h), STAT_OK);
            assert_eq!(mutex_unlock(h), STAT_OK);
            assert_eq!(mutex_destroy(h), STAT_OK);
        }
    }
}
