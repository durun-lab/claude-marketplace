//! OpenFX（OFX）のプラグインを別プロセスで読み込み、1 フレームを描いて返すホスト（`mm-host-api` の実装）。
//!
//! - アプリ側: [`OfxLauncher`] が実行ファイル `mm-host-ofx serve` を起こし、パイプで 1 行ずつ JSON をやり取りする。
//!   プラグインの一覧とパラメータは [`describe_bundle`] が `mm-host-ofx describe <バンドル>` を別プロセスで実行して受け取る。
//! - ホスト側（`src/host/`、実行ファイルだけ）: OpenFX の C API のホストを Rust で持ち、プラグインの DLL を読み込む。
//!   プラグインのクラッシュはホストのプロセスだけを止め、アプリには `HostCrashed` で返る。
//!
//! 要求の `params` の形は [`PrepareParams`] と [`RenderParams`]。手順と先行事例は README。

pub mod desc;
mod launcher;
mod locate;
pub mod scan;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use mm_host_api::SharedFrame;
use mm_types::{Params, Size};

pub use launcher::{describe_bundle, OfxLauncher, DESCRIBE_COMMAND};
pub use locate::{find_host, HOST_ENV, TEST_PLUGINS};
pub use mm_host_pipe::{PipeHost, SERVE_COMMAND, SPEC_ENV};

/// 時刻（秒）を OpenFX のフレームの番号に直すときの既定のフレームレート。
pub const DEFAULT_FRAME_RATE: f64 = 30.0;

/// `HostRequest::Prepare` の `params`。プラグインを読み込み、インスタンスを作る。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrepareParams {
    /// `<名前>.ofx.bundle` のフォルダー。
    pub bundle: PathBuf,
    /// 使うプラグインの `id`。`None` ならバンドルの最初のプラグイン。
    #[serde(default)]
    pub plugin: Option<String>,
    /// `RenderFrame` の時刻をフレームの番号に直すフレームレート。
    #[serde(default = "default_frame_rate")]
    pub frame_rate: f64,
}

fn default_frame_rate() -> f64 {
    DEFAULT_FRAME_RATE
}

/// `HostRequest::RenderFrame` の `params`。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RenderParams {
    /// Filter の文脈の入力（`Source` のクリップ）。`None` なら透明。大きさは `RenderFrame` の `size` に合わせる。
    #[serde(default)]
    pub source: Option<SharedFrame>,
    /// 時刻で評価済みのパラメータの値。無い名前は既定値のまま。
    #[serde(default)]
    pub values: Params,
    /// 画像を JSON に入れずに渡す共有メモリ。あれば `source` は使わず、結果も共有メモリに書く。
    #[serde(default)]
    pub shared: Option<SharedPixels>,
}

/// 入力と結果の画像を置く共有メモリ（Windows の名前つきのファイルマッピング）。アプリが作り、ホストが名前で開く。
/// 中身は `[入力 | 結果]` の 2 枚で、どちらも [`frame_bytes`] バイトの RGBA f32（リトルエンディアン、上の行から、
/// premultiplied、線形）。結果を共有メモリに書いた応答は、`SharedFrame::Cpu` の `data` が空になる。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedPixels {
    /// ファイルマッピングの名前（`Local\mm-ofx-<pid>-<番号>`）。
    pub name: String,
    /// 先頭に入力の画像を置いたか。偽なら透明を入力にする。
    #[serde(default)]
    pub has_source: bool,
}

/// 1 枚の画像の共有メモリの上の大きさ（RGBA f32）。共有メモリの全体はこの 2 倍。
pub fn frame_bytes(size: Size) -> usize {
    size.width as usize * size.height as usize * 4 * std::mem::size_of::<f32>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 要求の_params_は省略できる() {
        let p: PrepareParams =
            serde_json::from_value(serde_json::json!({"bundle": "a.ofx.bundle"}))
                .expect("bundle だけで読める");
        assert_eq!(p.plugin, None);
        assert_eq!(p.frame_rate, DEFAULT_FRAME_RATE);
        let r: RenderParams = serde_json::from_value(serde_json::json!({})).expect("空で読める");
        assert_eq!(r, RenderParams::default());
    }
}
