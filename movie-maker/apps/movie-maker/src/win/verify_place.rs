//! `--play-test` がクリックとドラッグを投げる位置。`Ui` と同じ `mm-ui-layout` の関数（`split_viewers`、
//! `TimelineLayout`、`Rows`、`Viewport`、`zoombar`）で、サーフェスの割り付けと Timeline の行とズームバーの位置を求める。
//! ズーム、スクロール、行の高さは、描画スレッドが置いた表示の状態（`ViewSlot::current`）を読む。

use mm_ui_layout::{
    split_viewers, zoombar, Rect, Rows, Scale, SurfaceLayout, TimelineLayout, TimelineModel,
    ViewState, Viewport,
};

use crate::win::state::Shared;
use crate::win::surface;

/// サーフェスの割り付けと Timeline の見えている所（論理ピクセル）と、表示の倍率。
pub(crate) struct Placement {
    pub surface: SurfaceLayout,
    pub scale: Scale,
    timeline: TimelineLayout,
    rows: Rows,
    state: ViewState,
    /// コンポジションの長さ（秒）。ズームバーが表す範囲に使う。
    duration: f64,
}

/// 今の割り付けと表示の状態。描画スレッドがまだ最初の表示を当てていなければ `None`。
/// Source の有無はフレームの到着から 1 周遅れて変わる `Ui` の側の値（`SourceSlot::is_shown`）を読む。
pub(crate) fn placement(shared: &Shared, hwnd: isize, model: &TimelineModel) -> Option<Placement> {
    let state = shared.view.current()?;
    let scale = Scale::new(surface::scale_factor(hwnd));
    let (w, h) = shared.surface_size();
    let surface = split_viewers(
        scale.to_logical(w as f32),
        scale.to_logical(h as f32),
        shared.source.is_shown(),
    );
    let rows = Rows::of(&model.items, &state.heights);
    let t = surface.timeline;
    let timeline = TimelineLayout::new((t.w, t.h), rows.content_height());
    Some(Placement {
        surface,
        scale,
        timeline,
        rows,
        state,
        duration: model.duration,
    })
}

impl Placement {
    /// Timeline の見えている所。
    pub(crate) fn viewport(&self) -> Viewport<'_> {
        Viewport {
            layout: &self.timeline,
            rows: &self.rows,
            time: self.state.time,
            scroll_y: self.state.scroll_y,
        }
    }

    /// 1 秒の論理ピクセル。
    pub(crate) fn px_per_sec(&self) -> f64 {
        self.state.time.px_per_sec
    }

    /// 行 `row` の帯の縦の中央で、時刻 `t` から論理ピクセルで `dx` ずらした点（クライアントの物理ピクセル）。
    pub(crate) fn bar_point(&self, row: usize, t: f64, dx: f32) -> (i32, i32) {
        let bar = self.viewport().bar(row, t, t);
        self.client((self.viewport().x_of(t) + dx, bar.y + bar.h / 2.0))
    }

    /// 行 `row` の時刻 `[start, end)` の帯の、行の領域に見えている所の中央（クライアントの物理ピクセル）。
    /// 見えている幅が `min_w` に足りなければ `None`。
    pub(crate) fn bar_center(
        &self,
        row: usize,
        (start, end): (f64, f64),
        min_w: f32,
    ) -> Option<(i32, i32)> {
        let bar = self.viewport().bar(row, start, end);
        let area = self.timeline.row_area;
        let (left, right) = (bar.x.max(area.x), bar.right().min(area.right()));
        (right - left >= min_w).then(|| self.client(((left + right) / 2.0, bar.y + bar.h / 2.0)))
    }

    /// 目盛りの帯の縦の中央の、時刻 `t` の点（クライアントの物理ピクセル）。
    pub(crate) fn ruler_point(&self, t: f64) -> (i32, i32) {
        let r = self.timeline.ruler;
        self.client((self.viewport().x_of(t), r.y + r.h / 2.0))
    }

    /// ズームバーのスクロールボックス（Timeline の座標の論理ピクセル）。
    pub(crate) fn zoom_thumb(&self) -> Rect {
        zoombar::thumb_of(&self.viewport(), self.duration)
    }

    /// ズームバーの溝の 1 論理ピクセルあたりの秒。
    pub(crate) fn zoombar_secs_per_px(&self) -> f64 {
        let width = self.timeline.row_area.w;
        let (lo, hi) = zoombar::extent(self.duration, &self.state.time, width);
        (hi - lo) / f64::from(self.timeline.zoombar.w.max(1.0))
    }

    /// ズームバーの縦の中央の、Timeline の座標の x の点（クライアントの物理ピクセル）。
    pub(crate) fn zoombar_point(&self, x: f32) -> (i32, i32) {
        let z = self.timeline.zoombar;
        self.client((x, z.y + z.h / 2.0))
    }

    /// 行の領域に全部が見えている行のうち、`want` に合う最初の行。
    pub(crate) fn first_visible(&self, want: impl Fn(usize) -> bool) -> Option<usize> {
        let vp = self.viewport();
        let area = self.timeline.row_area;
        vp.visible_rows().find(|i| {
            let row = vp.row(*i);
            row.y >= area.y && row.bottom() <= area.bottom() && want(*i)
        })
    }

    /// Timeline の座標の論理ピクセルの点を、サーフェスのクライアントの物理ピクセルにする。
    /// 切り捨てると最大 1 物理ピクセル左上へ寄るので、近い画素に丸める。
    fn client(&self, (x, y): (f32, f32)) -> (i32, i32) {
        let t = self.surface.timeline;
        let p = |v: f32| (v * self.scale.get()).round() as i32;
        (p(t.x + x), p(t.y + y))
    }

    /// 再生の操作のバーのボタン `command` の中央（クライアントの物理ピクセル）。バーを出していないか、
    /// そのバーにボタンが無ければ `None`。
    pub(crate) fn transport_button(
        &self,
        viewer: mm_ui_layout::Viewer,
        command: &str,
    ) -> Option<(i32, i32)> {
        let r = mm_ui_layout::transport_button(&self.surface, viewer, command)?;
        Some(self.surface_point((r.x + r.w / 2.0, r.y + r.h / 2.0)))
    }

    /// 再生の操作のバーのジョグバーの全体の線（サーフェスの論理ピクセル）。
    pub(crate) fn jog_track(&self, viewer: mm_ui_layout::Viewer) -> Option<mm_ui_layout::Rect> {
        mm_ui_layout::jog_track(&self.surface, viewer)
    }

    /// サーフェスの座標の論理ピクセルの点を、クライアントの物理ピクセルにする（近い画素に丸める）。
    pub(crate) fn surface_point(&self, (x, y): (f32, f32)) -> (i32, i32) {
        let p = |v: f32| (v * self.scale.get()).round() as i32;
        (p(x), p(y))
    }
}
