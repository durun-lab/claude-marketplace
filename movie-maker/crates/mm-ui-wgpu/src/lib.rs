//! wgpu のサーフェス上の UI。Program のビューアー（エンジンが返した線形の画像を Rec.1886 で出す）と
//! Timeline（`mm-ui-layout` が落とした矩形を描く）を 1 枚のサーフェスに描き、Timeline のポインターの入力を受ける。
//! Program の映像の上にはコメントの描き込みを重ねる。重ねるのはサーフェスへの描画だけで、フレームのテクスチャには書かない。
//!
//! 計算は `mm-ui-layout` に置き、このクレートは wgpu の資源と描画だけを持つ。
//! Timeline の文字（目盛りの時刻、帯の名前）は cosmic-text でシェーピングし（`fonts.rs`）、glyphon で描く（`text.rs`）。

mod drawing;
mod empty;
mod fonts;
mod model;
mod names;
mod program;
mod quads;
mod text;
mod thumbs;
mod ui;

pub use drawing::DrawingRenderer;
pub use empty::empty_state;
pub use model::{timeline_cut_ids, timeline_layer_ids, timeline_model};
pub use program::ProgramViewer;
pub use quads::QuadRenderer;
pub use text::TextStats;
pub use thumbs::{slot_image_from_rgb, slot_image_len, SLOT_H, SLOT_W};
pub use ui::{Hint, Region, StatusText, ThumbStats, Ui};
