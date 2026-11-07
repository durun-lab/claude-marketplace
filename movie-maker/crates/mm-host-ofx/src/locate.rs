//! アプリの側から `mm-host-ofx` の実行ファイルを探す。組み立て（`mm-runtime`）が `ofx` の種類を登録するかを決めるのに使う。

use std::path::PathBuf;

/// 実行ファイルを指す環境変数。
pub const HOST_ENV: &str = "MM_OFX_HOST";
/// 実行ファイルの名前（拡張子なし）。
const HOST_STEM: &str = "mm-host-ofx";

/// 試験のプラグイン（`build.rs` が作る `Invert.ofx.bundle` と `MmTestGain.ofx.bundle`）の置き場所。
/// このクレートの外の試験（`mm-nodes-ofx`）も同じプラグインで描くために公開する。
pub const TEST_PLUGINS: &str = env!("MM_OFX_TEST_PLUGINS");

/// `MM_OFX_HOST` か、このプロセスの実行ファイルと同じフォルダー、その親のフォルダーの `mm-host-ofx`。
/// 親も見るのは、試験の実行ファイル（`target/<profile>/deps/`）から `target/<profile>/` のホストを引くため。
pub fn find_host() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(HOST_ENV) {
        return Some(PathBuf::from(path)).filter(|p| p.is_file());
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = format!("{HOST_STEM}{}", std::env::consts::EXE_SUFFIX);
    let found = [Some(dir), dir.parent()]
        .into_iter()
        .flatten()
        .map(|d| d.join(&name))
        .find(|p| p.is_file());
    found
}
