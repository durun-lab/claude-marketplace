//! 音のプラグインのホスト（`mm-host-clap`、`mm-host-vst3`）の共通実装。
//!
//! - アプリ側: [`AudioEffect`] がホストを起こし、制御（`Ping`、`Prepare`、`Shutdown`）はパイプの JSON、
//!   音のブロックとパラメータのイベントは共有メモリ（[`block`]）で同じ呼び出しの中で往復させる。
//!   [`PluginEffectKind`] はレイヤーの効果として挿す種類（`mm-node-api` の実装）で、インスタンスごとにホストを持ち、
//!   続きでない区間でプラグインの状態を消す。
//! - ホスト側: [`serve`] が標準入力の要求と、共有メモリの要求を待って処理する繰り返しを持つ。
//!   規格ごとの違い（プラグインの読み込みと処理）は [`serve::Plugin`] で渡す。
//!
//! 規格ごとの名前（種類、実行ファイル、時間切れの `what`）は [`PluginStandard`] の定数で渡す（RFC 0139）。

pub mod block;
pub mod desc;
mod effect;
mod kind;
mod launch;
pub mod serve;

use crate::PipeKind;

pub use desc::{BundleDesc, ParamDesc, PluginDesc, PrepareParams, Skipped};
pub use effect::{AudioEffect, ParamPoint};
pub use kind::{param_key, PluginEffectKind, PARAM_PREFIX, PLUGIN_ID_PARAM, PLUGIN_PARAM};
pub use launch::{describe_bundle, find_host, DESCRIBE_COMMAND};

/// 音のプラグインの規格（CLAP、VST3）ごとの名前。各ホストのクレートが 1 つの型に実装する。
pub trait PluginStandard: Send + Sync + 'static {
    /// エラーの文に出す規格の名前（`CLAP`）。
    const NAME: &'static str;
    /// 種類の名前（`audio.clap`）。
    const KIND_ID: &'static str;
    /// ホストの起こし方。`name` は時間切れの `what` の頭（`host_clap.process`）にもなる。
    const PIPE: PipeKind;
    /// ホストの実行ファイルの名前（拡張子を除く）。
    const HOST_STEM: &'static str;
    /// ホストの実行ファイルを直に指す環境変数。
    const HOST_ENV: &'static str;
    /// 共有メモリとイベントの名前の頭（`mm-clap`）。
    const LINK_PREFIX: &'static str;
}
