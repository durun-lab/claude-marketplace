//! 開発のときだけ、動いているアプリの Rust の関数を差し替える（`hotpatch` feature と `dx serve --hotpatch`）。
//!
//! 差し替えは [`call`] の中でだけ効く。操作の適用と IPC のハンドラーの本体を [`call`] で囲む。
//! 入力中と書き出し中に届いたパッチは当てずに持ち、ウィンドウの題に印を出して、両方が終わったときに当てる。
//! feature を切ったビルドでは [`call`] は渡した関数をそのまま呼び、ほかは何もしない。

#[cfg(feature = "hotpatch")]
pub use live::{call, set_input, start};

#[cfg(not(feature = "hotpatch"))]
pub use off::{call, set_input, start};

/// パッチを待たせる理由。
#[cfg(any(feature = "hotpatch", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    /// サーフェスでボタンを押している間（ドラッグの途中）。
    Input,
    /// 書き出しのキューが動いている間。
    Export,
}

/// 当てるか待たせるかの決め。パッチの型を引数にして、feature を切った試験でも確かめる。
#[cfg(any(feature = "hotpatch", test))]
#[derive(Debug)]
pub struct Gate<T> {
    input: bool,
    export: bool,
    pending: Option<T>,
}

#[cfg(any(feature = "hotpatch", test))]
impl<T> Gate<T> {
    /// 何も待っていない状態。
    pub const fn new() -> Self {
        Self {
            input: false,
            export: false,
            pending: None,
        }
    }

    /// 入力中でも書き出し中でもないか。
    pub fn idle(&self) -> bool {
        !self.input && !self.export
    }

    /// 待たせているパッチがあるか（印を出すか）。
    pub fn held(&self) -> bool {
        self.pending.is_some()
    }

    /// 届いたパッチを受ける。いま当ててよければ返し、だめなら持つ。
    /// dx のパッチは起動したときのバイナリとの差分なので、持つのは最新の 1 つでよい。
    pub fn offer(&mut self, patch: T) -> Option<T> {
        if self.idle() {
            self.pending = None;
            return Some(patch);
        }
        self.pending = Some(patch);
        None
    }

    /// 理由の始まりと終わりを受ける。両方が終わって待たせていたパッチがあれば返す。
    pub fn set(&mut self, why: Busy, busy: bool) -> Option<T> {
        match why {
            Busy::Input => self.input = busy,
            Busy::Export => self.export = busy,
        }
        if self.idle() {
            self.pending.take()
        } else {
            None
        }
    }
}

#[cfg(feature = "hotpatch")]
mod live {
    use std::sync::Mutex;

    use dioxus_devtools::{subsecond, DevserverMsg};
    use tauri::{AppHandle, Listener, Manager};

    use super::{Busy, Gate};

    /// 書き出しのキューの状態のイベント。`deliver_ipc.rs` と `ui/sdk` の `EXPORT_PROGRESS_EVENT` と同じ名前。
    const EXPORT_PROGRESS_EVENT: &str = "mm:export-progress";
    /// 印を出さないときのウィンドウの題。`app.rs` の `create_window` と同じ。
    const TITLE: &str = "movie-maker";
    /// パッチを待たせている間のウィンドウの題。
    const TITLE_HELD: &str = "movie-maker（Rust の更新あり。入力と書き出しの後に当てる）";

    static GATE: Mutex<Gate<subsecond::JumpTable>> = Mutex::new(Gate::new());
    static APP: Mutex<Option<AppHandle>> = Mutex::new(None);

    /// `f` を差し替えの入口として呼ぶ。パッチが当たった後は新しい関数に入る。
    pub fn call<O>(f: impl FnOnce() -> O) -> O {
        // subsecond はポインタ 1 本分の大きさのクロージャを関数ポインタとみなし、差し替えを素通りする。
        // 2 本分の詰め物を一緒に捕まえ、どのクロージャもその大きさにならないようにする。
        let pad = [0usize; 2];
        // subsecond::call が FnMut を取るのは HotFnPanic で呼び直すため。0.7 はそのパニックを出さないので 1 回で済む。
        let mut f = Some(f);
        subsecond::call(move || {
            std::hint::black_box(&pad);
            let f = f.take().expect("差し替えの入口の関数は 1 回だけ呼ぶ");
            f()
        })
    }

    /// dx の開発サーバーにつなぎ、書き出しのイベントを聞き始める。dx から起動していなければ何もつながらない。
    pub fn start(app: &AppHandle) {
        *APP.lock().expect("hotpatch app") = Some(app.clone());
        app.listen(EXPORT_PROGRESS_EVENT, |event| {
            let running = serde_json::from_str::<serde_json::Value>(event.payload())
                .ok()
                .and_then(|v| v.get("running").and_then(serde_json::Value::as_bool));
            if let Some(running) = running {
                set_busy(Busy::Export, running);
            }
        });
        dioxus_devtools::connect(|msg| {
            let DevserverMsg::HotReload(msg) = msg else {
                return;
            };
            let ours = msg.for_pid == Some(std::process::id());
            if let Some(table) = msg.jump_table.filter(|_| ours) {
                let (ready, held) = with_gate(|gate| (gate.offer(table), gate.held()));
                ready.map_or_else(|| mark(held), apply);
            }
        });
        tracing::info!("dx の開発サーバーにつないだ（Rust の関数の差し替えを受ける）");
    }

    /// サーフェスのドラッグの始まり（真）と終わり（偽）を知らせる。
    pub fn set_input(busy: bool) {
        set_busy(Busy::Input, busy);
    }

    /// 入力や書き出しの始まりと終わりを受ける。両方が終わったら、待たせていたパッチを当てる。
    fn set_busy(why: Busy, busy: bool) {
        if let Some(table) = with_gate(|gate| gate.set(why, busy)) {
            apply(table);
        }
    }

    fn with_gate<R>(f: impl FnOnce(&mut Gate<subsecond::JumpTable>) -> R) -> R {
        f(&mut GATE.lock().expect("hotpatch gate"))
    }

    fn apply(table: subsecond::JumpTable) {
        // SAFETY: dx がこのプロセス（for_pid）のために作ったジャンプ表で、起動したバイナリと同じ ASLR の基準で引く。
        match unsafe { subsecond::apply_patch(table) } {
            Ok(()) => tracing::info!("Rust の関数を差し替えた"),
            Err(e) => tracing::warn!(error = ?e, "Rust の関数を差し替えられない"),
        }
        mark(false);
    }

    fn mark(held: bool) {
        let window = APP
            .lock()
            .expect("hotpatch app")
            .as_ref()
            .and_then(|app| app.get_window("main"));
        if let Some(window) = window {
            let _ = window.set_title(if held { TITLE_HELD } else { TITLE });
        }
    }
}

#[cfg(not(feature = "hotpatch"))]
mod off {
    /// `f` をそのまま呼ぶ。
    pub fn call<O>(f: impl FnOnce() -> O) -> O {
        f()
    }

    /// 何もしない。
    pub fn start(_app: &tauri::AppHandle) {}

    /// 何もしない。
    pub fn set_input(_busy: bool) {}
}

#[cfg(test)]
mod tests;
