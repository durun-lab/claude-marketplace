//! ズームバー。Timeline の下端の横のバーで、ズームバーが表す範囲の上に、見えている範囲をスクロールボックスで示す。
//! 中のドラッグで見えている範囲を動かし、両端のドラッグで始まりか終わりを動かして拡大縮小する（Premiere の
//! ズームスクロールバー）。溝を押すと、押した側へ 1 画面ぶん送る（縦のスクロールバーと同じ）。
//! 座標は Timeline の矩形の左上を (0, 0) とする。

use crate::geom::Rect;
use crate::time_view::{TimeView, MAX_PX_PER_SEC};
use crate::timeline_layout::Viewport;

/// スクロールボックスの幅の下限。長いコンポジションを大きく拡大しても掴めるようにする。
pub const MIN_THUMB_W: f32 = 24.0;
/// スクロールボックスの端（掴むと拡大縮小する所）の幅の上限。帯の端のつまみと同じ幅。
pub const EDGE_PX: f32 = 7.0;

/// ズームバーの上の位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomBarPart {
    /// スクロールボックスの左端。ドラッグで見えている範囲の始まりを動かす。
    StartEdge,
    /// スクロールボックスの中。ドラッグで見えている範囲を動かす。
    Thumb,
    /// スクロールボックスの右端。ドラッグで見えている範囲の終わりを動かす。
    EndEdge,
    /// スクロールボックスより左の溝。
    TrackBefore,
    /// スクロールボックスより右の溝。
    TrackAfter,
}

impl ZoomBarPart {
    /// スクロールボックスの上か（両端を含む）。
    pub fn on_thumb(self) -> bool {
        matches!(self, Self::StartEdge | Self::Thumb | Self::EndEdge)
    }
}

/// ズームバーが表す時刻の範囲（始め、終わり）。コンポジションの全体に見えている範囲を合わせ、全体表示の左右の余白や
/// 終わりより右へ動かした表示でも、スクロールボックスが溝に収まるようにする。
pub fn extent(duration: f64, view: &TimeView, width: f32) -> (f64, f64) {
    let (start, end) = view.visible(width);
    (start.min(0.0), end.max(duration.max(0.0)))
}

/// 範囲 `extent` を溝 `track` に並べたときの、時刻 `t` の x。
pub fn x_of(track: Rect, (lo, hi): (f64, f64), t: f64) -> f32 {
    let span = hi - lo;
    if span <= 0.0 {
        return track.x;
    }
    track.x + ((t - lo) / span) as f32 * track.w
}

/// スクロールボックス。見えている範囲を溝 `track` に並べ、幅が [`MIN_THUMB_W`] に足りなければ中心を保って広げる
/// （溝の外には出さない）。
pub fn thumb(track: Rect, duration: f64, view: &TimeView, width: f32) -> Rect {
    let range = extent(duration, view, width);
    let (start, end) = view.visible(width);
    let (x0, x1) = (x_of(track, range, start), x_of(track, range, end));
    let w = (x1 - x0).max(MIN_THUMB_W.min(track.w));
    let x = ((x0 + x1 - w) / 2.0).min(track.right() - w).max(track.x);
    Rect::new(x, track.y, w, track.h)
}

/// 見えている所のズームバーのスクロールボックス。
pub fn thumb_of(vp: &Viewport, duration: f64) -> Rect {
    thumb(vp.layout.zoombar, duration, &vp.time, vp.layout.row_area.w)
}

/// スクロールボックス `thumb` を置いた溝の上の `x` がどこか。端の幅はスクロールボックスの幅の 1/3 を超えない。
pub fn part_at(thumb: Rect, x: f32) -> ZoomBarPart {
    let edge = EDGE_PX.min(thumb.w / 3.0);
    if x < thumb.x {
        ZoomBarPart::TrackBefore
    } else if x >= thumb.right() {
        ZoomBarPart::TrackAfter
    } else if x < thumb.x + edge {
        ZoomBarPart::StartEdge
    } else if x >= thumb.right() - edge {
        ZoomBarPart::EndEdge
    } else {
        ZoomBarPart::Thumb
    }
}

/// 溝を押したときの表示。押した側（`forward` なら右）へ 1 画面ぶん送り、ズームバーが表す範囲に収める。
pub fn page(view: &TimeView, width: f32, duration: f64, forward: bool) -> TimeView {
    let (lo, hi) = extent(duration, view, width);
    let (start, end) = view.visible(width);
    let span = end - start;
    let to = if forward { start + span } else { start - span };
    TimeView {
        origin: to.min(hi - span).max(lo),
        ..*view
    }
}

