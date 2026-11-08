//! 線形の RGBA の f32 を、エンコーダーの信号の符号化値の RGBA（8 ビットか 16 ビット）のフレームにする。
//! スケーラーがそれを YUV にする。

use ff::format::Pixel;
use ff::frame;
use ffmpeg_next as ff;
use mm_types::tonemap::bt2390_to_sdr;
use mm_types::transfer::{linear_to_hlg, linear_to_pq, mul3, REC709_TO_REC2020};
use mm_types::RgbaImage;

use crate::spec::{CodecSpec, OutputSignal};
use crate::transfer::{EncodeTable, EncodeTable16, EncodeTableHdr};

/// 線形 → 符号化値の表。8 ビットは NV12 向け、16 ビットは 10 ビットの YUV 向け、HDR は Rec.2020 の PQ か HLG。
/// SDR の 2 つは、符号化の前にプレビューと同じトーンマップ（素材の上限 `peak`）を掛ける。
pub(crate) enum Encoding {
    /// 境界の表（255 個）を持つので、ほかの形と大きさを揃えるためBox に入れる。
    Code8(Box<EncodeTable>, f32),
    Code16(EncodeTable16, f32),
    Hdr(EncodeTableHdr),
}

impl Encoding {
    /// コーデックの指定の画素形式と信号、素材の HDR の上限（`EncodeSettings::source_peak`）から。
    pub(crate) fn for_spec(spec: &CodecSpec, source_peak: f32) -> Self {
        match spec.signal {
            OutputSignal::Pq => Self::Hdr(EncodeTableHdr::new(linear_to_pq)),
            OutputSignal::Hlg => Self::Hdr(EncodeTableHdr::new(linear_to_hlg)),
            OutputSignal::Sdr if spec.input.is_high_bit_depth() => {
                Self::Code16(EncodeTable16::new(), source_peak)
            }
            OutputSignal::Sdr => Self::Code8(Box::default(), source_peak),
        }
    }

    /// スケーラーに渡す RGBA の形式。
    pub(crate) fn pixel(&self) -> Pixel {
        match self {
            Self::Code8(..) => Pixel::RGBA,
            Self::Code16(..) | Self::Hdr(_) => Pixel::RGBA64LE,
        }
    }

    /// 1 フレームを符号化値のフレームにする。SDR はスケーラーに渡す RGBA、HDR はエンコーダーにそのまま渡す
    /// 10 bit の 4:2:0 の YUV（swscale は 16 bit の RGB を桁送りで 10 bit に縮め、PQ の上端で符号語が 3 工程ほど
    /// ずれるので、HDR の行列とレンジはここで解く）。
    pub(crate) fn prepare(&self, image: &RgbaImage) -> Prepared {
        let rgba = |fill: &dyn Fn(&mut frame::Video)| {
            let mut f = frame::Video::new(self.pixel(), image.width(), image.height());
            fill(&mut f);
            Prepared::Rgba(f)
        };
        match self {
            Self::Code8(t, peak) => rgba(&|f| {
                fill_rgba(f, image, |rgb| {
                    bt2390_to_sdr(rgb, *peak).map(|c| t.encode(c))
                })
            }),
            Self::Code16(t, peak) => rgba(&|f| {
                fill_rgba64(f, image, |rgb| {
                    bt2390_to_sdr(rgb, *peak).map(|c| t.encode(c))
                })
            }),
            Self::Hdr(t) => Prepared::Yuv(fill_yuv420p10(image, t)),
        }
    }
}

/// [`Encoding::prepare`] の結果。
pub(crate) enum Prepared {
    /// スケーラーで YUV にする RGBA。
    Rgba(frame::Video),
    /// エンコーダーの画素形式の YUV。
    Yuv(frame::Video),
}

/// Rec.2020 の非定輝度の輝度の係数（Kr, Kb）。
const REC2020_KR: f32 = 0.2627;
const REC2020_KB: f32 = 0.0593;
/// 10 bit の tv レンジの輝度の黒と幅、色差の中央と幅。
const Y_BLACK10: f32 = 64.0;
const Y_RANGE10: f32 = 876.0;
const C_MID10: f32 = 512.0;
const C_RANGE10: f32 = 896.0;

