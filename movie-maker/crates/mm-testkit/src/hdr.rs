//! HDR の素材のフィクスチャ。ffmpeg の CLI で HEVC Main10（libx265 のロスレス）の PQ と HLG の素材を作り、
//! 帯ごとの期待の線形の値（作業色空間、1.0 が HDR の基準の白）を持つ。デコーダー、書き出し、通しのテストが使う。
//!
//! 素材は横に [`BANDS`] の帯を並べた 320x160。RGB の符号化値（Rec.2020 の原色）を gbrp で作り、Rec.2020 の行列の
//! 10 bit の YUV にする。期待の値は符号化値を `mm_types::transfer` の式で線形にし、Rec.709 の原色に直したもの。

use std::path::{Path, PathBuf};
use std::process::Command;

use mm_types::transfer::{hlg_to_linear, mul3, pq_to_linear, REC2020_TO_REC709};
use mm_types::{RgbaImage, Size};

/// 帯の RGB の符号化値（8 bit、Rec.2020 の原色）。暗部、SDR の白の付近、PQ の 1000 cd/m² の付近、HLG の上端の付近、
/// Rec.709 の外の緑（作業色空間では赤と青が負になる）。
pub const BANDS: [[u8; 3]; 5] = [
    [0x40, 0x40, 0x40],
    [0x94, 0x94, 0x94],
    [0xC0, 0xC0, 0xC0],
    [0xF0, 0xF0, 0xF0],
    [0x20, 0xC0, 0x20],
];
/// 帯の幅（画素）。色差の補間が帯の中ほどに混ざらない幅。
pub const BAND_WIDTH: u32 = 64;
/// 素材の大きさ。高さは NVDEC の HEVC の下限（144）より大きくする。
pub const SIZE: Size = Size::new(BAND_WIDTH * BANDS.len() as u32, 160);
/// 素材のフレーム数と fps。
pub const FRAMES: u32 = 10;
pub const FPS: u32 = 30;

/// HDR の伝達特性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdrKind {
    Pq,
    Hlg,
}

impl HdrKind {
    /// ffmpeg と x265 の伝達特性の名前。
    pub fn trc(self) -> &'static str {
        match self {
            Self::Pq => "smpte2084",
            Self::Hlg => "arib-std-b67",
        }
    }

    /// 符号化値（0..=1）を線形にする式。
    pub fn to_linear(self, v: f32) -> f32 {
        match self {
            Self::Pq => pq_to_linear(v),
            Self::Hlg => hlg_to_linear(v),
        }
    }

    /// デコーダーが名乗る色空間の名前。
    pub fn color_space(self) -> &'static str {
        match self {
            Self::Pq => "bt2100_pq",
            Self::Hlg => "bt2100_hlg",
        }
    }
}

/// 帯 `band` の期待の線形の値（作業色空間）。
pub fn expected(kind: HdrKind, band: usize) -> [f32; 3] {
    let lin = BANDS[band].map(|c| kind.to_linear(f32::from(c) / 255.0));
    mul3(&REC2020_TO_REC709, lin)
}

/// 帯 `band` の中央の画素の座標。
pub fn band_center(band: usize) -> (u32, u32) {
    (band as u32 * BAND_WIDTH + BAND_WIDTH / 2, SIZE.height / 2)
}

/// 期待との差の許容。8 bit の符号化値を 10 bit の YUV にする丸め（PQ の 1000 cd/m² 付近で 1 段が約 1%）と、
/// 色差の行列の往復の分。値の大きさに比例させ、0 の付近は絶対の差で見る。
pub fn tolerance(want: f32) -> f32 {
    const RELATIVE: f32 = 0.03;
    const ABSOLUTE: f32 = 0.004;
    want.abs() * RELATIVE + ABSOLUTE
}

/// 画像の各帯の中央が期待に合うことを確かめる。合わなければ帯と値を返す。`what` は失敗の文言に入れる。
pub fn check_bands(image: &RgbaImage, kind: HdrKind, what: &str) -> Result<(), String> {
    for band in 0..BANDS.len() {
        let (x, y) = band_center(band);
        let p = image.pixel(x, y);
        let got = [p.r, p.g, p.b];
        let want = expected(kind, band);
        for c in 0..3 {
            if (got[c] - want[c]).abs() > tolerance(want[c]) {
                return Err(format!(
                    "{what} {kind:?} の帯 {band}: {got:?} 期待 {want:?}"
                ));
            }
        }
    }
    Ok(())
}

