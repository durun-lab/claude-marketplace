//! `shader.hlsl` のノードの種類。利用者の HLSL を DXC で SPIR-V に、naga で WGSL にして、`shader.wgsl` と同じ形で描く。
//!
//! naga は HLSL を直接読めないので、SPIR-V を挟む（04-nodes.md）。HLSL の書き方と DXC の入れ方は README.md。
//! 変換の失敗は `Err(InvalidArgument)` で `file:行:列: 本文` を返し、レンダラーがそのレイヤーだけを透明にして記録する。
//! DXC が無い環境では `Err(Unsupported)` を返す。

mod convert;
mod diag;
mod dxc;
mod kind;
mod pass;
mod source;
mod spirv_lines;

pub use convert::{convert, ConvertError, Converted, Stage};
pub use diag::Diagnostic;
pub use dxc::{Dxc, DXC_ENV};
pub use kind::{ShaderHlslKind, ASSET_PARAM, CODE_LABEL, CODE_PARAM, KIND_VERSION};
pub use source::MAX_INPUTS;
