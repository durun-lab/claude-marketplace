//! 書き出しの描画の段が符号化の段へ渡すフレーム（[`crate::Renderer::export_frame`]）。
//!
//! 描画の段は GPU にリードバックを出して待たずに次のフレームへ進み、数フレーム遅れて [`PendingFrame::finish`] で
//! 受け取る。受け取ったフレームは `Engine` を持たない符号化の段（`mm-export`）がエンコーダーに押す。

use mm_decode_api::Nv12Frame;
use mm_node_api::GpuHelper;
use mm_types::{Result, RgbaImage};

/// 符号化の段に渡す 1 フレーム。
pub enum ExportFrame {
    /// 線形の premultiplied の RGBA。NV12 を受けないエンコーダーと、GPU の無い実装が使う。
    Rgba(RgbaImage),
    /// Rec.709、tv レンジの NV12。SDR の書き出しのトーンマップを掛けた後の値。
    Nv12(Nv12Frame),
}

/// リードバックを待っている書き出しのフレーム。
pub trait PendingFrame: Send {
    /// リードバックの完了を待ち、符号化に渡す形にする。`gpu` は描いたレンダラーのもの（`Renderer::gpu`）。
    fn finish(self: Box<Self>, gpu: &mut dyn GpuHelper) -> Result<ExportFrame>;
}

/// その場でリードバックを済ませた実装は、出来上がったフレームをそのまま返す。
impl PendingFrame for ExportFrame {
    fn finish(self: Box<Self>, _gpu: &mut dyn GpuHelper) -> Result<ExportFrame> {
        Ok(*self)
    }
}
