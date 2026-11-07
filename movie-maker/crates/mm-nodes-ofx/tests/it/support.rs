//! 試験の部品。ホストは `mm-host-ofx` の実行ファイル（`target/<profile>/mm-host-ofx`）、プラグインは `mm-host-ofx` の
//! `build.rs` が作る試験のプラグイン。実行ファイルが無ければスキップし、`MM_REQUIRE_OFX` があれば失敗にする。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mm_host_ofx::{describe_bundle, OfxLauncher, TEST_PLUGINS};
use mm_nodes_ofx::{Describe, OfxKind, OfxPlugin, BUNDLE_PARAM, PLUGIN_PARAM};
use mm_types::{ParamValue, Params};

/// 試験の Gain（`mm-host-ofx/tests/plugins/gain.c`）。
pub const GAIN: &str = "MmTestGain.ofx.bundle";
pub const GAIN_ID: &str = "mm.test.Gain";
/// OpenFX の公式のサンプルの Invert。
pub const INVERT: &str = "Invert.ofx.bundle";
const DESCRIBE_TIMEOUT: Duration = Duration::from_secs(15);

/// ホストの実行ファイル。無ければ `None`（`MM_REQUIRE_OFX` があれば panic）。
pub fn host() -> Option<PathBuf> {
    let found = mm_host_ofx::find_host();
    if found.is_none() {
        assert!(
            std::env::var_os("MM_REQUIRE_OFX").is_none(),
            "MM_REQUIRE_OFX があるのに mm-host-ofx が無い（cargo build -p mm-host-ofx か MM_OFX_HOST）"
        );
        eprintln!("mm-host-ofx が無いのでスキップする");
    }
    found
}

/// 組み立て（`mm-runtime` の `ofx.rs`）と同じ形の describe。
pub fn describe(exe: PathBuf) -> Describe {
    Arc::new(move |bundle: &Path| {
        let desc = describe_bundle(&exe, bundle, DESCRIBE_TIMEOUT)?;
        Ok(desc
            .plugins
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
            .collect())
    })
}

/// 試験のプラグインの置き場所を OpenFX の置き場所にした種類。
pub fn kind(exe: &Path) -> OfxKind {
    kind_in(exe, PathBuf::from(TEST_PLUGINS))
}

/// `root` を OpenFX の置き場所にした種類。
pub fn kind_in(exe: &Path, root: PathBuf) -> OfxKind {
    OfxKind::new(
        Arc::new(OfxLauncher),
        exe.to_path_buf(),
        describe(exe.to_path_buf()),
        vec![root],
    )
}

/// バンドルと効果と値を指すパラメータ。
pub fn params(bundle: &str, plugin: &str, values: &[(&str, ParamValue)]) -> Params {
    let mut p = Params::new();
    p.set(BUNDLE_PARAM, ParamValue::Text(bundle.into()))
        .set(PLUGIN_PARAM, ParamValue::Text(plugin.into()));
    for (name, value) in values {
        p.set(*name, value.clone());
    }
    p
}
