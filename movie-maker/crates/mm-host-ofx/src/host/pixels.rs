//! プラグインに渡す画素の列と、`RgbaImage`（RGBA f32、premultiplied、線形、行は上から）との変換。
//! OpenFX の画像は行が下から（`kOfxImageEffectHostPropNativeOrigin` が左下）なので、行の順を逆にする。
//! 整数の深さ（Byte、Short）は値を 0〜1 に切って量子化するだけで、伝達関数は掛けない（色の管理は段階 8）。

use mm_types::{Result, RgbaImage, Size};

use super::names::{DEPTH_BYTE, DEPTH_FLOAT, DEPTH_SHORT};

/// 1 画素の成分の数（RGBA）。
const CHANNELS: usize = 4;
/// 割り当ての単位。`u64` で持ち、`f32` の読み書きに要る並びをそろえる。
const WORD: usize = std::mem::size_of::<u64>();

/// 画素の深さ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    Byte,
    Short,
    Float,
}

impl Depth {
    /// 精度の高い順。プラグインが対応するもののうち最初を選ぶ。
    const PREFERRED: [Self; 3] = [Self::Float, Self::Short, Self::Byte];

    pub fn ofx_name(self) -> &'static str {
        match self {
            Self::Byte => DEPTH_BYTE,
            Self::Short => DEPTH_SHORT,
            Self::Float => DEPTH_FLOAT,
        }
    }

    fn bytes(self) -> usize {
        match self {
            Self::Byte => 1,
            Self::Short => 2,
            Self::Float => 4,
        }
    }

    /// プラグインが対応する深さ（`OfxBitDepth*` の名前）から選ぶ。
    pub fn choose(supported: &[String]) -> Option<Self> {
        Self::PREFERRED
            .into_iter()
            .find(|d| supported.iter().any(|s| s == d.ofx_name()))
    }

    fn encode(self, v: f32, out: &mut [u8]) {
        match self {
            Self::Float => out.copy_from_slice(&v.to_ne_bytes()),
            Self::Short => out.copy_from_slice(&quantize(v, u16::MAX).to_ne_bytes()),
            Self::Byte => out[0] = quantize(v, u16::from(u8::MAX)) as u8,
        }
    }

    /// プラグインが書いた有限でない値（NaN、Inf）は 0 にする。JSON は NaN を書けず、応答が読めなくなるため。
    fn decode(self, b: &[u8]) -> f32 {
        match self {
            Self::Float => {
                let v = f32::from_ne_bytes([b[0], b[1], b[2], b[3]]);
                if v.is_finite() {
                    v
                } else {
                    0.0
                }
            }
            Self::Short => f32::from(u16::from_ne_bytes([b[0], b[1]])) / f32::from(u16::MAX),
            Self::Byte => f32::from(b[0]) / f32::from(u8::MAX),
        }
    }
}

fn quantize(v: f32, max: u16) -> u16 {
    (v.clamp(0.0, 1.0) * f32::from(max)).round() as u16
}

/// ホストが持つ画素の列。プラグインは `data_ptr` を通して書くので、Rust の参照を作らずに生のポインタで持つ。
#[derive(Debug)]
pub struct Pixels {
    data: *mut u64,
    words: usize,
    pub depth: Depth,
    width: u32,
    height: u32,
}

// SAFETY: data はこの値だけが持つ割り当てで、プラグインの書き込みは描画の action の間に限られる。
unsafe impl Send for Pixels {}
// SAFETY: 同上。ホストが読むのは描画の action が戻った後。
unsafe impl Sync for Pixels {}

impl Pixels {
    /// 0 で埋めた `bytes` バイト以上の割り当て（画像のメモリの suite 用）。
    pub fn raw(bytes: usize) -> std::result::Result<Self, String> {
        Self::alloc(bytes, Depth::Byte, 0, 0)
    }

