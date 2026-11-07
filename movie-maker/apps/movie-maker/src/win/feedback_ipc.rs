//! Feedback パネルの Tauri のコマンド。名前と引数の形は `ui/sdk/README.md` の「Feedback のコマンド」。
//! 中身は `mm_improve::Improver` の公開 API で、JSON は camelCase。

use std::sync::Arc;

use mm_improve::{ErrorInput, FeedbackItem, ImproveStatus, ListFilter, NewFeedback, SettingsPatch};

use crate::win::capture;
use crate::win::feedback::{self, Feedback};

/// このパネルのコマンドを呼び出す。知らない名前には `false` を返す。
pub fn handler(invoke: tauri::ipc::Invoke) -> bool {
    let h: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
        feedback_capture,
        feedback_get,
        feedback_list,
        feedback_report_error,
        feedback_settings,
        feedback_status,
        feedback_stop,
        feedback_submit,
    ];
    h(invoke)
}

fn service() -> Result<Arc<Feedback>, String> {
    feedback::service().ok_or_else(|| {
        "自動の改善を使えない（movie-maker の git の作業ツリーが見つからない。MM_IMPROVE_REPO で指せる）"
            .into()
    })
}

/// 要望の読み書きはファイルと git に触れるので、IPC のスレッドを塞がない。
async fn blocking<T: Send + 'static>(
    f: impl FnOnce(&Feedback) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let svc = service()?;
    tauri::async_runtime::spawn_blocking(move || f(&svc))
        .await
        .map_err(|e| e.to_string())?
}

/// 状態（設定、動いている回、未対応の数、待機の理由、最後の回）。
#[tauri::command]
pub async fn feedback_status() -> Result<ImproveStatus, String> {
    blocking(|s| s.status()).await
}

/// 要望の一覧（新しい順、返事つき）。`filter` の既定は全部。
#[tauri::command]
pub async fn feedback_list(filter: Option<ListFilter>) -> Result<Vec<FeedbackItem>, String> {
    blocking(move |s| s.list(filter.unwrap_or_default())).await
}

/// 要望を 1 件。
#[tauri::command]
pub async fn feedback_get(id: String) -> Result<FeedbackItem, String> {
    blocking(move |s| s.get(&id)).await
}

/// 要望を起票して回を起こす。
#[tauri::command]
pub async fn feedback_submit(feedback: NewFeedback) -> Result<FeedbackItem, String> {
    blocking(move |s| s.submit(feedback)).await
}

/// 設定を変える（ON / OFF、予算、時間の上限）。
#[tauri::command]
pub async fn feedback_settings(patch: SettingsPatch) -> Result<ImproveStatus, String> {
    blocking(move |s| s.update_settings(&patch)).await
}

/// すぐ止める（OFF にし、動いている回を木ごと止める）。
#[tauri::command]
pub async fn feedback_stop() -> Result<ImproveStatus, String> {
    blocking(|s| s.stop()).await
}

/// 画面の例外（`window.onerror` と `unhandledrejection`）を自動の起票に回す。記録は裏で行う。
#[tauri::command]
pub fn feedback_report_error(error: ErrorInput) {
    feedback::report_webview(error);
}

/// ウィンドウのスクリーンショットを撮って、置いたパスを返す。指した部品に添える。
#[tauri::command]
pub async fn feedback_capture(window: tauri::Window) -> Result<String, String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
    blocking(move |s| {
        let path = s.attachment_path()?;
        capture::capture_window(hwnd, &path)?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
}
