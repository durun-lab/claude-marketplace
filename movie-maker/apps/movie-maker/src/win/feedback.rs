//! Feedback パネルのアプリ側。自動の改善（`mm_improve::Improver`）を 1 つ持ち、起動時に回を起こす契機を与え、
//! アプリの panic、画面の例外、予算を超えたスパン、ホストのクラッシュを自動の起票に回す。
//! 振る舞いの詳しくは `ui/panels/feedback/README.md`、API は docs/design/10-review-improve.md の「mm-improve の作り」。

mod watch;

#[cfg(test)]
mod tests;

use std::backtrace::Backtrace;
use std::cell::Cell;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};

use mm_improve::{
    ErrorInput, ErrorSource, FeedbackItem, ImproveConfig, ImproveStatus, Improver, ListFilter,
    NewFeedback, SettingsPatch,
};
use serde_json::{json, Value};

pub use watch::watch_layer;

/// 起票、設定の変更、回の終わりを Feedback パネルへ知らせる Tauri のイベント。中身は無い。
pub const FEEDBACK_CHANGED_EVENT: &str = "mm:feedback-changed";
/// 回を動かす作業ツリーを変える環境変数。既定はこのアプリをビルドした movie-maker の作業ツリー。
const REPO_ENV: &str = "MM_IMPROVE_REPO";
/// 画面の部品を指したときのスクリーンショットの置き場所（`data_dir` からの相対）。
const ATTACHMENTS_DIR: &str = "attachments";

static SERVICE: OnceLock<Arc<Feedback>> = OnceLock::new();

thread_local! {
    /// panic の記録の中で起きた panic をもう一度記録しないためのフラグ。
    static IN_PANIC_HOOK: Cell<bool> = const { Cell::new(false) };
}

/// 変化を知らせる先。アプリでは Tauri のイベント、テストでは数える。
pub type Notify = Arc<dyn Fn() + Send + Sync>;

/// 自動の改善のサービス。IPC のコマンド、panic の記録、計装の見張りが共有する。
pub struct Feedback {
    improver: Improver,
    /// 送られた要望の `context.app` に添えるもの（開いているプロジェクトなど）。
    app_context: Value,
    /// 自動の起票は描画のスレッドからも来るので、ファイルの読み書きはバックグラウンドのスレッドで順に行う。
    reports: Sender<ErrorInput>,
    /// 回を起こしうる契機。バックグラウンドのスレッドが回の終わりまで待ってから知らせる。
    kicks: Sender<()>,
    notify: Arc<OnceLock<Notify>>,
}

impl Feedback {
    /// サービスを作り、自動の起票と回の終わりを待つスレッドを 1 本ずつ起こす。サービスを破棄するとスレッドも終わる。
    pub fn new(improver: Improver, app_context: Value) -> Arc<Self> {
        let (reports, report_rx) = mpsc::channel();
        let (kicks, kick_rx) = mpsc::channel();
        let notify = Arc::new(OnceLock::new());
        spawn_reporter(improver.clone(), report_rx, kicks.clone());
        spawn_watcher(improver.clone(), kick_rx, notify.clone());
        Arc::new(Self {
            improver,
            app_context,
            reports,
            kicks,
            notify,
        })
    }

    /// 変化を知らせる先を決める（1 回だけ効く）。
    pub fn set_notify(&self, notify: Notify) {
        let _ = self.notify.set(notify);
    }

    /// 回を起こす契機を与え、回が終わったらパネルに知らせる。
    pub fn kick(&self) {
        self.improver.kick();
        let _ = self.kicks.send(());
    }

    /// 要望を起票する。`context.app` にアプリの状態を添える。
    pub fn submit(&self, mut new: NewFeedback) -> Result<FeedbackItem, String> {
        let mut context = match std::mem::take(&mut new.context) {
            Value::Object(map) => map,
            Value::Null => serde_json::Map::new(),
            other => serde_json::Map::from_iter([("panel".to_owned(), other)]),
        };
        context.insert("app".into(), self.app_context.clone());
        new.context = Value::Object(context);
        let item = self.improver.submit(new).map_err(text)?;
        let _ = self.kicks.send(());
        Ok(item)
    }

    /// 要望の一覧（新しい順）。
    pub fn list(&self, filter: ListFilter) -> Result<Vec<FeedbackItem>, String> {
        self.improver.list(filter).map_err(text)
    }

    /// 要望を 1 件。
    pub fn get(&self, id: &str) -> Result<FeedbackItem, String> {
        self.improver.get(id).map_err(text)
    }

    /// 状態。
    pub fn status(&self) -> Result<ImproveStatus, String> {
        self.improver.status().map_err(text)
    }

