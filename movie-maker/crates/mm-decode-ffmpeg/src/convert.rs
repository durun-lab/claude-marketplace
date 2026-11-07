//! デコードしたフレーム（YUV など）を CPU の RGBA f32（作業色空間の線形）にする。
//!
//! 8 bit の SDR（Rec.709 の原色）は swscale で 8 bit の RGBA にして 256 段の表で線形にする。10 bit 以上、HDR（PQ と
//! HLG）、Rec.2020 の原色の素材は 8 bit を経由せず、16 bit の 4:4:4 の YUV から線形にする（`wide`）。

use ff::format::Pixel;
use ff::software::scaling::{Context as Scaler, Flags};
use ff::{ffi, frame};
use ffmpeg_next as ff;
use mm_decode_ffmpeg_core::matrix::{is_full_range, is_rec2020, Matrix, Transfer};
use mm_types::transfer::code8_table;
use mm_types::{MmError, Result, RgbaImage, Size};

use crate::wide::WideYuv;

/// 変換器の作り直しが要るかを決める、入力の性質。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceKey {
    format: Pixel,
    width: u32,
    height: u32,
    matrix: Matrix,
    full_range: bool,
    transfer: Transfer,
    rec2020: bool,
    bits: u32,
}

impl SourceKey {
    fn of(src: &frame::Video) -> Self {
        let transfer = Transfer::from_characteristic(src.color_transfer_characteristic());
        Self {
            format: src.format(),
            width: src.width(),
            height: src.height(),
            matrix: Matrix::from_frame(src.color_space(), src.height()),
            full_range: is_full_range(src.color_range()),
            transfer,
            rec2020: is_rec2020(src.color_primaries(), transfer),
            bits: bit_depth(src.format()),
        }
    }

    /// 8 bit を経由しない経路か。
    fn is_wide(self) -> bool {
        self.bits > 8 || self.transfer.is_hdr() || self.rec2020
    }

    /// swscale の出力。広い経路は色差を補間しただけの 16 bit の YUV（行列とレンジは `wide` が解く）。
    fn target(self) -> Pixel {
        if self.is_wide() {
            Pixel::YUVA444P16LE
        } else {
            Pixel::RGBA
        }
    }
}

/// 画素形式の 1 成分のビット数。記述子が無い形式は 8。
fn bit_depth(format: Pixel) -> u32 {
    // SAFETY: av_pix_fmt_desc_get は静的な記述子へのポインタか null を返す。
    unsafe {
        let desc = ffi::av_pix_fmt_desc_get(format.into());
        if desc.is_null() {
            8
        } else {
            (*desc).comp[0].depth as u32
        }
    }
}

/// swscale の色空間の番号。
fn sws_id(matrix: Matrix) -> i32 {
    match matrix {
        Matrix::Bt601 => ffi::SWS_CS_ITU601,
        Matrix::Bt709 => ffi::SWS_CS_ITU709,
        Matrix::Bt2020 => ffi::SWS_CS_BT2020,
    }
}

/// swscale を 1 つ持ち、入力の性質が変わったときだけ作り直す。
pub struct RgbaConverter {
    scaler: Option<(SourceKey, Scaler)>,
    out: Option<frame::Video>,
    /// 8 bit の符号化値 → 線形。作業色空間は線形の premultiplied なので、アルファを掛ける前に適用する。
    linear: [f32; 256],
}

impl Default for RgbaConverter {
    fn default() -> Self {
        Self {
            scaler: None,
            out: None,
            linear: code8_table(),
        }
    }
}

// swscale のコンテキストは生ポインタを持つため自動では Send にならない。所有は 1 つのデコーダーだけで、
// FFmpeg のコンテキストはスレッドに紐づかないので、デコーダーごと別スレッドへ移してよい。
unsafe impl Send for RgbaConverter {}

impl RgbaConverter {
    /// 1 フレームを RGBA の画像にする。アルファがあれば premultiplied にする。
    pub fn convert(&mut self, src: &frame::Video) -> Result<RgbaImage> {
        let key = SourceKey::of(src);
        if self.scaler.as_ref().map(|(k, _)| *k) != Some(key) {
            self.scaler = Some((key, make_scaler(&key)?));
            self.out = None;
        }
        let (_, scaler) = self.scaler.as_mut().expect("直前に作った");
        let out = self
            .out
            .get_or_insert_with(|| frame::Video::new(key.target(), key.width, key.height));
        scaler
            .run(src, out)
            .map_err(|e| MmError::Corrupt(format!("RGBA への変換: {e}")))?;
        let size = Size::new(key.width, key.height);
        if key.is_wide() {
            WideYuv::new(
                key.matrix,
                key.full_range,
                key.bits,
                key.transfer,
                key.rec2020,
            )
            .to_image(out, size)
        } else {
            to_image(out, size, &self.linear)
        }
    }
}

fn make_scaler(key: &SourceKey) -> Result<Scaler> {
    let mut scaler = Scaler::get(
        key.format,
        key.width,
        key.height,
        key.target(),
        key.width,
        key.height,
        Flags::BICUBIC | Flags::ACCURATE_RND | Flags::FULL_CHR_H_INT,
    )
    .map_err(|e| MmError::Unsupported(format!("{:?} を RGBA にできない: {e}", key.format)))?;
    // swscale の既定は 601 のため、宣言した行列とレンジを明示する。広い経路は YUV のレンジを変えずに通す。
    // RGB 入力では -1 が返るが無視してよい。
    let dst_full = if key.is_wide() {
        key.full_range as i32
    } else {
        1
    };
    unsafe {
        let coeffs = ffi::sws_getCoefficients(sws_id(key.matrix));
        ffi::sws_setColorspaceDetails(
            scaler.as_mut_ptr(),
            coeffs,
            key.full_range as i32,
            coeffs,
            dst_full,
            0,
            1 << 16,
            1 << 16,
        );
    }
    Ok(scaler)
}

/// 8 ビットの RGBA（ストレート）を、線形の premultiplied の f32 にする。
fn to_image(frame: &frame::Video, size: Size, linear: &[f32; 256]) -> Result<RgbaImage> {
    let (w, h) = (size.width as usize, size.height as usize);
    let stride = frame.stride(0);
    let data = frame.data(0);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = &data[y * stride..y * stride + w * 4];
        for px in row.chunks_exact(4) {
            let a = px[3] as f32 / 255.0;
            out.push(linear[px[0] as usize] * a);
            out.push(linear[px[1] as usize] * a);
            out.push(linear[px[2] as usize] * a);
            out.push(a);
        }
    }
    RgbaImage::from_raw(size, out)
}