    fn alloc(
        bytes: usize,
        depth: Depth,
        width: u32,
        height: u32,
    ) -> std::result::Result<Self, String> {
        let words = bytes.div_ceil(WORD).max(1);
        let mut buf = Vec::new();
        buf.try_reserve_exact(words).map_err(|e| e.to_string())?;
        buf.resize(words, 0u64);
        let data = Box::into_raw(buf.into_boxed_slice()).cast::<u64>();
        Ok(Self {
            data,
            words,
            depth,
            width,
            height,
        })
    }

    /// 0（透明）で埋めた画像。
    pub fn new(size: Size, depth: Depth) -> std::result::Result<Self, String> {
        let bytes = (size.width as usize)
            .checked_mul(size.height as usize)
            .and_then(|n| n.checked_mul(CHANNELS * depth.bytes()))
            .ok_or_else(|| format!("{}x{} は大きすぎる", size.width, size.height))?;
        Self::alloc(bytes, depth, size.width, size.height)
    }

    /// `RgbaImage` から。
    pub fn from_image(image: &RgbaImage, depth: Depth) -> std::result::Result<Self, String> {
        Self::from_rows(image.size(), image.data(), depth)
    }

    /// 上の行からの RGBA f32 の配列（`RgbaImage` と同じ並び）から。Float の深さは行ごとのコピーで済ませる。
    pub fn from_rows(size: Size, data: &[f32], depth: Depth) -> std::result::Result<Self, String> {
        let mut px = Self::new(size, depth)?;
        let w = size.width as usize * CHANNELS;
        let height = size.height as usize;
        if data.len() != w * height {
            return Err(format!(
                "画素の数 {} が {}x{} と合わない",
                data.len(),
                size.width,
                size.height
            ));
        }
        if depth == Depth::Float {
            let dst = px.floats_mut();
            for (y_top, src) in data.chunks_exact(w).enumerate() {
                dst[(height - 1 - y_top) * w..][..w].copy_from_slice(src);
            }
            return Ok(px);
        }
        let (row, comp) = (px.row_bytes(), depth.bytes());
        let bytes = px.bytes_mut();
        for (y_top, src) in data.chunks_exact(w).enumerate() {
            let dst = &mut bytes[(height - 1 - y_top) * row..][..row];
            for (v, out) in src.iter().zip(dst.chunks_exact_mut(comp)) {
                depth.encode(*v, out);
            }
        }
        Ok(px)
    }

    /// 上の行からの RGBA f32 の配列に書く。有限でない値は 0 にする（`Depth::decode` と同じ）。
    pub fn write_rows(&self, out: &mut [f32]) -> std::result::Result<(), String> {
        let w = self.width as usize * CHANNELS;
        let height = self.height as usize;
        if out.len() != w * height {
            return Err(format!(
                "書き先 {} が {}x{} と合わない",
                out.len(),
                self.width,
                self.height
            ));
        }
        if self.depth == Depth::Float {
            let src = self.floats();
            for (y_top, dst) in out.chunks_exact_mut(w).enumerate() {
                dst.copy_from_slice(&src[(height - 1 - y_top) * w..][..w]);
            }
            out.iter_mut()
                .filter(|v| !v.is_finite())
                .for_each(|v| *v = 0.0);
            return Ok(());
        }
        let (row, comp, bytes) = (self.row_bytes(), self.depth.bytes(), self.bytes());
        for (y_top, dst) in out.chunks_exact_mut(w).enumerate() {
            let src = &bytes[(height - 1 - y_top) * row..][..row];
            for (v, b) in dst.iter_mut().zip(src.chunks_exact(comp)) {
                *v = self.depth.decode(b);
            }
        }
        Ok(())
    }

    /// `RgbaImage` へ。
    pub fn to_image(&self) -> Result<RgbaImage> {
        let size = Size::new(self.width, self.height);
        let mut data = vec![0.0; size.width as usize * size.height as usize * CHANNELS];
        self.write_rows(&mut data)
            .map_err(mm_types::MmError::Internal)?;
        RgbaImage::from_raw(size, data)
    }

