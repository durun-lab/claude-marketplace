//! GPU を触る共通実装（RFC 0001）。API クレートは wgpu を出さず、実装どうしはここで wgpu の型を合わせる。
//!
//! - [`GpuContext`]: Device と Queue。アダプターを 1 回選び、レンダラー・デコーダー・ホストで共有する。
//! - [`WgpuGpu`]: インターフェースの `GpuHelper` の wgpu 実装。具象型は `as_any` で降りてテクスチャを使う。
//! - [`GpuFrameTexture`]: デコーダーが返す GPU のフレームの中身。`GpuHelper::import` でレンダラーが受ける。
//!
//! 依存の規則では「共通実装」で、API クレート（`mm-*-api`）はこれに依存せず、GPU を触る実装だけが依存する。

mod context;
mod device_slot;
mod frame;
mod helper;
mod select;
mod timer;

pub use context::{adapter_id, default_backends, GpuContext, GpuRequest, FRAME_FORMAT};
pub use frame::{GpuFrameTexture, WaitFn};
pub use helper::{map_blocking, Readback, WgpuGpu};
pub use select::{
    choose, enumerate_adapters, find_adapter, find_adapter_id, select_adapter, AdapterCandidate,
    GpuSelector, GPU_ENV,
};
pub use timer::PassTimer;
