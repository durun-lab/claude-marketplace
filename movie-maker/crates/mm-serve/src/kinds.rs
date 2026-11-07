//! 登録されているノードの種類と、そのパラメータの定義（MCP の `kinds`）。
//!
//! 種類を足すと組み立ての登録の 1 行だけでここに出る。パラメータは `NodeKind::params_for` で、
//! 選んだもので一覧が変わる種類（`ofx` のプラグインのパラメータなど）も、渡したパラメータの組に合わせて返す。

use mm_node_api::{InputSchema, NodeRegistry, Traits};
use mm_types::{MmError, ParamSchema, Params, Result};
use serde::Serialize;

/// 引く種類と、パラメータの一覧を決めるパラメータの組。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KindQuery {
    /// 種類の名前。`None` なら全部。
    pub kind: Option<String>,
    /// `params_for` に渡すパラメータ。空なら既定の定義（`params()` と同じ）。
    pub params: Params,
}

/// 1 つの種類の定義。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KindInfo {
    pub id: String,
    pub version: u32,
    pub traits: Traits,
    pub inputs: InputSchema,
    pub params: ParamSchema,
}

/// `registry` から `query` の種類の定義を名前の順で返す。名前の無い種類は `NotFound`。
pub fn describe_kinds(registry: &dyn NodeRegistry, query: &KindQuery) -> Result<Vec<KindInfo>> {
    let ids: Vec<String> = match &query.kind {
        Some(id) => vec![id.clone()],
        None => registry.kinds().into_iter().map(str::to_owned).collect(),
    };
    ids.iter()
        .map(|id| {
            let kind = registry
                .get(id)
                .ok_or_else(|| MmError::NotFound(format!("種類 {id} は登録されていない")))?;
            Ok(KindInfo {
                id: kind.id().to_owned(),
                version: kind.version(),
                traits: kind.traits(),
                inputs: kind.inputs(),
                params: kind.params_for(&query.params),
            })
        })
        .collect()
}
