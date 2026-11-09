//! パック（docs/design/13-vlog-pack.md）の Tauri のコマンド。WebView がパックのフォルダーを渡して
//! マクロの一覧を引き、マクロを展開した操作を 1 つのトランザクションとして適用する（Ctrl+Z で戻る）。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use mm_pack::{expand_for, font_files, FontCache, MacroSummary, Pack};
use mm_runtime::{Backend, RuntimeConfig};

use crate::win::edit::{submit, EditOutcome};
use crate::win::ipc::SyncState;

/// このパネルのコマンドを呼び出す。知らない名前には `false` を返す。
pub fn handler(invoke: tauri::ipc::Invoke) -> bool {
    let h: fn(tauri::ipc::Invoke) -> bool =
        tauri::generate_handler![pack_list, pack_apply, pack_transitions,];
    h(invoke)
}

/// パックの `transitions/` にあるトランジションの名前（名前の順）。
#[tauri::command]
pub async fn pack_transitions(dir: String) -> Result<Vec<String>, String> {
    let pack = Pack::load(&PathBuf::from(dir)).map_err(|e| e.to_string())?;
    Ok(pack.transitions.names().map(str::to_owned).collect())
}

/// パックのテンプレートとマクロの一覧（テンプレートが先）。
#[tauri::command]
pub async fn pack_list(dir: String) -> Result<Vec<MacroSummary>, String> {
    let pack = Pack::load(&PathBuf::from(dir)).map_err(|e| e.to_string())?;
    Ok(pack.macros.iter().map(|m| m.summary()).collect())
}

/// マクロ `name` を引数 `args`（名前 → 文字の値）で展開して適用する。
#[tauri::command]
pub async fn pack_apply(
    state: tauri::State<'_, Arc<SyncState>>,
    dir: String,
    name: String,
    args: BTreeMap<String, String>,
) -> Result<EditOutcome, String> {
    let shared = state.shared.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let pack = Pack::load(&PathBuf::from(dir)).map_err(|e| e.to_string())?;
        // プレートの大きさは、描くときと同じフォント（キャッシュにあるパックの書体）で文字を組んで測る（GPU は要らない）。
        // 取るのは `mm pack fetch-fonts` と `mm pack apply` に任せ、アプリはキャッシュに無ければ OS の書体で測る。
        let engine = mm_runtime::assemble(&RuntimeConfig {
            text: Backend::Cpu,
            ..RuntimeConfig::sample()
        })
        .map_err(|e| e.to_string())?;
        let shaper = &*engine.services().text_shaper;
        let fonts = FontCache::user()
            .map(|cache| cache.resolve(&pack.fonts))
            .unwrap_or_default();
        shaper
            .use_fonts(&font_files(&fonts))
            .map_err(|e| e.to_string())?;
        // 部品を置くコンポジションの画面の大きさ（ショートなど）を開いているプロジェクトから引く。
        let project = shared
            .doc()
            .map(|d| d.project)
            .ok_or_else(|| "No project open".to_owned())?;
        let txn = expand_for(&pack, &name, &args, shaper, &project).map_err(|e| e.to_string())?;
        submit(&shared, txn.ops)
    })
    .await
    .map_err(|e| e.to_string())?
}
