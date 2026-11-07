//! `ofx` のノードの種類。OpenFX のプラグインを別プロセスのホスト（`mm-host-ofx`）で描き、レイヤーにする。
//!
//! - プラグイン: パラメータ `bundle`（OpenFX の置き場所の下のバンドル）か `asset`（プロジェクトの素材）でバンドルを指し、
//!   `plugin` で効果の識別子を選ぶ。置き場所の外のバンドルは描かない（[`locate`]）。
//! - パラメータ: describe の結果を `ParamSchema` にして `NodeKind::params_for` で出す。describe はバンドルごとに 1 回で、
//!   中の共有ライブラリが変わるまでキャッシュする。
//! - 時刻: パラメータはレンダラーがキーフレームを評価した値を `RenderFrame` ごとに渡す（`mm-host-ofx` の方針）。
//! - 入出力: 入力 0 のレイヤーの画像を共有メモリで渡し、返った CPU の画像をレイヤーにする（`frames.rs`）。
//! - クラッシュ: ホストがクラッシュしたら 1 回だけ起こし直してやり直し、それでも描けなければそのレイヤーだけ誤りにする。
//!
//! ホストの実行ファイル、ランチャー、describe の関数は組み立て（`mm-runtime`）が選んで渡す（実装は実装に依存しない）。

mod catalog;
mod frames;
mod hosts;
mod kind;
pub mod locate;

pub use catalog::{Describe, OfxPlugin};
pub use kind::OfxKind;

/// 種類の名前。
pub const KIND_ID: &str = "ofx";
/// 種類のバージョン。パラメータの形か、同じパラメータで描く画素を変えたら上げる。
pub const KIND_VERSION: u32 = 1;
/// バンドルの名前。OpenFX の置き場所からの相対（`Vendor/Foo.ofx.bundle`）か、置き場所の中の絶対パス。
pub const BUNDLE_PARAM: &str = "bundle";
/// プロジェクトの素材のバンドル（`AssetRef`）。素材のパスはバンドルのフォルダーか、その中のファイル。
pub const ASSET_PARAM: &str = "asset";
/// 効果の識別子（`pluginIdentifier`）。空ならバンドルの最初の効果。
pub const PLUGIN_PARAM: &str = "plugin";
