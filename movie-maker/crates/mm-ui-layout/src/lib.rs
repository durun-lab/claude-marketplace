//! wgpu のサーフェス上の UI の計算。座標は論理ピクセル（WebView の CSS の px と同じ大きさ）、時刻は秒（f64）。
//! サーフェスの物理ピクセルとの換算は [`Scale`] だけが行う。
//!
//! 純関数だけを持ち、GPU にも他の `mm-*` にも依存しない（`cargo test` が数秒で回る）。
//!
//! 表示の範囲は [`TimeView`]（時刻と x の変換、拡大縮小、移動）、目盛りは [`ticks`]、
//! スナップとフレームへの丸めは [`snap`] と [`quantize`]（何に吸い付いたかは [`SnapTarget`]）、帯と端のつまみの当たり判定は [`hit_test`]、
//! ポインター操作（スクラブ、選択、つまみのドラッグ、縦のスクロール）は [`TimelineController`] が受け持つ。
//! Timeline の中の矩形（ツールバー、目盛りの帯、見出しの列、行の領域、縦のスクロールバー、ズームバー、ステータスバー）は
//! [`TimelineLayout`]、レイヤーの行の高さと上端は [`Rows`]、それにズームとスクロールを合わせた見えている所は [`Viewport`]。
//! [`timeline_quads`] と [`timeline_overlay`] は Timeline を [`Quad`] の並びに、[`timeline_labels`] は目盛りの時刻と帯の名前を
//! [`Label`] の並びにし、描画は `mm-ui-wgpu` が行う。文字の幅は [`TextMeasure`] で `mm-ui-wgpu` に測らせ、
//! 矩形の中に文字を置くのは [`place`]（揃え方、余白、切り詰め）。ドラッグ中のツールチップは矩形の幅を文字から決めるので、
//! 矩形と文字を [`drag_tooltip`] が一緒に置き、文字の上に重ねる線は [`clear_tooltip_area`] でツールチップを避ける。
//! [`drawing_segments`] はコメントの描き込みを、Program の映像の矩形の上の線分に分ける。

mod controller;
mod drag_tip;
mod drawing;
mod geom;
mod gesture;
mod header;
mod keymarks;
mod labels;
mod paint;
pub mod palette;
mod rows;
mod ruler;
mod scale;
mod screen;
pub mod scroll;
mod snap;
mod text;
mod time_view;
mod timeline;
mod timeline_layout;
mod view_state;

pub use controller::{Action, Mods, PointerEvent, TimelineController};
pub use drag_tip::{clear_tooltip_area, drag_amount, drag_tooltip, DragTip};
pub use drawing::{
    drawing_segments, frame_point, DrawingSegment, DrawingShape, DrawingShapeKind, DRAWING_LINE_PX,
    DRAWING_POINT_PX,
};
pub use geom::{fit_rect, Color, Quad, QuadShape, Rect};
pub use gesture::{body_edit, edge_edit, EditKind, Side};
pub use keymarks::MARK_PX;
pub use labels::{timeline_labels, LABEL_PAD};
pub use paint::{timeline_overlay, timeline_quads, PaintState};
pub use rows::{RowHeight, RowHeights, Rows};
pub use ruler::{label, major_step, ticks, Tick};
pub use scale::{Scale, TextPlacement};
pub use screen::{split_viewers, SurfaceLayout, TIMELINE_RATIO, VIEW_MARGIN};
pub use snap::{quantize, snap, SnapKind, SnapTarget, Snapped, SNAP_PX};
pub use text::{
    fit, line_height, place, Align, Fit, Label, TextMeasure, TextStyle, UiFont, ELLIPSIS,
};
pub use time_view::{TimeView, MAX_PX_PER_SEC, MIN_FIT_PX_PER_SEC, MIN_PX_PER_SEC};
pub use timeline::{
    bar_span, cut_span, hit_test, AudioState, CutBar, Hit, Item, ItemKind, Part, TimelineModel,
    TrimBounds, HANDLE_PX,
};
pub use timeline_layout::{TimelineLayout, Viewport, HEADER_W, RULER_H, SCROLLBAR_W};
pub use view_state::ViewState;
