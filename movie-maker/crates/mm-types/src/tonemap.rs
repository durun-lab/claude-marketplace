//! HDR の線形を SDR の表示（1.0 が白）に収めるトーンマップ。ITU-R BT.2390 の EETF（PQ の空間のエルミート曲線の肩）を
//! RGB の最大値に掛け、比で RGB を縮めて色相を保つ。素材の上限が 1 以下なら何もしないので、SDR の絵は変わらない。
//!
//! 線形の単位は [`crate::transfer`] と同じ（1.0 が HDR の基準の白）。WGSL の [`TONEMAP_WGSL`] は
//! `transfer::TRANSFER_WGSL` の後に連結する。

use crate::transfer::{linear_to_pq, pq_to_linear, HDR_REFERENCE_WHITE_NITS, HLG_PEAK_NITS};
use crate::ColorSpace;

/// プレビューのトーンマップの素材の上限（線形）。素材の色空間に HDR（`bt2100_pq`、`bt2100_hlg`）があれば
/// 1000 cd/m²（HDR10 のマスタリングの既定、HLG の基準の表示）、無ければ 1.0（トーンマップしない）。
pub fn preview_peak<'a>(spaces: impl IntoIterator<Item = &'a ColorSpace>) -> f32 {
    let hdr = spaces
        .into_iter()
        .any(|cs| cs == &ColorSpace::bt2100_pq() || cs == &ColorSpace::bt2100_hlg());
    if hdr {
        HLG_PEAK_NITS / HDR_REFERENCE_WHITE_NITS
    } else {
        1.0
    }
}

/// 肩の始まりを決める BT.2390 の係数（`KS = 1.5 * max_lum - 0.5`）。
const KNEE_SLOPE: f32 = 1.5;
const KNEE_OFFSET: f32 = 0.5;

/// BT.2390 の EETF。`max_lum` は表示の上限を素材の上限で割った PQ の値、`e1` は素材の上限で割った PQ の値。
fn eetf(e1: f32, max_lum: f32) -> f32 {
    let ks = KNEE_SLOPE * max_lum - KNEE_OFFSET;
    if e1 < ks {
        return e1;
    }
    let t = (e1.min(1.0) - ks) / (1.0 - ks);
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * ks
        + (t3 - 2.0 * t2 + t) * (1.0 - ks)
        + (-2.0 * t3 + 3.0 * t2) * max_lum
}

/// 線形の RGB を、上限 `source_peak`（線形の値）の素材として 1.0 の表示に収める。`source_peak` が 1 以下なら恒等。
pub fn bt2390_to_sdr(rgb: [f32; 3], source_peak: f32) -> [f32; 3] {
    if source_peak <= 1.0 {
        return rgb;
    }
    let peak = rgb[0].max(rgb[1]).max(rgb[2]);
    if peak <= 0.0 {
        return rgb;
    }
    let src = linear_to_pq(source_peak);
    let mapped = pq_to_linear(eetf(linear_to_pq(peak) / src, linear_to_pq(1.0) / src) * src);
    let k = mapped / peak;
    rgb.map(|c| (c * k).min(1.0))
}

/// [`bt2390_to_sdr`] の WGSL。`fn mm_tonemap_sdr(c: vec3<f32>, source_peak: f32) -> vec3<f32>`。
pub const TONEMAP_WGSL: &str = r"
fn mm_bt2390_eetf(e1: f32, max_lum: f32) -> f32 {
    let ks = 1.5 * max_lum - 0.5;
    if e1 < ks { return e1; }
    let t = (min(e1, 1.0) - ks) / (1.0 - ks);
    let t2 = t * t;
    let t3 = t2 * t;
    return (2.0 * t3 - 3.0 * t2 + 1.0) * ks + (t3 - 2.0 * t2 + t) * (1.0 - ks) + (-2.0 * t3 + 3.0 * t2) * max_lum;
}

fn mm_tonemap_sdr(c: vec3<f32>, source_peak: f32) -> vec3<f32> {
    let peak = max(c.x, max(c.y, c.z));
    if source_peak <= 1.0 || peak <= 0.0 { return c; }
    let src = mm_linear_to_pq(source_peak);
    let mapped = mm_pq_to_linear(mm_bt2390_eetf(mm_linear_to_pq(peak) / src, mm_linear_to_pq(1.0) / src) * src);
    return min(c * (mapped / peak), vec3<f32>(1.0));
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdr_の素材は変わらない() {
        let rgb = [0.9, 0.5, 0.1];
        assert_eq!(bt2390_to_sdr(rgb, 1.0), rgb);
    }

    #[test]
    fn 上限は白に_暗部はそのまま() {
        let peak = 1000.0 / 203.0;
        let top = bt2390_to_sdr([peak; 3], peak);
        assert!(top.iter().all(|c| (c - 1.0).abs() < 1e-3), "{top:?}");
        let dark = bt2390_to_sdr([0.05, 0.02, 0.01], peak);
        assert!((dark[0] - 0.05).abs() < 1e-4, "肩より下は恒等: {dark:?}");
    }

    #[test]
    fn 単調で色相を保つ() {
        let peak = 1000.0 / 203.0;
        let mut last = 0.0;
        for i in 1..=100 {
            let v = peak * i as f32 / 100.0;
            let out = bt2390_to_sdr([v, v * 0.5, 0.0], peak);
            assert!(out[0] >= last, "{v}");
            assert!((out[1] / out[0] - 0.5).abs() < 1e-4, "比を保つ");
            last = out[0];
        }
    }

    #[test]
    fn hdr_の素材があるときだけ上限を上げる() {
        let sdr = [ColorSpace::bt709(), ColorSpace::bt2020()];
        assert_eq!(preview_peak(&sdr), 1.0);
        let mixed = [ColorSpace::bt709(), ColorSpace::bt2100_pq()];
        assert!((preview_peak(&mixed) - 1000.0 / 203.0).abs() < 1e-4);
    }

    #[test]
    fn wgsl_の係数は_cpu_と同じ() {
        assert!(TONEMAP_WGSL.contains(&format!("{KNEE_SLOPE:?} * max_lum - {KNEE_OFFSET:?}")));
    }
}
