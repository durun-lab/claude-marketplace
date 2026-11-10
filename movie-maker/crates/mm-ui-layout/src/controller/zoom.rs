//! Timeline の表示の範囲を変える操作。キーとメニューの拡大縮小と全体表示（Shift+Z で元の表示と行き来する）、
//! ズームバーのドラッグ、再生中のページ送り、ホイール（拡大縮小、横の移動、縦のスクロール）。

use super::{Drag, TimelineController, SCROLL_STEP_PX};
use crate::screen::VIEW_MARGIN;
use crate::time_view::TimeView;
use crate::timeline::Hit;
use crate::zoombar::{self, ZoomBarDrag, ZoomBarPart};

/// ホイール 1 目盛りあたりの拡大率。
const ZOOM_STEP: f64 = 1.15;
/// ホイールで横に動かす量（px）。
const PAN_STEP_PX: f32 = 60.0;

/// キーとメニューの拡大縮小の 1 回の倍率。2 回で 2 倍になり、ホイールの 1 目盛り（1.15 倍）より大きく動くので、
/// 1 回押した分が見て分かる。
pub(super) const ZOOM_KEY_STEP: f64 = std::f64::consts::SQRT_2;

/// 全体表示にしたときの、全体表示とその前の表示。表示が全体表示のままなら、次の全体表示のコマンドで前の表示に戻す
/// （Resolve の Shift+Z）。ほかの操作で表示が変われば、次のコマンドはもう一度全体表示にする。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FullExtent {
    fit: TimeView,
    before: TimeView,
}

impl TimelineController {
    /// 再生位置 `playhead` を中心に `factor` 倍へ拡大する（縮小は長さ `duration` の全体が収まるまで）。
    /// 再生位置が見えていなければ、拡大した後に再生位置を行の領域の中央に置く（拡大した所が見えるように）。
    pub(super) fn zoom_at_playhead(&mut self, factor: f64, playhead: f64, duration: f64) {
        let width = self.layout.row_area.w;
        let floor = TimeView::zoom_floor(duration, width, VIEW_MARGIN);
        let x = self.view.x_of(playhead);
        if (0.0..=width).contains(&x) {
            self.view = self.view.zoom_at(x, factor, floor);
            return;
        }
        let zoomed = self.view.zoom_at(0.0, factor, floor);
        self.view = TimeView {
            origin: playhead - f64::from(width) / 2.0 / zoomed.px_per_sec,
            ..zoomed
        };
    }

    /// ツールバーのスライダーが指す倍率 `px_per_sec`（1 秒の論理ピクセル）にする。拡大の中心はキーと同じ再生位置。
    pub fn zoom_to(&mut self, px_per_sec: f64, playhead: f64, duration: f64) {
        if self.view.px_per_sec > 0.0 {
            self.zoom_at_playhead(px_per_sec / self.view.px_per_sec, playhead, duration);
        }
    }

    /// 全体表示にする。全体表示にした後に表示が変わっていなければ、全体表示の前の表示に戻す。
    pub(super) fn toggle_full_extent(&mut self, duration: f64) {
        if let Some(FullExtent { fit, before }) = self.full_extent.take() {
            if self.view == fit {
                self.view = before;
                return;
            }
        }
        let width = self.layout.row_area.w;
        if width <= 0.0 || duration <= 0.0 {
            return;
        }
        let fit = TimeView::fit(duration, width, VIEW_MARGIN);
        self.full_extent = Some(FullExtent {
            fit,
            before: self.view,
        });
        self.view = fit;
    }

    /// ズームバーを押した。スクロールボックスとその端なら掴み、溝なら押した側へ 1 画面ぶん送る。
    pub(super) fn press_zoombar(&mut self, part: ZoomBarPart, x: f32, duration: f64) {
        let width = self.layout.row_area.w;
        if part.on_thumb() {
            let track = self.layout.zoombar;
            let drag = ZoomBarDrag::new(part, x, track, (duration, self.view), width);
            self.drag = Drag::ZoomBar(drag);
        } else {
            let forward = part == ZoomBarPart::TrackAfter;
            self.view = zoombar::page(&self.view, width, duration, forward);
        }
    }

    /// 掴んだズームバーのスクロールボックスか端を `x` へ動かした。
    pub(super) fn drag_zoombar(&mut self, drag: &ZoomBarDrag, x: f32, duration: f64) {
        let width = self.layout.row_area.w;
        let floor = TimeView::zoom_floor(duration, width, VIEW_MARGIN);
        self.view = drag.view_at(x, width, floor);
        self.hover = Hit::ZoomBar { part: drag.part };
    }

    /// 再生位置が `from` から `to` へ動いて行の領域の端を越えたら、越えた先が見えるまでその向きへ 1 画面ずつ送る
    /// （再生中の Premiere の Page Scroll と、キーで動かした再生位置を見失わないこと）。端の外にあった再生位置が動いても
    /// 送らず、人が動かした表示を保つ。ドラッグの間は、ポインターの下の時刻が変わって編集の量が飛ぶので送らない。
    pub fn page_scroll(&mut self, from: f64, to: f64) {
        let width = self.layout.row_area.w;
        let (start, end) = self.view.visible(width);
        let page = end - start;
        if width <= 0.0 || page <= 0.0 || !matches!(self.drag, Drag::None) {
            return;
        }
        if from <= end && to > end {
            self.view.origin += ((to - start) / page).floor() * page;
        } else if from >= start && to < start {
            let back = self.view.origin - ((start - to) / page).ceil() * page;
            self.view.origin = back.max(0.0);
        }
    }

    /// ホイール。Ctrl でポインターの下（見出しの列の上なら行の領域の左端）を中心に拡大（縮小は長さ `duration` の全体が収まるまで）、
    /// Shift か、スクロールバーか見出しの列の上なら縦にスクロール、それ以外は横に動かす。
    pub(super) fn wheel(
        &mut self,
        (x, y): (f32, f32),
        dy: f32,
        (ctrl, shift): (bool, bool),
        duration: f64,
    ) {
        if ctrl {
            let anchor = (x - self.layout.row_area.x).max(0.0);
            let floor = TimeView::zoom_floor(duration, self.layout.row_area.w, VIEW_MARGIN);
            let factor = ZOOM_STEP.powf(f64::from(dy));
            self.view = self.view.zoom_at(anchor, factor, floor);
        } else if shift
            || self.layout.scrollbar.contains(x, y)
            || self.layout.headers.contains(x, y)
        {
            self.scroll_to(self.scroll_y - dy * SCROLL_STEP_PX);
        } else {
            self.view = self.view.pan_px(-dy * PAN_STEP_PX);
        }
    }
}

#[cfg(test)]
mod tests;
