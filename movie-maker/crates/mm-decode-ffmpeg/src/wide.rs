//! 10 bit 以上、HDR（PQ と HLG）、Rec.2020 の原色の素材を、8 bit を経由せずに作業色空間の線形にする（CPU の経路）。
//!
//! swscale は色差を補間して 16 bit の 4:4:4 の YUV にするだけに使い、行列とレンジは素材の符号語に戻して自前で解く。
//! swscale は高ビットの値を桁送りで 16 bit に広げ、白が 65535 に届かないので、swscale の 16 bit の RGB を
//! 0..1 として読むと PQ の上端で約 2% 暗くなる。式は GPU の経路（`mm-decode-hw` の `YUV_WGSL`）と同じ。

use std::sync::OnceLock;

use ff::frame;
use ffmpeg_next as ff;
use mm_decode_ffmpeg_core::matrix::{range_codes, Matrix, Transfer};
use mm_types::transfer::{mul3, REC2020_TO_REC709};
use mm_types::{Result, RgbaImage, Size};

/// 16 bit の符号化値の段の数と最大。
const STEPS16: usize = 1 << 16;
const MAX16: f32 = u16::MAX as f32;

/// 16 bit の符号化値 → 線形の表。伝達特性ごとに 1 回だけ作る。
fn table16(transfer: Transfer) -> &'static [f32] {
    static TABLES: [OnceLock<Vec<f32>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let slot = match transfer {
        Transfer::Bt1886 => 0,
        Transfer::Pq => 1,
        Transfer::Hlg => 2,
    };
    TABLES[slot].get_or_init(|| {
        (0..STEPS16)
            .map(|code| transfer.to_linear(code as f32 / MAX16))
            .collect()
    })
}

/// 16 bit の 4:4:4 の YUV から線形の RGB への変換。
pub(crate) struct WideYuv {
    /// 16 bit の値 → 素材のビット深度の符号語（swscale の桁送りを戻す）。
    scale: f32,
    range: [f32; 4],
    rgb: [f32; 4],
    linear: &'static [f32],
    rec2020: bool,
}

impl WideYuv {
    /// 素材の行列、レンジ、ビット深度、伝達特性、原色から。
    pub(crate) fn new(
        matrix: Matrix,
        full_range: bool,
        bits: u32,
        transfer: Transfer,
        rec2020: bool,
    ) -> Self {
        let bits = bits.clamp(8, 16);
        Self {
            scale: 1.0 / (1u32 << (16 - bits)) as f32,
            range: range_codes(full_range, bits),
            rgb: matrix.rgb_coefficients(),
            linear: table16(transfer),
            rec2020,
        }
    }

    /// 符号語の YUV を線形の RGB（作業色空間）にする。
    fn to_linear(&self, y: f32, u: f32, v: f32) -> [f32; 3] {
        let [y_off, y_range, c_mid, c_range] = self.range;
        let [rv, gu, gv, bu] = self.rgb;
        let luma = (y - y_off) / y_range;
        let (cb, cr) = ((u - c_mid) / c_range, (v - c_mid) / c_range);
        let encoded = [luma + rv * cr, luma - gu * cb - gv * cr, luma + bu * cb];
        let lin = encoded.map(|e| self.linear[(e.clamp(0.0, 1.0) * MAX16).round() as usize]);
        if self.rec2020 {
            mul3(&REC2020_TO_REC709, lin)
        } else {
            lin
        }
    }

    /// `YUVA444P16LE` のフレームを線形の premultiplied の画像にする。
    pub(crate) fn to_image(&self, frame: &frame::Video, size: Size) -> Result<RgbaImage> {
        let (w, h) = (size.width as usize, size.height as usize);
        let planes = [0, 1, 2, 3].map(|p| (frame.data(p), frame.stride(p)));
        let at = |p: usize, x: usize, y: usize| {
            let (data, stride) = planes[p];
            let i = y * stride + x * 2;
            f32::from(u16::from_le_bytes([data[i], data[i + 1]]))
        };
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                let code = |p| at(p, x, y) * self.scale;
                let rgb = self.to_linear(code(0), code(1), code(2));
                let a = at(3, x, y) / MAX16;
                out.extend([rgb[0] * a, rgb[1] * a, rgb[2] * a, a]);
            }
        }
        RgbaImage::from_raw(size, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 十ビットの_tv_レンジの白と黒が_0_と_1_になる() {
        let w = WideYuv::new(Matrix::Bt709, false, 10, Transfer::Bt1886, false);
        let white = w.to_linear(940.0, 512.0, 512.0);
        let black = w.to_linear(64.0, 512.0, 512.0);
        assert!(white.iter().all(|c| (c - 1.0).abs() < 1e-4), "{white:?}");
        assert!(black.iter().all(|c| c.abs() < 1e-6), "{black:?}");
    }

    #[test]
    fn pq_の_1000_カンデラは線形で_1_を超える() {
        let w = WideYuv::new(Matrix::Bt2020, false, 10, Transfer::Pq, true);
        // PQ の 0.7518 が 1000 cd/m²。tv レンジの 10 bit の符号語は 64 + 876 * e
        let rgb = w.to_linear(64.0 + 876.0 * 0.751_8, 512.0, 512.0);
        assert!(
            rgb.iter().all(|c| (c - 1000.0 / 203.0).abs() < 0.05),
            "{rgb:?}"
        );
    }
}
