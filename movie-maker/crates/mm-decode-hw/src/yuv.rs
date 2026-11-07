//! YUV 4:2:0（NV12 と P010）から、線形の RGB への変換の係数と WGSL。
//!
//! CPU の経路（`mm-decode-ffmpeg`）と同じく、符号化値に伝達特性の式（SDR は Rec.1886、HDR は PQ と HLG。
//! `mm_types::transfer`）を当てて線形にし、Rec.2020 の原色なら作業色空間（Rec.709）の原色に直す。
//! 10 bit は符号語のまま f32 で扱い、8 bit に下げない。色差は左寄せ（MPEG-2 の配置）の位置から線形に補間する。

use ffmpeg_next as ff;
use mm_decode_ffmpeg_core::matrix::{is_rec2020, range_codes, Matrix, Transfer};
use mm_types::transfer::{mul3, REC2020_TO_REC709};

/// 素材のビット深度。NV12 と P010 に載る 4:2:0 の 8 bit と 10 bit だけ。
pub fn stream_bit_depth(stream: &ff::format::stream::Stream<'_>) -> mm_types::Result<u32> {
    use ff::ffi::AVPixelFormat as P;
    // SAFETY: パラメーターは有効なストリームのもの。format は AVPixelFormat の整数値。
    let raw = unsafe { (*stream.parameters().as_ptr()).format };
    let is = |f: P| raw == f as i32;
    if is(P::AV_PIX_FMT_YUV420P) || is(P::AV_PIX_FMT_YUVJ420P) {
        Ok(8)
    } else if is(P::AV_PIX_FMT_YUV420P10LE) {
        Ok(10)
    } else {
        Err(mm_types::MmError::Unsupported(format!(
            "4:2:0 の 8 bit と 10 bit だけ扱える（画素形式 {raw}）"
        )))
    }
}

/// 輝度の幅と高さから、4:2:0 の色差プレーンの幅と高さ。奇数なら切り上げる。
pub fn chroma_size(width: u32, height: u32) -> (u32, u32) {
    (width.div_ceil(2), height.div_ceil(2))
}

/// 変換のパラメーター。符号語の単位（8 bit なら 0..255、10 bit なら 0..1023）で持つ。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YuvParams {
    /// テクスチャの値（0..1）→ 符号語。バッファを直接読むときは 1。
    pub code_scale: f32,
    pub y_off: f32,
    pub y_range: f32,
    pub c_mid: f32,
    pub c_range: f32,
    pub rv: f32,
    pub gu: f32,
    pub gv: f32,
    pub bu: f32,
    /// 色差プレーンの画のある幅と高さ（画素）。プレーンの余白を補間に混ぜないための端。
    pub chroma_w: f32,
    pub chroma_h: f32,
    /// 符号化値を線形にする式。
    pub transfer: Transfer,
    /// 線形にした後で Rec.2020 の原色から Rec.709 へ行列を掛けるか。
    pub rec2020: bool,
}

/// 一様バッファに置くときのバイト数（`vec4` 4 つ分）。
pub const PARAMS_BYTES: usize = 64;

impl YuvParams {
    /// 行列、レンジ、ビット深度、輝度の大きさから。`code_scale` は呼び出し側が入れる。
    pub fn new(
        matrix: Matrix,
        full_range: bool,
        bits: u32,
        code_scale: f32,
        size: (u32, u32),
    ) -> Self {
        let [y_off, y_range, c_mid, c_range] = range_codes(full_range, bits);
        let [rv, gu, gv, bu] = matrix.rgb_coefficients();
        let (chroma_w, chroma_h) = chroma_size(size.0, size.1);
        Self {
            code_scale,
            y_off,
            y_range,
            c_mid,
            c_range,
            rv,
            gu,
            gv,
            bu,
            chroma_w: chroma_w as f32,
            chroma_h: chroma_h as f32,
            transfer: Transfer::Bt1886,
            rec2020: false,
        }
    }

    /// 伝達特性と原色を決める。既定は SDR の Rec.1886、Rec.709 の原色。
    pub fn with_signal(mut self, transfer: Transfer, rec2020: bool) -> Self {
        self.transfer = transfer;
        self.rec2020 = rec2020;
        self
    }

    /// フレームの宣言（伝達特性と原色）から [`Self::with_signal`] を決める。GPU のフレームも宣言を持つ。
    pub fn with_frame_signal(self, frame: &ff::frame::Video) -> Self {
        let transfer = Transfer::from_characteristic(frame.color_transfer_characteristic());
        self.with_signal(transfer, is_rec2020(frame.color_primaries(), transfer))
    }

