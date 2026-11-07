//! ホストとアプリの間の JSON の形。プラグインのファイルの記述（列挙）と、`Prepare` の `params`。
//! CLAP と VST3 で同じ形を使い、規格ごとの意味は各項目に書く。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 1 つのプラグインのファイル（`.clap` のファイル、`.vst3` のバンドル）の記述。`<ホスト> describe <ファイル>` が 1 行で出す。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BundleDesc {
    /// プラグインのファイル。
    pub path: PathBuf,
    /// ファイルのプラグイン（VST3 は Audio Module Class のクラス）。インスタンスを作れたものはパラメータとポートも入る。
    pub plugins: Vec<PluginDesc>,
    /// インスタンスを作れなかったプラグインと理由。
    #[serde(default)]
    pub skipped: Vec<Skipped>,
}

/// 読めなかったプラグイン。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skipped {
    pub id: String,
    pub reason: String,
}

/// 1 つのプラグインの記述（規格の記述と、インスタンスから読んだパラメータとポート）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PluginDesc {
    /// CLAP の `id`（逆ドメイン名）、VST3 のクラスの ID（32 桁の 16 進）。
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// CLAP の features（`audio-effect`、`stereo` など）、VST3 のサブカテゴリー（`Fx`、`Delay` など）。
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub params: Vec<ParamDesc>,
    /// 主の入力のチャンネル数。入力が無ければ 0。
    #[serde(default)]
    pub input_channels: u32,
    /// 主の出力のチャンネル数。
    #[serde(default)]
    pub output_channels: u32,
}

impl PluginDesc {
    /// 音の効果として挿せるか（主の出力があり、入力が無いか 2 ch 以下）。
    pub fn is_effect(&self) -> bool {
        self.output_channels > 0 && self.input_channels <= 2
    }
}

/// 1 つのパラメータ。値は CLAP の plain value、VST3 の正規化した値（`min`..=`max`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamDesc {
    /// パラメータの `id`（CLAP の `clap_id`、VST3 の `ParamID`）。プラグインの版が変わっても変わらない。
    pub id: u32,
    pub name: String,
    /// `/` で区切った置き場所（VST3 は単位の ID）。
    #[serde(default)]
    pub module: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// 飛び飛びの値だけを取る。
    #[serde(default)]
    pub stepped: bool,
    /// 利用者に見せない。
    #[serde(default)]
    pub hidden: bool,
    /// ホストが変えられない。
    #[serde(default)]
    pub read_only: bool,
}

/// `HostRequest::Prepare` の `params`。プラグインを読み込み、共有メモリにつなぎ、処理を始める。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrepareParams {
    /// プラグインのファイル。
    pub path: PathBuf,
    /// 使うプラグインの `id`（[`PluginDesc::id`]）。`None` ならファイルの最初のプラグイン。
    #[serde(default)]
    pub plugin: Option<String>,
    /// サンプルレート。
    pub sample_rate: u32,
    /// アプリが作った共有メモリとイベントの名前の元（[`super::block`]）。
    pub link: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 記述は省いた項目を既定で読む() {
        let d: PluginDesc = serde_json::from_value(serde_json::json!({"id": "a", "name": "A"}))
            .expect("id と name だけで読める");
        assert!(d.params.is_empty());
        assert!(!d.is_effect(), "出力の無いプラグインは効果ではない");
    }
}
