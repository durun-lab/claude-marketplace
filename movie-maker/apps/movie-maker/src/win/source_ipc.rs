//! Source のビューアーを WebView につなぐ Tauri のコマンド。WebView の橋は、Media と Story が選んだ素材を `source_open`
//! で開く。再生とイン点・アウト点はサーフェスの再生の操作のバーとキーとメニューが持つ。

use std::sync::Arc;

use tauri::State;

use crate::win::ipc::SyncState;

/// このパネルのコマンドを呼び出す。知らない名前には `false` を返す。
pub fn handler(invoke: tauri::ipc::Invoke) -> bool {
    let h: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![source_open];
    h(invoke)
}

/// 素材を Source で開く。Media で選んだ素材の ID を渡す。
#[tauri::command]
pub async fn source_open(state: State<'_, Arc<SyncState>>, asset: String) -> Result<(), String> {
    let shared = state.shared.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let doc = shared.doc().ok_or("No project open")?;
        let comp = doc
            .project
            .composition(&doc.comp)
            .map_err(|e| e.to_string())?;
        shared.source.open(&doc.project, comp, &asset)
    })
    .await
    .map_err(|e| e.to_string())?
}
