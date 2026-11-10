//! Timeline の割り付け。Timeline の矩形を、ツールバー、目盛りの帯、見出しの列、行の領域、縦のスクロールバー、
//! ズームバー、ステータスバーの矩形に分ける。座標は Timeline の矩形の左上を (0, 0) とする。
//!
//! 領域を足す PR は、ここに自分の大きさの定数を置き、描く関数を `paint/` の下の自分のファイルに置く。
//! ステータスバーは下端の 1 行（下限の高さに足りなければ 0）。

use std::ops::Range;

use crate::geom::Rect;
use crate::rows::{RowHeight, Rows};
use crate::scroll;
use crate::time_view::TimeView;
use crate::timeline::span;

/// 目盛りの帯の高さ。
pub const RULER_H: f32 = 26.0;

/// 縦のスクロールバーの幅。中身が行の領域に入らないときだけ出す。
pub const SCROLLBAR_W: f32 = 12.0;

/// ツールバーの高さ。
pub(crate) const TOOLBAR_H: f32 = 30.0;

/// 見出しの列の幅（論理ピクセル）。
pub const HEADER_W: f32 = 180.0;

/// ズームバーの高さ。スクロールボックスの両端を狙って掴めるよう、縦のスクロールバーの幅より少し太くする。
const ZOOMBAR_H: f32 = 14.0;

/// ステータスバーの高さ。字の大きさ 12 の 1 行（15）に上下 3.5 の余白を足し、ツールバーのボタン（24）と再生の操作のバー（26）の間にそろえる。
pub(crate) const STATUS_H: f32 = 22.0;

/// Timeline の高さの下限。どの領域も潰さずに、Medium の行が 2 本見える高さ（領域の大きさを変える PR は、ここを変えなくてよい）。
pub const MIN_TIMELINE_H: f32 =
    TOOLBAR_H + RULER_H + 2.0 * RowHeight::Medium.px() + ZOOMBAR_H + STATUS_H;

/// Timeline の割り付け。どの矩形も大きさは 0 を下回らない。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TimelineLayout {
    /// Timeline の全体。
    pub bounds: Rect,
    /// 上端のツールバー。
    pub toolbar: Rect,
    /// 見出しの列の上、目盛りの帯の左の角。
    pub corner: Rect,
    /// 目盛りの帯。横は行の領域と同じ範囲で、縦にはスクロールしない。
    pub ruler: Rect,
    /// 見出しの列。行の領域と一緒に縦にスクロールする。
    pub headers: Rect,
    /// 行の領域。レイヤーの行を並べて縦にスクロールする。時刻の x はこの左端から測る。
    pub row_area: Rect,
    /// 縦のスクロールバー。中身が行の領域に入るときは幅 0。
    pub scrollbar: Rect,
    /// ズームバー。横は行の領域と同じ範囲。
    pub zoombar: Rect,
    /// ステータスバー。
    pub status: Rect,
}

impl TimelineLayout {
    /// 大きさ `(w, h)` の Timeline を、行の並びの高さ `content_h` に合わせて割る。
    pub fn new((w, h): (f32, f32), content_h: f32) -> Self {
        let (w, h) = (w.max(0.0), h.max(0.0));
        // ツールバーと同じく、下限の高さに足りなければ出さずに行へ回す（ポインターの下の操作はツールチップにもあるため）。
        let status_h = if h >= MIN_TIMELINE_H { STATUS_H } else { 0.0 };
        // 高さが足りないときは、再生位置を動かす目盛りの帯をズームバーより先に取る。
        let ruler_h = RULER_H.min(h - status_h);
        let zoombar_h = ZOOMBAR_H.min(h - status_h - ruler_h);
        // ツールバーは潰さずに出すか出さないかで、下限の高さに足りなければ出さずに行へ回す（再生の操作のバーと同じく、
        // ボタンのコマンドはキーとメニューにもあるため）。
        let toolbar_h = if h >= MIN_TIMELINE_H { TOOLBAR_H } else { 0.0 };
        let area_y = toolbar_h + ruler_h;
        let area_h = (h - area_y - zoombar_h - status_h).max(0.0);
        let scrollbar_w = if content_h > area_h {
            SCROLLBAR_W.min(w)
        } else {
            0.0
        };
        let header_w = HEADER_W.min(w - scrollbar_w);
        let area_w = (w - header_w - scrollbar_w).max(0.0);
        let row_area = Rect::new(header_w, area_y, area_w, area_h);
        Self {
            bounds: Rect::new(0.0, 0.0, w, h),
            toolbar: Rect::new(0.0, 0.0, w, toolbar_h),
            corner: Rect::new(0.0, toolbar_h, header_w, ruler_h),
            ruler: Rect::new(header_w, toolbar_h, area_w, ruler_h),
            headers: Rect::new(0.0, area_y, header_w, area_h),
            row_area,
            scrollbar: Rect::new(row_area.right(), area_y, scrollbar_w, area_h),
            zoombar: Rect::new(header_w, row_area.bottom(), area_w, zoombar_h),
            status: Rect::new(0.0, h - status_h, w, status_h),
        }
    }

