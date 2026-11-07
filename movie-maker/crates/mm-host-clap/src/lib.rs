//! CLAP の音のプラグイン（`.clap`）を別プロセスで読み込み、音のブロックを処理するホストと、
//! レイヤーの効果として挿す種類 `audio.clap`（`mm-node-api` の実装）。
//!
//! - アプリ側: [`ClapEffect`] が実行ファイル `mm-host-clap serve` を `mm_host_pipe::PipeHost` で起こし、
//!   制御（`Ping`、`Prepare`、`Shutdown`）はパイプの JSON、音のブロックは共有メモリ（[`shm`]）で渡す。
//!   列挙は [`describe_bundle`] が `mm-host-clap describe <ファイル>` を別プロセスで実行して受け取る。
//! - ホスト側（`src/host/`、実行ファイルだけ）: `clack-host` でプラグインを読み込み、音のスレッドで処理する。
//!   プラグインのクラッシュはホストのプロセスだけを止め、アプリには `HostCrashed` で返る。
//!
//! 手順、遅延、決定性、先行事例は README。

pub mod desc;
mod effect;
mod kind;
mod launcher;
pub mod shm;

pub use desc::{BundleDesc, ParamDesc, PluginDesc, PrepareParams, Skipped};
pub use effect::{ClapEffect, ParamPoint};
pub use kind::{param_key, ClapEffectKind, KIND_ID, PARAM_PREFIX, PLUGIN_ID_PARAM, PLUGIN_PARAM};
pub use launcher::{describe_bundle, find_host, ClapLauncher, DESCRIBE_COMMAND, HOST_ENV};
pub use mm_host_pipe::{PipeHost, SERVE_COMMAND, SPEC_ENV};