    pub fn data_ptr(&self) -> *mut std::ffi::c_void {
        self.data.cast()
    }

    fn row_bytes(&self) -> usize {
        self.width as usize * CHANNELS * self.depth.bytes()
    }

    pub fn row_bytes_i32(&self) -> i32 {
        i32::try_from(self.row_bytes()).unwrap_or(i32::MAX)
    }

    pub fn width_i32(&self) -> i32 {
        i32::try_from(self.width).unwrap_or(i32::MAX)
    }

    pub fn height_i32(&self) -> i32 {
        i32::try_from(self.height).unwrap_or(i32::MAX)
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: words 個の u64 の割り当てで、読むのはプラグインが書き終えた後。
        unsafe { std::slice::from_raw_parts(self.data.cast::<u8>(), self.words * WORD) }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: words 個の u64 の割り当てを、まだプラグインに渡していない間だけ書く。
        unsafe { std::slice::from_raw_parts_mut(self.data.cast::<u8>(), self.words * WORD) }
    }

    /// f32 の並び。割り当ては u64 の境界なので f32 の境界も満たす。
    fn floats(&self) -> &[f32] {
        // SAFETY: bytes と同じ割り当てを f32 の配列として読む。どのビット列も f32 として正しい。
        unsafe { std::slice::from_raw_parts(self.data.cast::<f32>(), self.words * 2) }
    }

    fn floats_mut(&mut self) -> &mut [f32] {
        // SAFETY: bytes_mut と同じ。
        unsafe { std::slice::from_raw_parts_mut(self.data.cast::<f32>(), self.words * 2) }
    }
}

impl Drop for Pixels {
    fn drop(&mut self) {
        // SAFETY: alloc が Box::into_raw で作った words 個の割り当て。
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(self.data, self.words)) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mm_types::Rgba;

    fn gradient() -> RgbaImage {
        let mut img = RgbaImage::transparent(Size::new(3, 2));
        img.set_pixel(0, 0, Rgba::new(1.0, 0.0, 0.0, 1.0));
        img.set_pixel(2, 1, Rgba::new(0.0, 0.5, 1.0, 1.0));
        img
    }

    #[test]
    fn どの深さでも往復する_行は下から() {
        let img = gradient();
        for depth in Depth::PREFERRED {
            let px = Pixels::from_image(&img, depth).unwrap();
            let back = px.to_image().unwrap();
            let tol = if depth == Depth::Byte {
                1.0 / 255.0
            } else {
                1e-4
            };
            assert!(back.max_diff(&img).unwrap() <= tol, "{depth:?}");
        }
        let px = Pixels::from_image(&img, Depth::Float).unwrap();
        // 上の行の左端（赤）は、OpenFX の並びでは最後の行の先頭。
        let last_row = px.row_bytes();
        assert_eq!(px.bytes()[last_row..last_row + 4], 1.0f32.to_ne_bytes());
    }

    #[test]
    fn 有限でない値は_0_にして返す() {
        let mut px = Pixels::new(Size::new(1, 1), Depth::Float).unwrap();
        let written = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.5];
        for (v, out) in written.iter().zip(px.bytes_mut().chunks_exact_mut(4)) {
            out.copy_from_slice(&v.to_ne_bytes());
        }
        assert_eq!(px.to_image().unwrap().data(), [0.0, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn 深さは精度の高い順に選ぶ() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            Depth::choose(&names(&[DEPTH_BYTE, DEPTH_FLOAT])),
            Some(Depth::Float)
        );
        assert_eq!(Depth::choose(&names(&[DEPTH_BYTE])), Some(Depth::Byte));
        assert_eq!(Depth::choose(&names(&["OfxBitDepthHalf"])), None);
    }
}