/// スクロールボックスとその端のドラッグ。溝の 1 px あたりの秒は押したときの範囲で決め、ドラッグ中に範囲が変わっても
/// ポインターと表示の動きの比を保つ。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomBarDrag {
    pub part: ZoomBarPart,
    /// 押したときのポインターの x。
    pub from_x: f32,
    /// 押したときの表示。
    pub from: TimeView,
    /// 押したときにズームバーが表した範囲。見えている範囲はこの外に出さない。
    pub extent: (f64, f64),
    /// 溝の 1 px あたりの秒。
    pub secs_per_px: f64,
}

impl ZoomBarDrag {
    /// 溝 `track` の上の `part` を `x` で押したときのドラッグ。`width` は行の領域の幅。
    pub fn new(
        part: ZoomBarPart,
        x: f32,
        track: Rect,
        (duration, view): (f64, TimeView),
        width: f32,
    ) -> Self {
        let extent = extent(duration, &view, width);
        let secs_per_px = if track.w > 0.0 {
            (extent.1 - extent.0) / f64::from(track.w)
        } else {
            0.0
        };
        Self {
            part,
            from_x: x,
            from: view,
            extent,
            secs_per_px,
        }
    }

    /// ポインターが `x` に来たときの表示。中は倍率を保って動かし、端は反対の端を動かさずに倍率を変える。
    /// 倍率は縮小の下限 `floor`（[`TimeView::zoom_floor`]。押したときの倍率が下限より細かければその倍率）から
    /// [`MAX_PX_PER_SEC`] に収める。
    pub fn view_at(&self, x: f32, width: f32, floor: f64) -> TimeView {
        let w = f64::from(width.max(1.0));
        let dt = f64::from(x - self.from_x) * self.secs_per_px;
        let (start, end) = self.from.visible(width);
        let (lo, hi) = self.extent;
        let shortest = w / MAX_PX_PER_SEC;
        let lowest = floor.min(self.from.px_per_sec);
        let px = |span: f64| (w / span).clamp(lowest, MAX_PX_PER_SEC);
        match self.part {
            ZoomBarPart::StartEdge => {
                let px = px(end - (start + dt).max(lo).min(end - shortest));
                TimeView {
                    origin: end - w / px,
                    px_per_sec: px,
                }
            }
            ZoomBarPart::EndEdge => TimeView {
                origin: start,
                px_per_sec: px((end + dt).min(hi).max(start + shortest) - start),
            },
            _ => TimeView {
                origin: (start + dt).min(hi - (end - start)).max(lo),
                ..self.from
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 幅 800 の溝（行の領域と同じ幅）。
    const TRACK: Rect = Rect::new(0.0, 286.0, 800.0, 14.0);
    const WIDTH: f32 = 800.0;

    /// 1 秒 80 px で、10 秒から 20 秒が見えている表示。
    fn zoomed() -> TimeView {
        TimeView {
            origin: 10.0,
            px_per_sec: 80.0,
        }
    }

    #[test]
    fn スクロールボックスはズームバーが表す範囲の中の見えている範囲に置く() {
        let t = thumb(TRACK, 100.0, &zoomed(), WIDTH);
        assert_eq!((t.x, t.w), (80.0, 80.0), "100 秒の 10 から 20 秒");
        assert_eq!((t.y, t.h), (TRACK.y, TRACK.h));
        let fit = TimeView::fit(100.0, WIDTH, 16.0);
        let whole = thumb(TRACK, 100.0, &fit, WIDTH);
        assert_eq!((whole.x, whole.w), (0.0, 800.0), "全体表示は溝の全部");
    }

    #[test]
    fn 終わりより右まで見えていても範囲を広げてスクロールボックスを溝に収める() {
        let past = TimeView {
            origin: 95.0,
            px_per_sec: 80.0,
        };
        assert_eq!(extent(100.0, &past, WIDTH), (0.0, 105.0));
        let t = thumb(TRACK, 100.0, &past, WIDTH);
        assert!((t.right() - 800.0).abs() < 1e-3, "{t:?}");
    }

    #[test]
    fn 細いスクロールボックスは中心を保って下限の幅に広げ_溝の端では内側に寄せる() {
        let narrow = TimeView {
            origin: 500.0,
            px_per_sec: 800.0,
        };
        let t = thumb(TRACK, 1000.0, &narrow, WIDTH);
        assert_eq!(t.w, MIN_THUMB_W);
        assert!(
            (t.x + t.w / 2.0 - 400.4).abs() < 1e-3,
            "500 から 501 秒の中央"
        );
        let at_end = TimeView {
            origin: 999.0,
            ..narrow
        };
        assert_eq!(thumb(TRACK, 1000.0, &at_end, WIDTH).right(), 800.0);
    }

    #[test]
    fn 端はスクロールボックスの両端で_幅の三分の一を超えない() {
        let t = Rect::new(100.0, 0.0, 90.0, 14.0);
        assert_eq!(part_at(t, 99.0), ZoomBarPart::TrackBefore);
        assert_eq!(part_at(t, 100.0), ZoomBarPart::StartEdge);
        assert_eq!(part_at(t, 106.0), ZoomBarPart::StartEdge);
        assert_eq!(part_at(t, 150.0), ZoomBarPart::Thumb);
        assert_eq!(part_at(t, 185.0), ZoomBarPart::EndEdge);
        assert_eq!(part_at(t, 190.0), ZoomBarPart::TrackAfter);
        let small = Rect::new(0.0, 0.0, 12.0, 14.0);
        assert_eq!(part_at(small, 4.5), ZoomBarPart::Thumb, "端は 4 px まで");
        assert!(ZoomBarPart::EndEdge.on_thumb() && !ZoomBarPart::TrackAfter.on_thumb());
    }

    #[test]
    fn 溝を押すと押した側へ_1_画面ぶん送り_範囲の端で止まる() {
        let v = zoomed();
        assert_eq!(page(&v, WIDTH, 100.0, true).origin, 20.0);
        assert_eq!(page(&v, WIDTH, 100.0, false).origin, 0.0);
        let near_end = TimeView { origin: 85.0, ..v };
        assert_eq!(page(&near_end, WIDTH, 100.0, true).origin, 90.0);
        assert_eq!(page(&v, WIDTH, 100.0, true).px_per_sec, 80.0);
    }

    fn drag(part: ZoomBarPart, x: f32) -> ZoomBarDrag {
        ZoomBarDrag::new(part, x, TRACK, (100.0, zoomed()), WIDTH)
    }

    #[test]
    fn 中のドラッグは倍率を保って動かし_範囲の外へは出さない() {
        let d = drag(ZoomBarPart::Thumb, 120.0);
        assert_eq!(d.secs_per_px, 0.125, "100 秒を 800 px に");
        let moved = d.view_at(200.0, WIDTH, 2.0);
        assert_eq!(
            (moved.origin, moved.px_per_sec),
            (20.0, 80.0),
            "80 px は 10 秒"
        );
        assert_eq!(
            d.view_at(10_000.0, WIDTH, 2.0).origin,
            90.0,
            "終わりで止まる"
        );
        assert_eq!(d.view_at(-10_000.0, WIDTH, 2.0).origin, 0.0, "先頭で止まる");
    }

    #[test]
    fn 右端のドラッグは始まりを動かさずに倍率を変える() {
        let d = drag(ZoomBarPart::EndEdge, 160.0);
        let wider = d.view_at(240.0, WIDTH, 2.0);
        assert_eq!(wider.origin, 10.0);
        assert!(
            (wider.px_per_sec - 40.0).abs() < 1e-9,
            "10 秒ぶん縮めて 20 秒を見せる"
        );
        let narrower = d.view_at(120.0, WIDTH, 2.0);
        assert!((narrower.px_per_sec - 160.0).abs() < 1e-9, "5 秒を見せる");
        let crossed = d.view_at(-1000.0, WIDTH, 2.0);
        assert!(
            (crossed.px_per_sec - MAX_PX_PER_SEC).abs() < 1e-6,
            "始まりを越えず、拡大の上限で止まる"
        );
        let all = d.view_at(10_000.0, WIDTH, 2.0);
        assert!(
            (all.px_per_sec - 800.0 / 90.0).abs() < 1e-9,
            "範囲の終わりまで"
        );
    }

    #[test]
    fn 左端のドラッグは終わりを動かさずに倍率を変え_下限より縮めない() {
        let d = drag(ZoomBarPart::StartEdge, 80.0);
        let wider = d.view_at(0.0, WIDTH, 2.0);
        assert!((wider.px_per_sec - 40.0).abs() < 1e-9, "0 秒から 20 秒");
        assert!(wider.origin.abs() < 1e-9);
        let (_, end) = wider.visible(WIDTH);
        assert!((end - 20.0).abs() < 1e-9, "終わりは動かない");
        let floored = d.view_at(0.0, WIDTH, 60.0);
        assert_eq!(floored.px_per_sec, 60.0, "下限で止まる");
        assert!((floored.visible(WIDTH).1 - 20.0).abs() < 1e-9);
    }
}
