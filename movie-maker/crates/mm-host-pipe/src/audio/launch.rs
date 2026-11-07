//! アプリ側の起こし方。ホストの実行ファイルを探し、`<ホスト> describe` を 1 ファイル 1 プロセスで実行する。
//! `serve` は [`super::AudioEffect`] が [`crate::PipeHost`] で起こす。
//!
//! 音のホストは共有テクスチャを返さないので、一時フォルダーを作らず、期限切れの応答に共有テクスチャが
//! 入っていたら閉じずに漏れとして数える（各規格の `PIPE`）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use mm_types::Result;

use super::desc::BundleDesc;
use super::PluginStandard;

/// 起こすホストの実行ファイル。`S::HOST_ENV` があればそれ、無ければ今の実行ファイルと同じフォルダー
/// （テストの実行ファイルなら 1 つ上も）の `S::HOST_STEM`。
pub fn find_host<S: PluginStandard>() -> Option<PathBuf> {
    crate::find_executable(S::HOST_ENV, S::HOST_STEM)
}

/// `executable describe <path>` を別プロセスで実行し、ファイルの記述を受け取る。
/// プラグインの describe がクラッシュしても、このプロセスには `HostCrashed` で返る。
pub fn describe_bundle<S: PluginStandard>(
    executable: &Path,
    path: &Path,
    timeout: Duration,
) -> Result<BundleDesc> {
    crate::run_describe(executable, path, timeout, S::PIPE.name)
}
