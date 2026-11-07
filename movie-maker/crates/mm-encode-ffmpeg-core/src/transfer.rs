//! Rec.1886 の表示の式（`mm_types::transfer`）で、線形から 8 ビットと 16 ビットの符号化値へ丸める表と、
//! HDR（PQ と HLG）の 16 ビットの符号化値へ丸める表。

use mm_types::transfer::{bt1886_to_linear, linear_to_bt1886};

/// ルックアップテーブルの分解能。1 ステップが符号化値の半分の幅より細かくなる暗部の下限（`DARK_LIMIT`）から上で、
/// 符号化値の丸めと一致する。
const LUT_SIZE: usize = 1 << 16;

/// これより暗い線形の値はルックアップテーブルの 1 ステップが符号化値の幅を超えるので、境界の二分探索で決める。
const DARK_LIMIT: f32 = 1.0e-4;

/// 線形の値を 8 ビットの符号化値へ丸める表。符号化値どうしの中点の線形値で区切る。
/// 全画素で `powf` を呼ばないので 1080p でも書き出しの足を引かない。
pub struct EncodeTable {
    /// `bounds[c]` は符号化値 `c` と `c + 1` の境界。
    bounds: [f32; 255],
    /// 線形の値を `LUT_SIZE - 1` 倍したステップの符号化値。
    lut: Vec<u8>,
}

impl EncodeTable {
    pub fn new() -> Self {
        let bounds = std::array::from_fn(|c| bt1886_to_linear((c as f32 + 0.5) / 255.0));
        let lut = (0..LUT_SIZE)
            .map(|i| search(&bounds, (i as f32 + 0.5) / (LUT_SIZE - 1) as f32))
            .collect();
        Self { bounds, lut }
    }

    /// 線形の値 → 符号化値。範囲外は端に丸め、NaN は 0。
    pub fn encode(&self, linear: f32) -> u8 {
        if linear.is_nan() || linear < DARK_LIMIT {
            return search(&self.bounds, linear);
        }
        // `as` は飽和するので 1 を超える値は最後のステップになる
        self.lut[((linear * (LUT_SIZE - 1) as f32) as usize).min(LUT_SIZE - 1)]
    }
}

/// 16 ビットの表で、これより暗い線形の値は式で求める。表の 1 ステップ（`1 / LUT_SIZE`）の符号化値の幅が、10 ビットの
/// 半ステップ（`TOLERANCE_16` = 32）を超える暗さの上限。
const DARK_LIMIT_16: f32 = 1.0e-3;
/// 16 ビットの表と式のずれの上限（16 ビットの符号化値で、10 ビットの半ステップ）。
#[cfg(test)]
const TOLERANCE_16: i32 = 32;

/// 線形の値を 16 ビットの符号化値へ。10 ビットの YUV へ縮める前の精度を保つ（8 ビットを経由すると 10 ビットの階調が出ない）。
pub struct EncodeTable16 {
    lut: Vec<u16>,
}

impl EncodeTable16 {
    pub fn new() -> Self {
        let lut = (0..LUT_SIZE)
            .map(|i| exact16((i as f32 + 0.5) / (LUT_SIZE - 1) as f32))
            .collect();
        Self { lut }
    }

    /// 線形の値 → 符号化値。範囲外は端に丸め、NaN は 0。暗部は表の 1 ステップが粗いので式で求める。
    pub fn encode(&self, linear: f32) -> u16 {
        if linear.is_nan() || linear < DARK_LIMIT_16 {
            return exact16(linear);
        }
        self.lut[((linear * (LUT_SIZE - 1) as f32) as usize).min(LUT_SIZE - 1)]
    }
}

impl Default for EncodeTable16 {
    fn default() -> Self {
        Self::new()
    }
}

/// HDR の表の下端と上端の 2 のべき（線形）。下端より暗い値は式で求め、上端（約 26000 cd/m²）より明るい値は
/// PQ でも HLG でも符号化値の上限に切る。
const HDR_MIN_EXP: i32 = -14;
const HDR_MAX_EXP: i32 = 7;
/// HDR の表の 1 オクターブの段の数（仮数の上位のビット数）。1 段の相対の幅 1/1024 で、PQ と HLG の 10 ビットの
/// 1 段の 1/4 に収まる。
const HDR_MANTISSA_BITS: u32 = 10;
/// f32 の仮数のビット数と指数の偏り。
const F32_MANTISSA_BITS: u32 = 23;
const F32_EXP_BIAS: i32 = 127;

/// 線形の値を PQ か HLG の 16 ビットの符号化値へ。HDR の値は 0..1 に収まらないので、線形の値でなく
/// f32 の指数と仮数の上位で引く（オクターブごとに同じ数の段を持つ対数の表）。
pub struct EncodeTableHdr {
    encode: fn(f32) -> f32,
    lut: Vec<u16>,
}