    /// 時刻の縦の線（再生位置、スナップのガイド線）を引く範囲。目盛りの帯と行の領域を合わせた列。
    pub fn time_column(&self) -> Rect {
        let top = self.ruler.y;
        Rect::new(
            self.row_area.x,
            top,
            self.row_area.w,
            self.row_area.bottom() - top,
        )
    }
}

/// Timeline の見えている所。割り付け、行の並び、ズームと横の位置、縦のスクロールの組。
/// 当たり判定、矩形と文字の置き場所、自動試験のクリックの位置が同じ値から計算する。
#[derive(Debug, Clone, Copy)]
pub struct Viewport<'a> {
    pub layout: &'a TimelineLayout,
    pub rows: &'a Rows,
    /// ズームと横の位置。x は行の領域の左端を 0 とする。
    pub time: TimeView,
    /// 縦のスクロール（論理ピクセル）。行の領域の上端に見えている、行の並びの上端からの距離。
    pub scroll_y: f32,
}

impl Viewport<'_> {
    /// 時刻 `t` の x（Timeline の座標）。
    pub fn x_of(&self, t: f64) -> f32 {
        self.layout.row_area.x + self.time.x_of(t)
    }

    /// x（Timeline の座標）の時刻。
    pub fn t_of(&self, x: f32) -> f64 {
        self.time.t_of(x - self.layout.row_area.x)
    }

    /// 行 `i` の矩形。横は行の領域の幅。
    pub fn row(&self, i: usize) -> Rect {
        let r = self.layout.row_area;
        let top = r.y + self.rows.top(i) - self.scroll_y;
        Rect::new(r.x, top, r.w, self.rows.bottom(i) - self.rows.top(i))
    }

    /// 行 `i` の、時刻 `[start, end)` の帯の矩形。短い帯も掴めるように最小の幅を持たせる。
    pub fn bar(&self, i: usize, start: f64, end: f64) -> Rect {
        let (x, w) = span(&self.time, start, end);
        let (y, h) = self.rows.bar(i);
        let r = self.layout.row_area;
        Rect::new(r.x + x, r.y + y - self.scroll_y, w, h)
    }

    /// 行の領域に一部でも見えている行。
    pub fn visible_rows(&self) -> Range<usize> {
        self.rows
            .overlapping(self.scroll_y, self.scroll_y + self.layout.row_area.h)
    }

    /// 縦のスクロールバーのスクロールボックス。
    pub fn thumb(&self) -> Rect {
        scroll::thumb(
            self.layout.scrollbar,
            self.rows.content_height(),
            self.scroll_y,
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::rows::{RowHeight, RowHeights};
    use crate::timeline::TimelineModel;

    /// 試験の見えている所の一式。行の高さは既定、1 秒 100 px、左端が 0 秒。
    pub(crate) struct TestViewport {
        pub layout: TimelineLayout,
        pub rows: Rows,
        pub time: TimeView,
        pub scroll_y: f32,
    }

    impl TestViewport {
        /// 大きさ `size`（高さはツールバーを除く）の Timeline に `model` の行を並べる。
        pub(crate) fn new(model: &TimelineModel, size: (f32, f32)) -> Self {
            let rows = Rows::of(&model.items, &RowHeights::default());
            Self {
                layout: TimelineLayout::new(
                    (size.0, size.1 + TOOLBAR_H + STATUS_H),
                    rows.content_height(),
                ),
                rows,
                time: TimeView {
                    origin: 0.0,
                    px_per_sec: 100.0,
                },
                scroll_y: 0.0,
            }
        }

        pub(crate) fn vp(&self) -> Viewport<'_> {
            Viewport {
                layout: &self.layout,
                rows: &self.rows,
                time: self.time,
                scroll_y: self.scroll_y,
            }
        }

        /// 行 `i` の帯の上端から 5 の y。
        pub(crate) fn in_bar(&self, i: usize) -> f32 {
            self.vp().bar(i, 0.0, 0.0).y + 5.0
        }
    }

    #[test]
    fn 中身が入れば縦のスクロールバーは幅_0_で_行の領域が右端まで() {
        let l = TimelineLayout::new((800.0, 300.0), 200.0);
        let w = 800.0 - HEADER_W;
        assert_eq!(l.ruler, Rect::new(HEADER_W, TOOLBAR_H, w, RULER_H));
        let rows_h = 300.0 - TOOLBAR_H - RULER_H - ZOOMBAR_H - STATUS_H;
        assert_eq!(
            l.row_area,
            Rect::new(HEADER_W, TOOLBAR_H + RULER_H, w, rows_h)
        );
        assert_eq!(l.scrollbar.w, 0.0);
        assert_eq!(l.bounds, Rect::new(0.0, 0.0, 800.0, 300.0));
    }

    #[test]
    fn ズームバーは行の領域の下端に_行の領域と同じ横の範囲で置く() {
        for content in [200.0, 1000.0] {
            let l = TimelineLayout::new((800.0, 300.0), content);
            let z = l.zoombar;
            assert_eq!((z.x, z.w), (l.row_area.x, l.row_area.w), "{content}");
            assert_eq!((z.y, z.h), (l.row_area.bottom(), ZOOMBAR_H));
            assert_eq!(z.bottom(), 300.0 - STATUS_H, "ステータスバーの上");
        }
    }

    #[test]
    fn 中身が行の領域より高ければ右に縦のスクロールバーを出し_目盛りの帯も同じ幅に縮める() {
        let l = TimelineLayout::new((800.0, 300.0), 1000.0);
        assert_eq!(
            l.scrollbar,
            Rect::new(
                800.0 - SCROLLBAR_W,
                TOOLBAR_H + RULER_H,
                SCROLLBAR_W,
                l.row_area.h
            )
        );
        assert_eq!(l.row_area.right(), l.scrollbar.x);
        assert_eq!((l.ruler.x, l.ruler.w), (l.row_area.x, l.row_area.w));
        assert_eq!(
            l.time_column(),
            Rect::new(
                HEADER_W,
                TOOLBAR_H,
                l.row_area.w,
                300.0 - TOOLBAR_H - ZOOMBAR_H - STATUS_H
            )
        );
    }

    #[test]
    fn ステータスバーは下端の_1_行で_下限に足りなければ出さない_矩形は負にならない() {
        let l = TimelineLayout::new((800.0, 300.0), 1000.0);
        assert_eq!(l.status, Rect::new(0.0, 300.0 - STATUS_H, 800.0, STATUS_H));
        let low = TimelineLayout::new((800.0, MIN_TIMELINE_H - 1.0), 1000.0);
        assert_eq!(low.status.h, 0.0);
        let tiny = TimelineLayout::new((5.0, 10.0), 1000.0);
        for r in [
            tiny.ruler,
            tiny.row_area,
            tiny.scrollbar,
            tiny.headers,
            tiny.zoombar,
        ] {
            assert!(r.w >= 0.0 && r.h >= 0.0, "{r:?}");
        }
        assert_eq!(tiny.ruler.h, 10.0, "目盛りの帯も Timeline の高さを超えない");
        assert_eq!(tiny.zoombar.h, 0.0, "足りない高さは目盛りの帯が先に取る");
    }

    #[test]
    fn ツールバーは下限の高さがあれば上端に出し_足りなければ潰さずに出さないで行に回す() {
        let full = TimelineLayout::new((800.0, MIN_TIMELINE_H), 1000.0);
        assert_eq!(full.toolbar, Rect::new(0.0, 0.0, 800.0, TOOLBAR_H));
        assert_eq!(full.ruler.y, TOOLBAR_H);
        assert_eq!(full.row_area.h, 2.0 * RowHeight::Medium.px(), "行は 2 本");
        let low = TimelineLayout::new((800.0, MIN_TIMELINE_H - 1.0), 1000.0);
        assert_eq!(low.toolbar.h, 0.0);
        assert_eq!(low.ruler.y, 0.0);
        assert_eq!(
            low.row_area.h,
            MIN_TIMELINE_H - 1.0 - RULER_H - ZOOMBAR_H,
            "ツールバーとステータスバーの分も行に回す"
        );
    }

    #[test]
    fn 見えている所は行の領域の左上とスクロールから帯と行を置く() {
        let rows = Rows::new([RowHeight::Medium; 10]);
        let layout =
            TimelineLayout::new((800.0, 100.0 + TOOLBAR_H + STATUS_H), rows.content_height());
        let vp = Viewport {
            layout: &layout,
            rows: &rows,
            time: TimeView {
                origin: 0.0,
                px_per_sec: 100.0,
            },
            scroll_y: 40.0,
        };
        assert_eq!(vp.row(2).y, TOOLBAR_H + RULER_H + 56.0 - 40.0);
        assert_eq!(
            vp.bar(2, 1.0, 2.0),
            Rect::new(
                HEADER_W + 100.0,
                TOOLBAR_H + RULER_H + 58.0 - 40.0,
                100.0,
                25.0
            )
        );
        assert_eq!(
            vp.visible_rows(),
            1..4,
            "行の領域の 60 px に 40 から 100 まで"
        );
        assert_eq!(vp.x_of(1.5), HEADER_W + 150.0);
        assert!((vp.t_of(HEADER_W + 150.0) - 1.5).abs() < 1e-9);
    }
}