/// 線形の画像を Rec.2020 の PQ か HLG の符号化値にし、Rec.2020 の行列の 10 bit の tv レンジの YUV 4:2:0 にする。
/// 色差は 2x2 の符号化値の平均から作る（GPU の NV12 の書き出しと同じ置き方）。
fn fill_yuv420p10(image: &RgbaImage, table: &EncodeTableHdr) -> frame::Video {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let encoded: Vec<[f32; 3]> = image
        .data()
        .chunks_exact(4)
        .map(|px| {
            mul3(&REC709_TO_REC2020, [px[0], px[1], px[2]])
                .map(|c| f32::from(table.encode(c)) / f32::from(u16::MAX))
        })
        .collect();
    let mut f = frame::Video::new(Pixel::YUV420P10LE, w as u32, h as u32);
    let stride = f.stride(0);
    let plane = f.data_mut(0);
    for (y, row) in encoded.chunks_exact(w).enumerate() {
        for (x, e) in row.iter().enumerate() {
            let i = y * stride + x * 2;
            plane[i..i + 2].copy_from_slice(&code10(Y_BLACK10 + Y_RANGE10 * luma2020(*e)));
        }
    }
    // 色差のプレーン 1 は Cb（B − Y）、2 は Cr（R − Y）
    for (p, channel, kc) in [(1, 2, REC2020_KB), (2, 0, REC2020_KR)] {
        let stride = f.stride(p);
        let plane = f.data_mut(p);
        for cy in 0..h.div_ceil(2) {
            for cx in 0..w.div_ceil(2) {
                let avg = average_2x2(&encoded, w, h, cx * 2, cy * 2);
                let diff = (avg[channel] - luma2020(avg)) / (2.0 * (1.0 - kc));
                let i = cy * stride + cx * 2;
                plane[i..i + 2].copy_from_slice(&code10(C_MID10 + C_RANGE10 * diff));
            }
        }
    }
    f
}

/// Rec.2020 の非定輝度の輝度（符号化値から）。
fn luma2020(e: [f32; 3]) -> f32 {
    REC2020_KR * e[0] + (1.0 - REC2020_KR - REC2020_KB) * e[1] + REC2020_KB * e[2]
}

/// (x, y) を左上とする 2x2 の平均。端は最後の行と列を繰り返す。
fn average_2x2(encoded: &[[f32; 3]], w: usize, h: usize, x: usize, y: usize) -> [f32; 3] {
    let mut sum = [0.0; 3];
    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let e = encoded[(y + dy).min(h - 1) * w + (x + dx).min(w - 1)];
        sum = std::array::from_fn(|c| sum[c] + e[c]);
    }
    sum.map(|s| s / 4.0)
}

/// 10 bit の符号語に丸めて、リトルエンディアンの 2 バイトにする。
fn code10(v: f32) -> [u8; 2] {
    (v.round().clamp(0.0, 1023.0) as u16).to_le_bytes()
}

/// 線形の f32 の画素を `encode` で Rec.1886 の 8 ビットにして RGBA のフレームへコピーする。
/// アルファは使わず不透明として書く（premultiplied なので黒の上に置いた色と同じ）。
fn fill_rgba(dst: &mut frame::Video, image: &RgbaImage, encode: impl Fn([f32; 3]) -> [u8; 3]) {
    let width = image.width() as usize;
    let stride = dst.stride(0);
    let data = dst.data_mut(0);
    for (y, row) in image.data().chunks_exact(width * 4).enumerate() {
        let out = &mut data[y * stride..y * stride + width * 4];
        for (o, px) in out.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
            o[..3].copy_from_slice(&encode([px[0], px[1], px[2]]));
            o[3] = 255;
        }
    }
}

/// `fill_rgba` の 16 ビットにしたもの（リトルエンディアン）。`encode` が線形の RGB を符号化値にする。
fn fill_rgba64(dst: &mut frame::Video, image: &RgbaImage, encode: impl Fn([f32; 3]) -> [u16; 3]) {
    let width = image.width() as usize;
    let stride = dst.stride(0);
    let data = dst.data_mut(0);
    for (y, row) in image.data().chunks_exact(width * 4).enumerate() {
        let out = &mut data[y * stride..y * stride + width * 8];
        for (o, px) in out.chunks_exact_mut(8).zip(row.chunks_exact(4)) {
            let code = encode([px[0], px[1], px[2]]);
            for c in 0..3 {
                o[c * 2..c * 2 + 2].copy_from_slice(&code[c].to_le_bytes());
            }
            o[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
        }
    }
}
