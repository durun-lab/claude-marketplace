//! OpenFX のホスト（`mm-host-ofx`）で描く `ofx` の種類。ホストの実行ファイルが無ければ登録しない。

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use mm_host_ofx::desc::BundleDesc;
use mm_host_ofx::OfxLauncher;
use mm_nodes_ofx::{Describe, OfxKind, OfxPlugin};
use mm_types::Result;

use crate::Registry;

/// 1 つのバンドルの describe の期限。プラグインの読み込みと全部の文脈の describe を含む。
const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(15);

/// `ofx` の種類を登録する。`mm-host-ofx` の実行ファイルが見つからなければ登録しない。
pub(crate) fn register(registry: &mut Registry) -> Result<()> {
    match ofx() {
        Some(kind) => registry.register(kind),
        None => Ok(()),
    }
}

fn ofx() -> Option<Arc<OfxKind>> {
    let Some(exe) = mm_host_ofx::find_host() else {
        tracing::info!("mm-host-ofx が無いので ofx の種類を登録しない");
        return None;
    };
    let describe_exe = exe.clone();
    let describe: Describe = Arc::new(move |bundle: &Path| -> Result<Vec<OfxPlugin>> {
        let desc = mm_host_ofx::describe_bundle(&describe_exe, bundle, DESCRIBE_TIMEOUT)?;
        Ok(plugins(desc))
    });
    let roots = mm_host_ofx::scan::plugin_dirs();
    Some(Arc::new(OfxKind::new(
        Arc::new(OfxLauncher),
        exe,
        describe,
        roots,
    )))
}

/// describe の結果を種類の形にする。このホストで描けない効果（`context` が無い）は除く。
/// 値を渡せる名前は、隠しのパラメータも含めて `ParamSpec` に変わるもの。
fn plugins(desc: BundleDesc) -> Vec<OfxPlugin> {
    desc.plugins
        .into_iter()
        .filter(|p| p.context.is_some())
        .map(|p| OfxPlugin {
            schema: p.param_schema(),
            settable: p
                .params
                .iter()
                .filter_map(|x| x.to_spec().map(|s| s.name))
                .collect(),
            id: p.id,
        })
        .collect()
}
