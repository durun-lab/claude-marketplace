//! 帯のマーカー。レイヤーの全部のパラメータのキーフレームの時刻を、帯の下端に並べる。
//! 置き場所と当たり判定はここが決め、描くのは `paint/keymarks.rs`（同じ値を見る）。

use crate::geom::Rect;
use crate::rows::RowHeight;
use crate::timeline::Item;
use crate::timeline_layout::Viewport;

/// 帯のマーカーのひし形の一辺（外接する正方形、論理ピクセル）。Medium と Large の帯は、下端のこの高さに
/// マーカーを並べ、名前をその上に置く。
pub const MARK_PX: f32 = 9.0;
/// Small の行のマーカーの矩形の高さ。名前の字の下の方にだけ掛かるよう、ひし形より低い縦線にする。
const SMALL_MARK_H: f32 = 6.0;
/// これより近い（x の差がこれ未満の）キーフレームは 1 つのマーカーにまとめる。
pub(crate) const MERGE_PX: f32 = 6.0;
/// 縦線の幅。高さはマーカーの矩形より 2 低くし、上に縁の 1 を空ける。
const LINE_W: f32 = 2.0;

/// 帯のマーカー 1 つ。近いキーフレームをまとめたときは、先頭のキーフレームの位置に置く。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BarMark {
    /// `Item::keyframes` の添字（まとめた先頭）。
    pub(crate) keyframe: usize,
    /// 中心の x（Timeline の座標）。
    pub(crate) x: f32,
}

/// `item` の、行の領域の横の範囲に見えるマーカー（左から順）。
/// まとめ方は先頭のキーフレームから数えるので、横に動かしてもマーカーの位置は変わらない。
pub(crate) fn marks(vp: &Viewport, item: &Item) -> Vec<BarMark> {
    marks_moved(vp, item, 0.0)
}

/// `item` のキーフレームを `by` 秒動かしたときに見えるマーカー（ドラッグ中の `shiftLayer`）。動かした後の位置で
/// 見えるものを選ぶので、行の領域の外から入ってくるマーカーも描ける。
pub(crate) fn marks_moved(vp: &Viewport, item: &Item, by: f64) -> Vec<BarMark> {
    let area = vp.layout.row_area;
    let half = MARK_PX / 2.0;
    let (left, right) = (area.x - half, area.right() + half);
    let mut out = Vec::new();
    let mut last_x = f32::NEG_INFINITY;
    for (keyframe, t) in item.keyframes.iter().enumerate() {
        let x = vp.x_of(*t + by);
        if x > right {
            break;
        }
        if x - last_x < MERGE_PX {
            continue;
        }
        last_x = x;
        if x >= left {
            out.push(BarMark { keyframe, x });
        }
    }
    out
}

/// 行 `i` のマーカー `x` の矩形。帯の下端に揃える。当たり判定もこの矩形。
pub(crate) fn mark_rect(vp: &Viewport, i: usize, x: f32) -> Rect {
    let bar = vp.bar(i, 0.0, 0.0);
    let h = if vp.rows.height(i) == RowHeight::Small {
        SMALL_MARK_H
    } else {
        MARK_PX
    };
    Rect::new(x - MARK_PX / 2.0, bar.bottom() - h, MARK_PX, h)
}

/// 行 `i` のマーカーを縦線で描くか。Small の行は常に縦線、ほかの行は `rect` が行の領域に収まらないとき
/// （端で切るとひし形の形が変わるので、切っても形が変わらない縦線にする）。
pub(crate) fn is_line(vp: &Viewport, i: usize, rect: Rect) -> bool {
    let area = vp.layout.row_area;
    vp.rows.height(i) == RowHeight::Small
        || rect.x < area.x
        || rect.y < area.y
        || rect.right() > area.right()
        || rect.bottom() > area.bottom()
}

/// 縦線の矩形。マーカーの矩形 `rect` の横の中央で、下端に揃える。
pub(crate) fn line_rect(rect: Rect) -> Rect {
    let h = rect.h - 2.0;
    Rect::new(
        rect.x + (rect.w - LINE_W) / 2.0,
        rect.bottom() - h,
        LINE_W,
        h,
    )
}

/// 点 (x, y) の下にある、行 `i` のマーカー。矩形が重なる所では、中心が近い方。
pub(crate) fn mark_at(vp: &Viewport, i: usize, item: &Item, (x, y): (f32, f32)) -> Option<BarMark> {
    marks(vp, item)
        .into_iter()
        .filter(|m| mark_rect(vp, i, m.x).contains(x, y))
        .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
}

#[cfg(test)]
mod tests;