impl EncodeTableHdr {
    /// `encode` は線形から符号化値への式（`mm_types::transfer::linear_to_pq` など）。
    pub fn new(encode: fn(f32) -> f32) -> Self {
        let octaves = (HDR_MAX_EXP - HDR_MIN_EXP) as usize;
        let steps = 1usize << HDR_MANTISSA_BITS;
        let half_step = 1u32 << (F32_MANTISSA_BITS - HDR_MANTISSA_BITS - 1);
        let lut = (0..octaves * steps)
            .map(|i| {
                let exp = (i / steps) as i32 + HDR_MIN_EXP + F32_EXP_BIAS;
                let mantissa = ((i % steps) as u32) << (F32_MANTISSA_BITS - HDR_MANTISSA_BITS);
                let center =
                    f32::from_bits(((exp as u32) << F32_MANTISSA_BITS) | mantissa | half_step);
                code16(encode(center))
            })
            .collect();
        Self { encode, lut }
    }

    /// 線形の値 → 符号化値。負と NaN は 0、上限を超える値は 65535。
    pub fn encode(&self, linear: f32) -> u16 {
        if linear.is_nan() || linear <= 0.0 {
            return 0;
        }
        let bits = linear.to_bits();
        let exp = ((bits >> F32_MANTISSA_BITS) & 0xff) as i32 - F32_EXP_BIAS;
        if exp < HDR_MIN_EXP {
            return code16((self.encode)(linear));
        }
        if exp >= HDR_MAX_EXP {
            return u16::MAX;
        }
        let mantissa =
            (bits >> (F32_MANTISSA_BITS - HDR_MANTISSA_BITS)) & ((1 << HDR_MANTISSA_BITS) - 1);
        self.lut[((exp - HDR_MIN_EXP) as usize) << HDR_MANTISSA_BITS | mantissa as usize]
    }
}

/// 0..1 の符号化値を 16 ビットに丸める。範囲外は端に切る。
fn code16(e: f32) -> u16 {
    (e.clamp(0.0, 1.0) * 65535.0).round() as u16
}

fn exact16(linear: f32) -> u16 {
    if linear.is_nan() {
        return 0;
    }
    (linear_to_bt1886(linear.clamp(0.0, 1.0)) * 65535.0).round() as u16
}

fn search(bounds: &[f32; 255], linear: f32) -> u8 {
    bounds.partition_point(|b| *b <= linear) as u8
}

impl Default for EncodeTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 符号化値の線形を戻すと元の符号化値になる() {
        let table = EncodeTable::new();
        for code in 0..=255u32 {
            let linear = mm_types::transfer::code8_to_linear(code as u8);
            assert_eq!(u32::from(table.encode(linear)), code);
        }
    }

    #[test]
    fn 全域で式の丸めと高々_1_ステップしかずれない() {
        let table = EncodeTable::new();
        let mut v = 1.0e-7f32;
        while v < 1.0 {
            let exact = i32::from(mm_types::transfer::linear_to_code8(v));
            let got = i32::from(table.encode(v));
            assert!((got - exact).abs() <= 1, "線形 {v}: 式 {exact} 表 {got}");
            v *= 1.003;
        }
    }

    #[test]
    fn 十六ビットの表は式と_10_ビットの半ステップ以内しかずれない() {
        let table = EncodeTable16::new();
        let mut v = 1.0e-7f32;
        while v < 1.0 {
            let exact = i32::from(exact16(v));
            let got = i32::from(table.encode(v));
            assert!(
                (got - exact).abs() <= TOLERANCE_16,
                "線形 {v}: 式 {exact} 表 {got}"
            );
            v *= 1.003;
        }
        assert_eq!(table.encode(1.0), 65535);
        assert_eq!(table.encode(-1.0), 0);
        assert_eq!(table.encode(f32::NAN), 0);
    }

    #[test]
    fn hdr_の表は式と_10_ビットの_1_段の_4分の1以内しかずれない() {
        use mm_types::transfer::{linear_to_hlg, linear_to_pq};
        for encode in [linear_to_pq as fn(f32) -> f32, linear_to_hlg] {
            let table = EncodeTableHdr::new(encode);
            let mut v = 1.0e-6f32;
            while v < 60.0 {
                let exact = i32::from(code16(encode(v)));
                let got = i32::from(table.encode(v));
                assert!(
                    (got - exact).abs() <= TOLERANCE_16 / 2,
                    "線形 {v}: 式 {exact} 表 {got}"
                );
                v *= 1.001;
            }
            assert_eq!(table.encode(0.0), 0);
            assert_eq!(table.encode(-1.0), 0);
            assert_eq!(table.encode(f32::NAN), 0);
            assert_eq!(table.encode(1.0e6), u16::MAX);
        }
    }

    #[test]
    fn 範囲外は端に丸める() {
        let table = EncodeTable::new();
        assert_eq!(table.encode(-1.0), 0);
        assert_eq!(table.encode(2.0), 255);
        assert_eq!(table.encode(f32::NAN), 0);
    }
}
