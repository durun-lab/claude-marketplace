//! CLAP の音のプラグイン（`.clap`）を別プロセスで読み込み、音のブロックを処理するホストと、
//! レイヤーの効果として挿す種類 `audio.clap`（`mm-node-api` の実装）。
//!
//! - アプリ側: [`ClapEffect`] が実行ファイル `mm-host-clap serve` を `mm_host_pipe::PipeHost` で起こし、
//!   制御（`Ping`、`Prepare`、`Shutdown`）はパイプの JSON、音のブロックは共有メモリで渡す。
//!   列挙は [`describe_bundle`] が `mm-host-clap describe <ファイル>` を別プロセスで実行して受け取る。
//!   受け渡しと種類の本体は VST3 と共通の `mm_host_pipe::audio` で、ここは CLAP の名前（[`Clap`]）を持つ。
//! - ホスト側（`src/host/`、実行ファイルだけ）: `clack-host` でプラグインを読み込み、音のスレッドで処理する。
//!   プラグインのクラッシュはホストのプロセスだけを止め、アプリには `HostCrashed` で返る。
//!
//! 手順、遅延、決定性、先行事例は README。

use std::path::{Path, PathBuf};
use std::time::Duration;

use mm_host_pipe::audio::{AudioEffect, PluginEffectKind, PluginStandard};
use mm_host_pipe::{PipeKind, StaleHandles};
use mm_types::Result;

pub use mm_host_pipe::audio::block as shm;
pub use mm_host_pipe::audio::{
    param_key, BundleDesc, ParamDesc, ParamPoint, PluginDesc, PrepareParams, Skipped,
    DESCRIBE_COMMAND, PARAM_PREFIX, PLUGIN_ID_PARAM, PLUGIN_PARAM,
};
pub use mm_host_pipe::{PipeHost, SERVE_COMMAND, SPEC_ENV};

/// 種類の名前。
pub const KIND_ID: &str = "audio.clap";
/// ホストの実行ファイルを直に指す環境変数。
pub const HOST_ENV: &str = "MM_CLAP_HOST";
/// 起動直後の `Ping` の期限。プラグインはまだ読み込まないので短くてよい。
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// CLAP の名前。
pub struct Clap;

impl PluginStandard for Clap {
    const NAME: &'static str = "CLAP";
    const KIND_ID: &'static str = KIND_ID;
    const PIPE: PipeKind = PipeKind {
        name: "host_clap",
        startup_timeout: STARTUP_TIMEOUT,
        work_dir_env: None,
        stale_handles: StaleHandles::Count,
        configure: |_| {},
    };
    const HOST_STEM: &'static str = "mm-host-clap";
    const HOST_ENV: &'static str = HOST_ENV;
    const LINK_PREFIX: &'static str = "mm-clap";
}

/// 1 つの CLAP のプラグインのインスタンス。
pub type ClapEffect = AudioEffect<Clap>;
/// 種類 `audio.clap`。
pub type ClapEffectKind = PluginEffectKind<Clap>;

/// 起こすホストの実行ファイル。`MM_CLAP_HOST` があればそれ、無ければ今の実行ファイルの隣の `mm-host-clap`。
pub fn find_host() -> Option<PathBuf> {
    mm_host_pipe::audio::find_host::<Clap>()
}

/// `executable describe <path>` を別プロセスで実行し、`.clap` のファイルの記述を受け取る。
pub fn describe_bundle(executable: &Path, path: &Path, timeout: Duration) -> Result<BundleDesc> {
    mm_host_pipe::audio::describe_bundle::<Clap>(executable, path, timeout)
}
