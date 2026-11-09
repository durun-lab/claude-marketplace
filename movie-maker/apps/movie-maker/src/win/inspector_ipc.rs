//! Inspector の Tauri のコマンド。種類の一覧の 1 件（パラメータの名前、型、既定値、範囲、選択肢）を引く。
//! MCP の `kinds` と同じ `Core::kinds` を通るので、組み立てに種類を登録すると Inspector の入力欄にも出る。

use std::sync::Arc;

use mm_serve::{KindInfo, KindQuery};
use mm_types::Params;

use crate::win::ipc::SyncState;

/// このパネルのコマンドを呼び出す。知らない名前には `false` を返す。
pub fn handler(invoke: tauri::ipc::Invoke) -> bool {
    let h: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![inspector_kind,];
    h(invoke)
}

/// 種類 `kind` の、種類の一覧の 1 件。`params` は `params_for` に渡すレイヤーの今の値（選んだもので一覧が変わる種類のため）。
/// 登録の無い種類（`composition` など）と、書き手として開いていないプロジェクト（JSON、HTTP サーバー無し）は失敗を返す。
#[tauri::command]
pub async fn inspector_kind(
    state: tauri::State<'_, Arc<SyncState>>,
    kind: String,
    params: Params,
) -> Result<KindInfo, String> {
    let shared = state.shared.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let core = shared
            .core
            .get()
            .ok_or_else(|| "Project is not open for writing".to_string())?;
        let query = KindQuery {
            kind: Some(kind),
            params,
        };
        let mut found = core.kinds(&query).map_err(|e| e.to_string())?;
        found
            .pop()
            .ok_or_else(|| "Kind list came back empty".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mm_runtime::Registry;
    use mm_serve::{describe_kinds, KindQuery};
    use mm_testkit::node::SolidKind;

    #[test]
    fn 定義は_ui_が読む形の_json_になる() {
        let mut registry = Registry::new();
        registry.register(Arc::new(SolidKind)).unwrap();
        let query = KindQuery {
            kind: Some("solid".into()),
            ..KindQuery::default()
        };
        let found = describe_kinds(&registry, &query).unwrap();
        let json = serde_json::to_value(&found[0]).unwrap();
        let first = &json["params"]["params"][0];
        assert!(first["name"].is_string());
        assert!(first["ty"]["type"].is_string(), "{first}");
        assert!(first["default"]["type"].is_string(), "{first}");
    }
}