/// 帯の期待との相対の差の最大（報告用）。0 の付近の値は 1 で割る。
pub fn max_relative_error(image: &RgbaImage, kind: HdrKind) -> f32 {
    (0..BANDS.len())
        .flat_map(|band| {
            let (x, y) = band_center(band);
            let p = image.pixel(x, y);
            let want = expected(kind, band);
            [p.r, p.g, p.b]
                .into_iter()
                .zip(want)
                .map(|(g, w)| (g - w).abs() / w.abs().max(1.0))
                .collect::<Vec<_>>()
        })
        .fold(0.0, f32::max)
}

/// `dir` に HDR の素材を作って返す（あれば作らない）。ffmpeg の CLI（libx265 つき）が要る。
/// 同時に作っても壊れないよう、別名で書いてから置く。
pub fn fixture(dir: &Path, kind: HdrKind) -> PathBuf {
    let target = dir.join(format!("hdr_{}.mp4", kind.trc()));
    if target.exists() {
        return target;
    }
    std::fs::create_dir_all(dir).expect("フィクスチャの置き場を作れた");
    let tmp = dir.join(format!("hdr_{}.{}.tmp.mp4", kind.trc(), std::process::id()));
    let status = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-filter_complex", &graph(kind)])
        .args([
            "-frames:v",
            &FRAMES.to_string(),
            "-c:v",
            "libx265",
            "-pix_fmt",
            "yuv420p10le",
        ])
        .args(["-x265-params", &x265_params(kind)])
        .args([
            "-color_primaries",
            "bt2020",
            "-color_trc",
            kind.trc(),
            "-colorspace",
            "bt2020nc",
        ])
        .arg(&tmp)
        .status()
        .expect("ffmpeg の CLI を起動できた");
    assert!(status.success(), "{kind:?} の HDR の素材を作れた");
    if std::fs::rename(&tmp, &target).is_err() {
        // 別のプロセスが先に置いた
        let _ = std::fs::remove_file(&tmp);
    }
    target
}

/// 帯を gbrp で作って横に並べ、Rec.2020 の行列の 10 bit の YUV にするフィルタのグラフ。
fn graph(kind: HdrKind) -> String {
    let h = SIZE.height;
    let mut parts: Vec<String> = BANDS
        .iter()
        .enumerate()
        .map(|(i, [r, g, b])| {
            format!("color=c=0x{r:02X}{g:02X}{b:02X}:s={BAND_WIDTH}x{h}:r={FPS},format=gbrp[b{i}]")
        })
        .collect();
    let inputs: String = (0..BANDS.len()).map(|i| format!("[b{i}]")).collect();
    parts.push(format!(
        "{inputs}hstack=inputs={},scale=out_color_matrix=bt2020nc:out_range=tv,format=yuv420p10le,\
         setparams=color_primaries=bt2020:color_trc={}:colorspace=bt2020nc:range=tv",
        BANDS.len(),
        kind.trc()
    ));
    parts.join(";")
}

fn x265_params(kind: HdrKind) -> String {
    format!(
        "lossless=1:log-level=error:colorprim=bt2020:transfer={}:colormatrix=bt2020nc",
        kind.trc()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 期待の値は_1_を超える帯と負の帯を持つ() {
        let pq_peak = expected(HdrKind::Pq, 2)[0];
        assert!(pq_peak > 4.0, "PQ の 0xC0 は 1000 cd/m² の付近: {pq_peak}");
        assert!(
            expected(HdrKind::Hlg, 3)[0] > 1.0,
            "HLG の 0xF0 は SDR の白より明るい"
        );
        assert!(
            expected(HdrKind::Pq, 4)[0] < 0.0,
            "Rec.2020 の緑は Rec.709 で赤が負"
        );
        assert!(
            (expected(HdrKind::Pq, 1)[1] - 1.0).abs() < 0.01,
            "PQ の 0x94 は SDR の白"
        );
    }
}
