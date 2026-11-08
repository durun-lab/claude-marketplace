//! タイムラインの作業の明るさ（`color_config.working_range`）。既定は `sdr`。
//!
//! HDR の素材があり、HDR の信号で書き出すレンダーターゲットを持つ既存のプロジェクトは `hdr` にして、HDR の書き出しの
//! ハイライトと合成の後のトーンマップの見え方を前のまま保つ（Premiere Pro が自動のトーンマップを足したとき、
//! 既存のプロジェクトを切った状態で開いたのと同じ）。新しいプロジェクトは移行の名前を全部持つので通らない。

use serde_json::Value;

use crate::color_config::{is_hdr_space_name, HDR_SIGNAL_CODECS};
use crate::WorkingRange;

pub(crate) fn up(value: &mut Value) {
    if value.pointer("/color_config/working_range").is_some() {
        return;
    }
    let any = |list: &str, field: &str, hit: &dyn Fn(&str) -> bool| {
        value[list]
            .as_array()
            .is_some_and(|items| items.iter().any(|i| i[field].as_str().is_some_and(hit)))
    };
    let hdr_asset = any("assets", "color_space", &is_hdr_space_name);
    let hdr_target = any("render_targets", "codec", &|c| HDR_SIGNAL_CODECS.contains(&c));
    if hdr_asset && hdr_target {
        value["color_config"]["working_range"] = Value::from(WorkingRange::Hdr.as_str());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::{hdr_signal_from_sdr_range, load, Project, WorkingRange};

    const NAME: &str = "20261108_working_range";

    /// 移行の前の v9 のプロジェクトのデータ（素材 1 つ、レンダーターゲット 1 つ）の素材の色空間とコーデックを変える。
    fn before(color_space: &str, codec: &str) -> Value {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/doc");
        let text = std::fs::read_to_string(dir.join("v9/burnin.json")).unwrap();
        let mut value: Value = serde_json::from_str(&text).unwrap();
        value["assets"][0]["color_space"] = Value::from(color_space);
        value["render_targets"][0]["codec"] = Value::from(codec);
        value
    }

    fn range_of(value: &Value) -> WorkingRange {
        load(&value.to_string()).unwrap().color_config.working_range
    }

    #[test]
    fn hdr_の素材を_hdr_の信号で書き出すプロジェクトだけ_hdr_になる() {
        assert_eq!(range_of(&before("bt2100_pq", "hevc_pq")), WorkingRange::Hdr);
        assert_eq!(range_of(&before("bt2100_hlg", "hevc_hlg")), WorkingRange::Hdr);
        assert_eq!(range_of(&before("bt2100_pq", "h264")), WorkingRange::Sdr);
        assert_eq!(range_of(&before("bt709", "hevc_pq")), WorkingRange::Sdr);
    }

    #[test]
    fn 移行を済ませたプロジェクトと設定を持つプロジェクトは変えない() {
        let mut done = before("bt2100_pq", "hevc_pq");
        done["migrations"] = Value::from(vec![NAME]);
        assert_eq!(range_of(&done), WorkingRange::Sdr);
        let mut set = before("bt2100_pq", "hevc_pq");
        set["color_config"] = serde_json::json!({ "working_range": "sdr" });
        assert_eq!(range_of(&set), WorkingRange::Sdr);
    }

    #[test]
    fn 移行の後に_sdr_のまま_hdr_の信号で書き出すと分かる() {
        let p: Project = load(&before("bt2100_pq", "hevc_pq").to_string()).unwrap();
        assert!(!hdr_signal_from_sdr_range(&p, "hevc_pq"));
        let mut sdr = p.clone();
        sdr.color_config.working_range = WorkingRange::Sdr;
        assert!(hdr_signal_from_sdr_range(&sdr, "hevc_pq"));
        assert!(!hdr_signal_from_sdr_range(&sdr, "h264"));
        let plain: Project = load(&before("bt709", "hevc_pq").to_string()).unwrap();
        assert!(!hdr_signal_from_sdr_range(&plain, "hevc_pq"));
    }
}
