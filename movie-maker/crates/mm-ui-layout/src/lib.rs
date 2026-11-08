//! wgpu のサーフェス上の UI の計算。座標は論理ピクセル（WebView の CSS の px と同じ大きさ）、時刻は秒（f64）。
//! サーフェスの物理ピクセルとの換算は [`Scale`] だけが行う。
//!
//! 純関数だけを持ち、GPU にも他の `mm-*` にも依存しない（`cargo test` が数秒で回る）。
//!
//! 表示の範囲は [`TimeView`]（時刻と x の変換、拡大縮小、移動）、目盛りは [`ticks`]、
//! スナップとフレームへの丸めは [`snap`] と [`quantize`]、帯と端のつまみの当たり判定は [`hit_test`]、
//! ポインター操作（スクラブ、選択、つまみのドラッグ）は [`TimelineController`] が受け持つ。
//! [`timeline_quads`] は Timeline を [`Quad`] の並びに落とし、描画は `mm-ui-wgpu` が行う。
//! [`drawing_segments`] はコメントの描き込みを、Program の映像の矩形の上の線分に分ける。

mod controller;
mod digits;
mod drawing;
mod geom;
mod gesture;
mod paint;
pub mod palette;
mod ruler;
mod scale;
mod screen;
mod snap;
mod time_view;
mod timeline;

pub use controller::{Action, Mods, PointerEvent, TimelineController};
pub use digits::{text_quads, text_width};
pub use drawing::{
    drawing_segments, frame_point, DrawingSegment, DrawingShape, DrawingShapeKind, DRAWING_LINE_PX,
    DRAWING_POINT_PX,
};
pub use geom::{fit_rect, Color, Quad, Rect};
pub use gesture::{body_edit, edge_edit, EditKind, Side};
pub use paint::{timeline_quads, PaintState};
pub use ruler::{label, major_step, ticks, Tick};
pub use scale::Scale;
pub use screen::{split_surface, split_viewers, SurfaceLayout, VIEW_MARGIN};
pub use snap::{quantize, snap, Snapped, SNAP_PX};
pub use time_view::{TimeView, MAX_PX_PER_SEC, MIN_PX_PER_SEC};
pub use timeline::{
    bar_span, cut_span, hit_test, row_top, CutBar, Hit, Item, ItemKind, Part, TimelineModel,
    TrimBounds, HANDLE_PX, ROW_GAP, ROW_H, RULER_H,
};
