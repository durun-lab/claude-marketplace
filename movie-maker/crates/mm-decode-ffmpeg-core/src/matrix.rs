//! YUV の行列とレンジの宣言、伝達特性（SDR の Rec.1886、HDR の PQ と HLG）と原色の判定。

use ff::color::{Primaries, Range, Space, TransferCharacteristic};
use ffmpeg_next as ff;
use mm_types::{transfer, ColorSpace};

/// YUV から RGB への行列。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matrix {
    Bt601,
    Bt709,
    Bt2020,
}

impl Matrix {
    /// フレームの宣言から。未指定は画の高さで決める（HD 以上は 709）。
    pub fn from_frame(space: Space, height: u32) -> Self {
        match space {
            Space::BT709 => Self::Bt709,
            Space::BT2020NCL | Space::BT2020CL => Self::Bt2020,
            Space::SMPTE170M | Space::BT470BG | Space::FCC | Space::SMPTE240M => Self::Bt601,
            _ if height >= 720 => Self::Bt709,
            _ => Self::Bt601,
        }
    }

    /// プロジェクトのデータに出す色空間の名前。レンジは RGB への変換で展開済みなので名前に入れない。
    pub fn color_space(self) -> ColorSpace {
        match self {
            Self::Bt601 => ColorSpace::bt601(),
            Self::Bt709 => ColorSpace::bt709(),
            Self::Bt2020 => ColorSpace::bt2020(),
        }
    }

    /// 輝度の係数 (Kr, Kb)。
    pub fn coefficients(self) -> (f32, f32) {
        match self {
            Self::Bt601 => (0.299, 0.114),
            Self::Bt709 => (0.2126, 0.0722),
            Self::Bt2020 => (0.2627, 0.0593),
        }
    }

    /// 色差から RGB への係数 `[rv, gu, gv, bu]`。`R = Y + rv Cr`、`G = Y - gu Cb - gv Cr`、`B = Y + bu Cb`。
    pub fn rgb_coefficients(self) -> [f32; 4] {
        let (kr, kb) = self.coefficients();
        let kg = 1.0 - kr - kb;
        let rv = 2.0 * (1.0 - kr);
        let bu = 2.0 * (1.0 - kb);
        [rv, bu * kb / kg, rv * kr / kg, bu]
    }
}

/// フレームのレンジの宣言。未指定は tv レンジ。
pub fn is_full_range(range: Range) -> bool {
    matches!(range, Range::JPEG)
}

/// `bits` ビットの符号語の単位での `[輝度の黒, 輝度の幅, 色差の中央, 色差の幅]`。
pub fn range_codes(full_range: bool, bits: u32) -> [f32; 4] {
    let unit = (1u32 << (bits - 8)) as f32;
    let max_code = ((1u32 << bits) - 1) as f32;
    let c_mid = 128.0 * unit;
    if full_range {
        [0.0, max_code, c_mid, max_code]
    } else {
        [16.0 * unit, 219.0 * unit, c_mid, 224.0 * unit]
    }
}

/// 符号化値を線形にする式。SDR は全部 Rec.1886 の表示の式で読む（`mm_types::transfer` の決まり）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transfer {
    Bt1886,
    /// SMPTE ST 2084。
    Pq,
    /// ARIB STD-B67。
    Hlg,
}

impl Transfer {
    /// フレームの宣言から。PQ と HLG のほかは SDR として Rec.1886 で読む。
    pub fn from_characteristic(trc: TransferCharacteristic) -> Self {
        match trc {
            TransferCharacteristic::SMPTE2084 => Self::Pq,
            TransferCharacteristic::ARIB_STD_B67 => Self::Hlg,
            _ => Self::Bt1886,
        }
    }

    /// 符号化値（0..=1）を線形の光にする（1.0 が SDR の白、HDR はそれを超える）。
    pub fn to_linear(self, v: f32) -> f32 {
        match self {
            Self::Bt1886 => transfer::bt1886_to_linear(v),
            Self::Pq => transfer::pq_to_linear(v),
            Self::Hlg => transfer::hlg_to_linear(v),
        }
    }

    /// GPU の変換の一様バッファに入れる番号。`mm-decode-hw` の WGSL の `params.transfer` と同じ。
    pub fn code(self) -> f32 {
        match self {
            Self::Bt1886 => 0.0,
            Self::Pq => 1.0,
            Self::Hlg => 2.0,
        }
    }

    /// HDR（1 を超える線形の値を持つ）か。
    pub fn is_hdr(self) -> bool {
        self != Self::Bt1886
    }
}

/// 線形にした後で Rec.2020 の原色から作業色空間（Rec.709）へ行列を掛けるか。宣言が Rec.2020 のときと、
/// 原色が未指定の HDR（PQ と HLG は Rec.2020 が前提）のとき。
pub fn is_rec2020(primaries: Primaries, transfer: Transfer) -> bool {
    match primaries {
        Primaries::BT2020 => true,
        Primaries::Unspecified => transfer.is_hdr(),
        _ => false,
    }
}

/// プロジェクトのデータに出す色空間の名前。HDR は伝達特性の名前（`bt2100_pq`、`bt2100_hlg`）、SDR は行列の名前。
pub fn color_space_of(matrix: Matrix, transfer: Transfer) -> ColorSpace {
    match transfer {
        Transfer::Pq => ColorSpace::bt2100_pq(),
        Transfer::Hlg => ColorSpace::bt2100_hlg(),
        Transfer::Bt1886 => matrix.color_space(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_と_hlg_は_hdr_の式で_ほかは_rec1886() {
        assert_eq!(
            Transfer::from_characteristic(TransferCharacteristic::SMPTE2084),
            Transfer::Pq
        );
        assert_eq!(
            Transfer::from_characteristic(TransferCharacteristic::ARIB_STD_B67),
            Transfer::Hlg
        );
        for sdr in [
            TransferCharacteristic::BT709,
            TransferCharacteristic::SMPTE170M,
            TransferCharacteristic::Unspecified,
        ] {
            assert_eq!(
                Transfer::from_characteristic(sdr),
                Transfer::Bt1886,
                "{sdr:?}"
            );
        }
        assert!(
            Transfer::Pq.to_linear(1.0) > 40.0,
            "PQ の 1.0 は 10000 cd/m²"
        );
    }

    #[test]
    fn 原色は宣言を優先し_未指定の_hdr_は_rec2020() {
        assert!(is_rec2020(Primaries::BT2020, Transfer::Bt1886));
        assert!(is_rec2020(Primaries::Unspecified, Transfer::Pq));
        assert!(!is_rec2020(Primaries::Unspecified, Transfer::Bt1886));
        assert!(!is_rec2020(Primaries::BT709, Transfer::Hlg));
    }

    #[test]
    fn 色空間の名前は_hdr_なら伝達特性で決める() {
        assert_eq!(
            color_space_of(Matrix::Bt2020, Transfer::Pq),
            ColorSpace::bt2100_pq()
        );
        assert_eq!(
            color_space_of(Matrix::Bt2020, Transfer::Hlg),
            ColorSpace::bt2100_hlg()
        );
        assert_eq!(
            color_space_of(Matrix::Bt709, Transfer::Bt1886),
            ColorSpace::bt709()
        );
    }

    #[test]
    fn 行列は宣言を優先し_未指定は高さで決める() {
        assert_eq!(Matrix::from_frame(Space::BT2020NCL, 480), Matrix::Bt2020);
        assert_eq!(Matrix::from_frame(Space::Unspecified, 1080), Matrix::Bt709);
        assert_eq!(Matrix::from_frame(Space::Unspecified, 480), Matrix::Bt601);
    }
}
