//! 縦のスクロール。スクロールの範囲、行を見せる位置、スクロールボックスの位置と長さ。
//! 長さはどれも論理ピクセルで、スクロールは行の並びの上端から、行の領域の上端に見えている所までの距離。

use crate::geom::Rect;
use crate::rows::Rows;

/// スクロールボックスの長さの下限。行が数千あっても掴めるようにする。
pub const MIN_THUMB_PX: f32 = 24.0;

/// 縦のスクロールの上限。中身が高さ `view_h` に入れば 0。
pub fn max_scroll(content_h: f32, view_h: f32) -> f32 {
    (content_h - view_h).max(0.0)
}

/// 範囲に収めたスクロール。有限でない値は 0 にする。
pub fn clamp_scroll(y: f32, content_h: f32, view_h: f32) -> f32 {
    if y.is_finite() {
        y.clamp(0.0, max_scroll(content_h, view_h))
    } else {
        0.0
    }
}

/// 行 `i` が全部見える、スクロール `scroll` から最も近いスクロール。高さ `view_h` より高い行は上端を合わせる。
pub fn reveal(rows: &Rows, i: usize, scroll: f32, view_h: f32) -> f32 {
    let (top, bottom) = (rows.top(i), rows.bottom(i));
    let y = if top < scroll || bottom - top > view_h {
        top
    } else if bottom > scroll + view_h {
        bottom - view_h
    } else {
        scroll
    };
    clamp_scroll(y, rows.content_height(), view_h)
}

/// スクロールバー `track` のスクロールボックス。長さは見えている割合で、下限は [`MIN_THUMB_PX`]。
pub fn thumb(track: Rect, content_h: f32, scroll: f32) -> Rect {
    let len = thumb_len(track, content_h);
    let travel = track.h - len;
    let max = max_scroll(content_h, track.h);
    let at = if max > 0.0 && travel > 0.0 {
        (scroll / max).clamp(0.0, 1.0) * travel
    } else {
        0.0
    };
    Rect::new(track.x, track.y + at, track.w, len)
}

/// スクロールボックスの上端を `top` に置いたときのスクロール。
pub fn scroll_for_thumb(track: Rect, content_h: f32, top: f32) -> f32 {
    let travel = track.h - thumb_len(track, content_h);
    if travel <= 0.0 {
        return 0.0;
    }
    ((top - track.y) / travel).clamp(0.0, 1.0) * max_scroll(content_h, track.h)
}

fn thumb_len(track: Rect, content_h: f32) -> f32 {
    if content_h <= track.h || content_h <= 0.0 {
        return track.h;
    }
    (track.h * track.h / content_h)
        .max(MIN_THUMB_PX)
        .min(track.h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::RowHeight;

    const TRACK: Rect = Rect::new(788.0, 26.0, 12.0, 200.0);

    #[test]
    fn スクロールは中身が見えている高さを超えた分までに収める() {
        assert_eq!(max_scroll(500.0, 200.0), 300.0);
        assert_eq!(max_scroll(100.0, 200.0), 0.0);
        assert_eq!(clamp_scroll(-5.0, 500.0, 200.0), 0.0);
        assert_eq!(clamp_scroll(400.0, 500.0, 200.0), 300.0);
        assert_eq!(clamp_scroll(f32::NAN, 500.0, 200.0), 0.0);
    }

    #[test]
    fn 見せる行が上にあれば上端に_下にあれば下端に合わせ_見えていれば動かさない() {
        let rows = Rows::new([RowHeight::Medium; 20]);
        assert_eq!(reveal(&rows, 2, 100.0, 140.0), 56.0, "上の行は上端へ");
        assert_eq!(
            reveal(&rows, 10, 0.0, 140.0),
            308.0 - 140.0,
            "下の行は下端へ"
        );
        assert_eq!(reveal(&rows, 4, 100.0, 140.0), 100.0, "見えている行");
        let tall = Rows::new([RowHeight::Medium, RowHeight::Large]);
        assert_eq!(reveal(&tall, 1, 0.0, 40.0), 28.0, "高さより高い行は上端");
    }

    #[test]
    fn スクロールボックスは見えている割合の長さで_スクロールの割合の位置に置く() {
        let top = thumb(TRACK, 800.0, 0.0);
        assert_eq!((top.y, top.h), (26.0, 50.0), "200 / 800 の長さ");
        let end = thumb(TRACK, 800.0, 600.0);
        assert_eq!(end.bottom(), TRACK.bottom());
        let whole = thumb(TRACK, 100.0, 0.0);
        assert_eq!(whole.h, TRACK.h, "中身が入るときは全部");
    }

    #[test]
    fn 数千の行でもスクロールボックスは下限の長さを保ち_ドラッグの位置がスクロールに戻る() {
        let content = 7500.0 * 28.0;
        let t = thumb(TRACK, content, 0.0);
        assert_eq!(t.h, MIN_THUMB_PX);
        let middle = TRACK.y + (TRACK.h - MIN_THUMB_PX) / 2.0;
        let s = scroll_for_thumb(TRACK, content, middle);
        assert!((s - (content - TRACK.h) / 2.0).abs() < 1.0, "{s}");
        assert!((thumb(TRACK, content, s).y - middle).abs() < 1e-3);
        assert_eq!(scroll_for_thumb(TRACK, content, -100.0), 0.0);
        assert_eq!(scroll_for_thumb(TRACK, content, 1e6), content - TRACK.h);
    }
}