    /// 16 bit のテクスチャ（P010 は上位 10 bit に詰める）の値 → 符号語の倍率。
    pub fn texture_code_scale(bits: u32) -> f32 {
        if bits == 8 {
            255.0
        } else {
            65535.0 / (1u32 << (16 - bits)) as f32
        }
    }

    /// 一様バッファの中身。
    pub fn to_bytes(self) -> [u8; PARAMS_BYTES] {
        let fields = [
            self.code_scale,
            self.y_off,
            self.y_range,
            self.c_mid,
            self.c_range,
            self.rv,
            self.gu,
            self.gv,
            self.bu,
            self.chroma_w,
            self.chroma_h,
            self.transfer.code(),
            f32::from(u8::from(self.rec2020)),
            0.0,
            0.0,
            0.0,
        ];
        let mut out = [0u8; PARAMS_BYTES];
        for (chunk, v) in out.chunks_exact_mut(4).zip(fields) {
            chunk.copy_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// 符号語から線形の RGB（CPU で同じ式を適用する確かめ用）。
    pub fn to_linear(self, y: f32, u: f32, v: f32) -> [f32; 3] {
        let luma = (y - self.y_off) / self.y_range;
        let cb = (u - self.c_mid) / self.c_range;
        let cr = (v - self.c_mid) / self.c_range;
        let rgb = [
            luma + self.rv * cr,
            luma - self.gu * cb - self.gv * cr,
            luma + self.bu * cb,
        ];
        let linear = rgb.map(|c| self.transfer.to_linear(c.clamp(0.0, 1.0)));
        if self.rec2020 {
            mul3(&REC2020_TO_REC709, linear)
        } else {
            linear
        }
    }
}

/// WGSL の共通部。呼ぶ側が `var<uniform> params: Params` を宣言し、`mm_types::transfer::TRANSFER_WGSL` を前に連結する。
///
/// `chroma_taps` は画素 (x, y) の色差の補間に使う 4 点と重みを返す。色差は横が輝度の左端、
/// 縦が 2 行の中間にあるので、色差プレーンの座標は (x / 2, (y - 0.5) / 2)。端はプレーンの画のある範囲に留める。
pub const YUV_WGSL: &str = r"
struct Params {
    code_scale: f32,
    y_off: f32,
    y_range: f32,
    c_mid: f32,
    c_range: f32,
    rv: f32,
    gu: f32,
    gv: f32,
    bu: f32,
    chroma_w: f32,
    chroma_h: f32,
    transfer: f32,
    rec2020: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
}

struct Taps { x0: u32, x1: u32, y0: u32, y1: u32, fx: f32, fy: f32 }

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn chroma_taps(x: u32, y: u32) -> Taps {
    let cx = f32(x) * 0.5;
    let cy = (f32(y) - 0.5) * 0.5;
    let ix = i32(floor(cx));
    let iy = i32(floor(cy));
    let max_x = i32(params.chroma_w) - 1;
    let max_y = i32(params.chroma_h) - 1;
    var t: Taps;
    t.x0 = u32(clamp(ix, 0, max_x));
    t.x1 = u32(clamp(ix + 1, 0, max_x));
    t.y0 = u32(clamp(iy, 0, max_y));
    t.y1 = u32(clamp(iy + 1, 0, max_y));
    t.fx = cx - floor(cx);
    t.fy = cy - floor(cy);
    return t;
}

fn chroma_mix(c00: vec2<f32>, c10: vec2<f32>, c01: vec2<f32>, c11: vec2<f32>, t: Taps) -> vec2<f32> {
    return mix(mix(c00, c10, t.fx), mix(c01, c11, t.fx), t.fy);
}

fn yuv_to_linear(y: f32, u: f32, v: f32) -> vec4<f32> {
    let luma = (y - params.y_off) / params.y_range;
    let cb = (u - params.c_mid) / params.c_range;
    let cr = (v - params.c_mid) / params.c_range;
    let rgb = clamp(
        vec3<f32>(luma + params.rv * cr, luma - params.gu * cb - params.gv * cr, luma + params.bu * cb),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    var lin = mm_bt1886_to_linear3(rgb);
    if params.transfer == 1.0 {
        lin = mm_pq_to_linear3(rgb);
    } else if params.transfer == 2.0 {
        lin = mm_hlg_to_linear3(rgb);
    }
    if params.rec2020 == 1.0 {
        lin = mm_rec2020_to_rec709(lin);
    }
    return vec4<f32>(lin, 1.0);
}
";

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (u32, u32) = (320, 240);

    #[test]
    fn 限定レンジの白と黒が_0_と_1_になる() {
        let p = YuvParams::new(Matrix::Bt709, false, 8, 255.0, SIZE);
        let white = p.to_linear(235.0, 128.0, 128.0);
        let black = p.to_linear(16.0, 128.0, 128.0);
        assert!(white.iter().all(|c| (c - 1.0).abs() < 1e-5), "{white:?}");
        assert!(black.iter().all(|c| c.abs() < 1e-5), "{black:?}");
    }

    #[test]
    fn 十ビットは八ビットと同じ明るさを細かい段で表す() {
        let p8 = YuvParams::new(Matrix::Bt709, false, 8, 255.0, SIZE);
        let p10 = YuvParams::new(
            Matrix::Bt709,
            false,
            10,
            YuvParams::texture_code_scale(10),
            SIZE,
        );
        let a = p8.to_linear(126.0, 128.0, 128.0)[0];
        let b = p10.to_linear(504.0, 512.0, 512.0)[0];
        assert!((a - b).abs() < 1e-5, "{a} {b}");
        let step8 = p8.to_linear(127.0, 128.0, 128.0)[0] - a;
        let step10 = p10.to_linear(505.0, 512.0, 512.0)[0] - b;
        assert!(step10 < step8 / 3.0, "10 bit の 1 段は 8 bit の 1/4 ほど");
    }

    #[test]
    fn p010_の上位詰めの値が符号語に戻る() {
        let scale = YuvParams::texture_code_scale(10);
        let raw16 = 504u32 << 6;
        let code = raw16 as f32 / 65535.0 * scale;
        assert!((code - 504.0).abs() < 1e-3, "{code}");
    }

    #[test]
    fn 原色の係数は行列の定義に合う() {
        let p = YuvParams::new(Matrix::Bt709, true, 8, 255.0, SIZE);
        // 全レンジの純赤: Y = Kr * 255、Cr = 255、Cb = 128 - (Kr/(1-Kb)) ... 係数で往復して 1 になる
        let (kr, kb) = (0.2126f32, 0.0722f32);
        let y = kr * 255.0;
        let cb = 128.0 + (0.0 - y / 255.0) / (2.0 * (1.0 - kb)) * 255.0;
        let cr = 128.0 + (1.0 - y / 255.0) / (2.0 * (1.0 - kr)) * 255.0;
        let rgb = p.to_linear(y, cb, cr);
        assert!(
            (rgb[0] - 1.0).abs() < 1e-3 && rgb[1] < 1e-3 && rgb[2] < 1e-3,
            "{rgb:?}"
        );
    }

    #[test]
    fn pq_の十ビットは_1_を超える線形の光になる() {
        let p = YuvParams::new(
            Matrix::Bt2020,
            false,
            10,
            YuvParams::texture_code_scale(10),
            SIZE,
        )
        .with_signal(Transfer::Pq, true);
        // tv レンジの 10 bit: 符号化値 e は 64 + 876 * e。PQ の 0.5807 が 203 cd/m²（線形の 1.0）
        let code = |e: f32| 64.0 + 876.0 * e;
        let white = p.to_linear(code(0.580_7), 512.0, 512.0);
        assert!(white.iter().all(|c| (c - 1.0).abs() < 5e-3), "{white:?}");
        let peak = p.to_linear(code(0.751_8), 512.0, 512.0);
        assert!(
            peak.iter().all(|c| (c - 1000.0 / 203.0).abs() < 0.05),
            "1000 cd/m²: {peak:?}"
        );
        let bytes = p.to_bytes();
        let field = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!((field(11), field(12)), (1.0, 1.0), "伝達特性と原色の番号");
    }

    #[test]
    fn 奇数の大きさの色差プレーンは切り上げる() {
        assert_eq!(chroma_size(320, 240), (160, 120));
        assert_eq!(chroma_size(321, 241), (161, 121));
        assert_eq!(chroma_size(1, 1), (1, 1));
    }

    #[test]
    fn 色差の端の大きさが一様バッファに入る() {
        let p = YuvParams::new(Matrix::Bt709, false, 8, 255.0, (321, 241));
        let bytes = p.to_bytes();
        let field = |i: usize| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        assert_eq!((field(9), field(10)), (161.0, 121.0));
    }
}
