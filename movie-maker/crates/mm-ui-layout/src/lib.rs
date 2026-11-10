//! wgpu のサーフェス上の UI の計算。座標は論理ピクセル（WebView の CSS の px と同じ大きさ）、時刻は秒（f64）。
//! サーフェスの物理ピクセルとの換算は [`Scale`] だけが行う。
//!
//! 純関数だけを持ち、GPU にも他の `mm-*` にも依存しない（`cargo test` が数秒で回る）。
//!
//! 表示の範囲は [`TimeView`]（時刻と x の変換、拡大縮小、移動）、目盛りは [`ticks`]、
//! スナップとフレームへの丸めは [`snap`] と [`quantize`]（何に吸い付いたかは [`SnapTarget`]）、帯と端のつまみの当たり判定は [`hit_test`]、
//! ポインター操作（スクラブ、選択、つまみのドラッグ、縦のスクロール、ズームバー）とキーとメニューのコマンド
//! （[`TimelineCommand`]）は [`TimelineController`] が受け持つ。
//! Timeline の中の矩形（ツールバー、目盛りの帯、見出しの列、行の領域、縦のスクロールバー、ズームバー、ステータスバー）は
//! [`TimelineLayout`]、レイヤーの行の高さと上端は [`Rows`]、それにズームとスクロールを合わせた見えている所は [`Viewport`]。
//! レイヤーの行の下に開くキーフレームの行は [`KeyframeRow`]、その操作は [`KeyframeAction`] で返す。
//! [`timeline_quads`] と [`timeline_overlay`] は描く関数の入力 [`PaintInput`]（帯の波形のピーク [`Waves`] を含む）から
//! Timeline を [`Quad`] の並びに、[`timeline_labels`] は目盛りの時刻と帯の名前を
//! [`Label`] の並びにし、描画は `mm-ui-wgpu` が行う。`mm-ui-wgpu` は [`timeline_quads`] から帯までの矩形の数
//! （[`UnderText::bars`]）も受け、その後ろに帯のサムネイル（[`timeline_thumbs`]）を描く。
//! 文字の幅は [`TextMeasure`] で `mm-ui-wgpu` に測らせ、矩形の中に文字を置くのは [`place`]（揃え方、余白、切り詰め）。ドラッグ中のツールチップは矩形の幅を文字から決めるので、
//! 矩形と文字を [`drag_tooltip`] が一緒に置き、文字の上に重ねる線は [`clear_tooltip_area`] でツールチップを避ける。
//! 再生位置のタイムコード（[`timeline_timecode`]、[`transport_timecodes`]）は再生中に毎フレーム変わるので、ほかの文字と
//! 分けて返し、等幅の数字で置く（[`place_timecode`]）。ツールチップに重なれば [`clear_tooltip_labels`] で切る。
//! [`drawing_segments`] はコメントの描き込みを、Program の映像の矩形の上の線分に分ける。
//! [`EmptyHints`] はコンポジションに中身が無いときの空の状態の案内（Program と Timeline の行の領域の文字）を置く。

mod controller;
mod drag_tooltip;
mod drawing;
mod empty;
mod geom;
mod gesture;
mod header;
mod keyframe_rows;
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
mod thumbs;
mod time_view;
mod timeline;
mod timeline_layout;
mod tool;
mod toolbar;
mod transport;
mod view_state;
mod wave;
pub mod zoombar;

pub use controller::{
    edit_target, Action, Mods, PointerEvent, TimelineCommand, TimelineController,
};
pub use drag_tooltip::{
    clear_tooltip_area, clear_tooltip_labels, drag_amount, drag_tooltip, DragTooltip,
};
pub use drawing::{
    drawing_segments, frame_point, DrawingSegment, DrawingShape, DrawingShapeKind, DRAWING_LINE_PX,
    DRAWING_POINT_PX,
};
pub use empty::{EmptyHints, EmptyKeys, EmptyState};
pub use geom::{fit_rect, Color, Quad, QuadShape, Rect};
pub use gesture::{body_edit, edge_edit, EditKind, Side};
pub use header::{button_center, disclosure_center, name_center};
pub use keyframe_rows::{
    keyframe_center, toggle_open, KeyframeAction, KeyframeIndex, KeyframeRow, KeyframeRowState,
    RowKeyframe, RowPick, ROW_DIAMOND_PX,
};
pub use keymarks::MARK_PX;
pub use labels::{timeline_labels, timeline_timecode, LABEL_PAD};
pub use paint::{timeline_overlay, timeline_quads, PaintInput, PaintState, UnderText};
pub use rows::{RowHeight, RowHeights, Rows, KEYFRAME_ROW_H};
pub use ruler::{label, major_step, ticks, Tick};
pub use scale::{Scale, TextPlacement};
pub use screen::{
    content_width, split_viewers, splitter_at, splitter_rect, surface_layout, timeline_height,
    timeline_ratio, vertical_splitter_rect, Splitter, SurfaceLayout, MIN_PROGRAM_H, SPLITTER_W,
    TIMELINE_RATIO, VIEW_MARGIN,
};
pub use snap::{quantize, snap, SnapKind, SnapTarget, Snapped, SNAP_PX};
pub use text::{
    fit, line_height, place, place_timecode, Align, Fit, Label, TextMeasure, TextStyle, UiFont,
    ELLIPSIS,
};
pub use thumbs::{timeline_thumbs, ThumbQuad, THUMB_ASPECT};
pub use time_view::{TimeView, MAX_PX_PER_SEC, MIN_FIT_PX_PER_SEC, MIN_PX_PER_SEC};
pub use timeline::{
    bar_span, cut_span, hit_test, AudioState, CutBar, HeaderButton, Hit, Item, ItemKind, Part,
    TimelineModel, TrimBounds, HANDLE_PX,
};
pub use timeline_layout::{TimelineLayout, Viewport, HEADER_W, RULER_H, SCROLLBAR_W};
pub use tool::{BladeLine, Tool};
pub use toolbar::{
    toolbar_button, toolbar_labels, zoom_fraction, zoom_px, ToolbarAction, ToolbarController,
    ToolbarNames, ToolbarPart, ToolbarState, ToolbarView,
};
pub use transport::{
    jog_track, transport_button, transport_labels, transport_quads, transport_timecodes,
    TransportAction, TransportController, TransportPart, TransportState, Viewer, ViewerState,
    TRANSPORT_H,
};
pub use view_state::ViewState;
pub use wave::{Peaks, WaveSource, Waves};