    /// 設定を変える（ON / OFF、予算）。
    pub fn update_settings(&self, patch: &SettingsPatch) -> Result<ImproveStatus, String> {
        let status = self.improver.update_settings(patch).map_err(text)?;
        let _ = self.kicks.send(());
        Ok(status)
    }

    /// すぐ止める。
    pub fn stop(&self) -> Result<ImproveStatus, String> {
        let status = self.improver.stop().map_err(text)?;
        let _ = self.kicks.send(());
        Ok(status)
    }

    /// エラーか遅い処理をバックグラウンドで記録する。呼んだスレッドを待たせない。
    pub fn report(&self, input: ErrorInput) {
        let _ = self.reports.send(input);
    }

    /// その場で記録する（panic のように、すぐプロセスが終わりうるとき）。
    fn record_now(&self, input: &ErrorInput) {
        if let Ok(Some(_)) = self.improver.record(input) {
            let _ = self.kicks.send(());
        }
    }

    /// スクリーンショットの置き場所。名前は時刻から作る。
    pub fn attachment_path(&self) -> Result<PathBuf, String> {
        let dir = self.improver.config().data_dir.join(ATTACHMENTS_DIR);
        std::fs::create_dir_all(&dir).map_err(text)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(text)?
            .as_millis();
        Ok(dir.join(format!("{stamp}.png")))
    }
}

fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn spawn_reporter(improver: Improver, rx: Receiver<ErrorInput>, kicks: Sender<()>) {
    std::thread::spawn(move || {
        for input in rx {
            match improver.record(&input) {
                Ok(Some(_)) => drop(kicks.send(())),
                Ok(None) => {}
                Err(e) => tracing::warn!("自動の起票を記録できない: {e}"),
            }
        }
    });
}

/// 契機ごとにバックグラウンドの実行が止まるまで待ち、知らせる。待つ間に来た契機は、終わった後にもう一度待つので取りこぼさない。
fn spawn_watcher(improver: Improver, rx: Receiver<()>, notify: Arc<OnceLock<Notify>>) {
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            if let Some(n) = notify.get() {
                n();
            }
            while rx.try_recv().is_ok() {}
            improver.wait_idle();
            if let Some(n) = notify.get() {
                n();
            }
        }
    });
}

/// アプリのサービス。起動で作れなかったら `None`。
pub fn service() -> Option<Arc<Feedback>> {
    SERVICE.get().cloned()
}

/// 起動時に呼ぶ。サービスを作り、panic を記録するフックを入れ、回を起こす契機を与える。
/// 作業ツリーが見つからなければ自動の改善は使わない（パネルは理由を出す）。
pub fn start(project: &Path) {
    let improver = match standard_improver() {
        Ok(imp) => imp,
        Err(e) => {
            tracing::warn!("自動の改善を使えない: {e}");
            return;
        }
    };
    let context = json!({ "project": project.to_string_lossy() });
    let svc = Feedback::new(improver, context);
    if SERVICE.set(svc.clone()).is_err() {
        return;
    }
    install_panic_hook(svc.clone());
    svc.kick();
}

/// ウィンドウができたら、変化を WebView へ知らせる。
pub fn connect(app: tauri::AppHandle) {
    use tauri::Emitter;
    if let Some(svc) = service() {
        svc.set_notify(Arc::new(move || {
            let _ = app.emit(FEEDBACK_CHANGED_EVENT, ());
        }));
    }
}

/// `mm improve` と同じ組み立て（[`Improver::standard`]）で、バックグラウンドの実行を有効にする。
fn standard_improver() -> mm_types::Result<Improver> {
    let data_dir = ImproveConfig::default_data_dir()?;
    let start = std::env::var_os(REPO_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let cfg = ImproveConfig::discover(data_dir, &start)?;
    Ok(Improver::standard(cfg, |_| {}).with_background())
}

fn install_panic_hook(svc: Arc<Feedback>) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !IN_PANIC_HOOK.replace(true) {
            svc.record_now(&panic_input(info));
            IN_PANIC_HOOK.set(false);
        }
        prev(info);
    }));
}

/// panic の中身を自動の起票の形にする。
fn panic_input(info: &PanicHookInfo<'_>) -> ErrorInput {
    let mut input = ErrorInput::error(ErrorSource::App, panic_message(info.payload()));
    input.where_ = info
        .location()
        .map(|l| format!("{}:{}", l.file(), l.line()));
    input.stack = Some(Backtrace::force_capture().to_string());
    input
}

/// panic の payload の文。`panic!` は `&str` か `String` を載せる。
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic（中身は文字列でない）".into())
}

/// WebView の例外を記録する。出どころは画面に決め、WebView が名乗る `source` は使わない。
pub fn report_webview(mut input: ErrorInput) {
    input.source = ErrorSource::Webview;
    if let Some(svc) = service() {
        svc.report(input);
    }
}
