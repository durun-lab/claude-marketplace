//! Devices パネルの Tauri のコマンド。端末の登録（符号と QR）と、登録を外すのは PC のアプリからだけで、
//! 人の待ち受け（HTTP）には出さない。名前と引数の形は `ui/panels/devices/src/api.ts`。

use std::sync::Arc;

use mm_serve::DeviceInfo;
use tauri::State;

use crate::win::devices::{DevicesState, Pairing, Status};

/// このパネルのコマンドを呼び出す。知らない名前には `false` を返す。
pub fn handler(invoke: tauri::ipc::Invoke) -> bool {
    let h: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
        devices_issue_code,
        devices_list,
        devices_revoke,
        devices_status,
    ];
    h(invoke)
}

type Devices<'a> = State<'a, Arc<DevicesState>>;

async fn blocking<T: Send + 'static>(
    state: Devices<'_>,
    f: impl FnOnce(&DevicesState) -> T + Send + 'static,
) -> Result<T, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || f(&state))
        .await
        .map_err(|e| e.to_string())
}

/// 待ち受けの状態と、足りない設定の文言。
#[tauri::command]
async fn devices_status(state: Devices<'_>) -> Result<Status, String> {
    blocking(state, DevicesState::status).await
}

/// 登録の符号と QR を出す。前の符号は無効になる。
#[tauri::command]
async fn devices_issue_code(state: Devices<'_>) -> Result<Pairing, String> {
    blocking(state, DevicesState::issue_code).await?
}

/// 登録した端末（登録した順）。
#[tauri::command]
async fn devices_list(state: Devices<'_>) -> Result<Vec<DeviceInfo>, String> {
    blocking(state, DevicesState::list).await?
}

/// 端末の登録を外す。無い ID なら `false`。
#[tauri::command]
async fn devices_revoke(state: Devices<'_>, id: String) -> Result<bool, String> {
    blocking(state, move |s| s.revoke(&id)).await?
}
