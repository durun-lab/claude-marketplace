//! 表示の状態（Timeline の）。ズームと横の位置、縦のスクロール、行の高さ、スナップの入/切。
//! プロジェクトのデータではないので操作にせず、アプリが `.mm/view.json` に置く（取り消しの列に入らない）。

use crate::rows::RowHeights;
use crate::time_view::TimeView;

/// 表示の状態（Timeline の）。
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    /// ズームと横の位置。
    pub time: TimeView,
    /// 縦のスクロール（論理ピクセル）。
    pub scroll_y: f32,
    /// 行の高さ。
    pub heights: RowHeights,
    /// スナップの入/切。
    pub snapping: bool,
}
