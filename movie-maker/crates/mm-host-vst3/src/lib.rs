//! VST3 の音のプラグイン（`.vst3` のバンドル）を別プロセスで読み込み、音のブロックを処理するホストと、
//! レイヤーの効果として挿す種類 `audio.vst3`（`mm-node-api` の実装）。
//!
//! - アプリ側: [`Vst3Effect`] が実行ファイル `mm-host-vst3 serve` を `mm_host_pipe::PipeHost` で起こし、
//!   制御（`Ping`、`Prepare`、`Shutdown`）はパイプの JSON、音のブロックは共有メモリで渡す。
//!   列挙は [`describe_bundle`] が `mm-host-vst3 describe <バンドル>` を別プロセスで実行して受け取る。
//!   受け渡しと種類の本体は CLAP と共通の `mm_host_pipe::audio` で、ここは VST3 の名前（[`Vst3`]）を持つ。
//! - ホスト側（`src/host/`、実行ファイルだけ）: `vst3` クレートのバインディングでモジュールを読み込み、
//!   1 つのスレッドで初期化から処理までを行う。
//!
//! 手順、遅延、決定性、先行事例は README。

mod bundle;

use std::path::{Path, PathBuf};
use std::time::Duration;

use mm_host_pipe::audio::{AudioEffect, PluginEffectKind, PluginStandard};
use mm_host_pipe::{PipeKind, StaleHandles};
use mm_types::Result;

pub use bundle::{class_id_hex, default_dirs, module_file, parse_class_id, scan_dir};
pub use mm_host_pipe::audio::block as shm;
pub use mm_host_pipe::audio::{
    param_key, BundleDesc, ParamDesc, ParamPoint, PluginDesc, PrepareParams, Skipped,
    DESCRIBE_COMMAND, PARAM_PREFIX, PLUGIN_ID_PARAM, PLUGIN_PARAM,
};
pub use mm_host_pipe::{PipeHost, SERVE_COMMAND, SPEC_ENV};

/// 種類の名前。
pub const KIND_ID: &str = "audio.vst3";
/// ホストの実行ファイルを直に指す環境変数。
pub const HOST_ENV: &str = "MM_VST3_HOST";
/// 起動直後の `Ping` の期限。プラグインはまだ読み込まないので短くてよい。
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// VST3 の名前。
pub struct Vst3;

impl PluginStandard for Vst3 {
    const NAME: &'static str = "VST3";
    const KIND_ID: &'static str = KIND_ID;
    const PIPE: PipeKind = PipeKind {
        name: "host_vst3",
        startup_timeout: STARTUP_TIMEOUT,
        work_dir_env: None,
        stale_handles: StaleHandles::Count,
        configure: |_| {},
    };
    const HOST_STEM: &'static str = "mm-host-vst3";
    const HOST_ENV: &'static str = HOST_ENV;
    const LINK_PREFIX: &'static str = "mm-vst3";
    const EXTENSION: &'static str = "vst3";

    fn plugin_dirs() -> Vec<PathBuf> {
        default_dirs()
    }
}

/// 1 つの VST3 のプラグインのインスタンス。
pub type Vst3Effect = AudioEffect<Vst3>;
/// 種類 `audio.vst3`。
pub type Vst3EffectKind = PluginEffectKind<Vst3>;

/// 起こすホストの実行ファイル。`MM_VST3_HOST` があればそれ、無ければ今の実行ファイルの隣の `mm-host-vst3`。
pub fn find_host() -> Option<PathBuf> {
    mm_host_pipe::audio::find_host::<Vst3>()
}

/// `executable describe <path>` を別プロセスで実行し、`.vst3` のバンドルの記述を受け取る。
pub fn describe_bundle(executable: &Path, path: &Path, timeout: Duration) -> Result<BundleDesc> {
    mm_host_pipe::audio::describe_bundle::<Vst3>(executable, path, timeout)
}
