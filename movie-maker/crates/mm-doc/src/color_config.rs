//! プロジェクトの色の設定（`colorConfig`）。今はタイムラインの作業の明るさだけを持つ。
//!
//! 作業の明るさは、HDR の素材のトーンマップを掛ける位置を決める（`docs/design/03-data-model.md` の「色」）。
//! SDR（既定）は HDR の素材をデコードの直後で SDR の白に収め、HDR は 1 を超える値のまま合成して出力の側で収める。

use serde::{Deserialize, Serialize};

use mm_types::ColorSpace;

use crate::Project;

/// タイムラインの作業の明るさ。Resolve のタイムラインの色空間、Premiere Pro のシーケンスの作業色空間に当たる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum WorkingRange {
    /// HDR の素材を入力の側で SDR の白（線形 1.0）に収める。SDR の文字や画像の白は SDR の書き出しで白のまま。
    #[default]
    Sdr,
    /// HDR の素材を 1 を超える値のまま合成する。SDR の出力（プレビュー、SDR の書き出し、PNG）は合成の後で収める。
    Hdr,
}

impl WorkingRange {
    /// JSON の値（`sdr`、`hdr`）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sdr => "sdr",
            Self::Hdr => "hdr",
        }
    }
}

/// プロジェクトの色の設定。既定（全部が既定値）は保存しない。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ColorConfig {
    /// タイムラインの作業の明るさ。
    #[serde(default)]
    pub working_range: WorkingRange,
}

impl ColorConfig {
    /// 全部が既定値か（保存で省く）。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// HDR の信号（PQ、HLG）で書き出すレンダーターゲットのコーデックの名前。名前の順。
/// `mm-encode-ffmpeg` の試験が、コーデックの表の `OutputSignal::is_hdr` と揃うことを確かめる。
pub const HDR_SIGNAL_CODECS: [&str; 2] = ["hevc_hlg", "hevc_pq"];

/// 色空間の名前が HDR の素材（`bt2100_pq`、`bt2100_hlg`）か。
pub(crate) fn is_hdr_space_name(name: &str) -> bool {
    [ColorSpace::bt2100_pq(), ColorSpace::bt2100_hlg()]
        .iter()
        .any(|cs| cs.0 == name)
}

/// 作業の明るさが SDR のまま、HDR の素材があるプロジェクトを HDR の信号のコーデック `codec` で書き出すか。
/// 真なら HDR の素材のハイライトは入力の側のトーンマップで SDR の白の下に収まったまま書き出される。
pub fn hdr_signal_from_sdr_range(project: &Project, codec: &str) -> bool {
    project.color_config.working_range == WorkingRange::Sdr
        && HDR_SIGNAL_CODECS.contains(&codec)
        && project
            .assets
            .iter()
            .any(|a| is_hdr_space_name(&a.color_space.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 既定は_sdr_で_json_は小文字() {
        assert_eq!(ColorConfig::default().working_range, WorkingRange::Sdr);
        let hdr = ColorConfig {
            working_range: WorkingRange::Hdr,
        };
        let json = serde_json::to_value(&hdr).unwrap();
        assert_eq!(json, serde_json::json!({ "working_range": "hdr" }));
        assert_eq!(WorkingRange::Hdr.as_str(), "hdr");
        let back: ColorConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, hdr);
        let empty: ColorConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(empty.is_default());
    }
}
