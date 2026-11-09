//! 帯の下端のキーフレームのマーカー。レイヤーの全部のパラメータのキーフレームの時刻を、帯の下端に並べる。
//! 置き場所と当たり判定はここが決め、描くのは `paint/keymarks.rs`（同じ値を見る）。

use crate::geom::Rect;
use crate::rows::RowHeight;
use crate::timeline::Item;
use crate::timeline_layout::Viewport;

/// ひし形の一辺（外接する正方形）。
pub(crate) const MARK_PX: f32 = 9.0;
/// これより近い（x の差がこれ未満の）キーフレームは 1 つのマーカーにまとめる。
pub(crate) const MERGE_PX: f32 = 6.0;
/// Small の行の縦線の幅と高さ。
const TICK_W: f32 = 2.0;
const TICK_H: f32 = 7.0;

/// マーカー 1 つ。近いキーフレームをまとめたときは、先頭のキーフレームの位置に置く。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Mark {
    /// `Item::keyframes` の添字（まとめた先頭）。
    pub(crate) key: usize,
    /// 中心の x（Timeline の座標）。
    pub(crate) x: f32,
}

/// `item` の、行の領域の横の範囲に見えるマーカー（左から順）。
/// まとめ方は先頭から数えるので、横に動かしてもマーカーの位置は変わらない。
pub(crate) fn marks(vp: &Viewport, item: &Item) -> Vec<Mark> {
    let area = vp.layout.row_area;
    let reach = MARK_PX / 2.0;
    let (left, right) = (area.x - reach, area.right() + reach);
    let mut out: Vec<Mark> = Vec::new();
    let mut anchor = f32::NEG_INFINITY;
    for (key, t) in item.keyframes.iter().enumerate() {
        let x = vp.x_of(*t);
        if x > right {
            break;
        }
        if x - anchor < MERGE_PX {
            continue;
        }
        anchor = x;
        if x >= left {
            out.push(Mark { key, x });
        }
    }
    out
}

/// 行 `i` のマーカー `x` の外接する正方形。帯の下端に揃える。当たり判定もこの矩形。
pub(crate) fn mark_rect(vp: &Viewport, i: usize, x: f32) -> Rect {
    let bar = vp.bar(i, 0.0, 0.0);
    Rect::new(x - MARK_PX / 2.0, bar.bottom() - MARK_PX, MARK_PX, MARK_PX)
}

/// 行 `i` のマーカーを縦線で描くか。Small の行は常に縦線、ほかの行は `rect` が行の領域に収まらないとき
/// （端で切るとひし形の形が変わるので、切っても形が変わらない縦線にする）。
pub(crate) fn is_tick(vp: &Viewport, i: usize, rect: Rect) -> bool {
    let area = vp.layout.row_area;
    vp.rows.height(i) == RowHeight::Small
        || rect.x < area.x
        || rect.y < area.y
        || rect.right() > area.right()
        || rect.bottom() > area.bottom()
}

/// 縦線の矩形（`rect` の中央に置く）。
pub(crate) fn tick_rect(rect: Rect) -> Rect {
    Rect::new(
        rect.x + (rect.w - TICK_W) / 2.0,
        rect.bottom() - TICK_H,
        TICK_W,
        TICK_H,
    )
}

/// 点 (x, y) の下にある、行 `i` のマーカー。
pub(crate) fn mark_at(vp: &Viewport, i: usize, item: &Item, (x, y): (f32, f32)) -> Option<Mark> {
    marks(vp, item)
        .into_iter()
        .find(|m| mark_rect(vp, i, m.x).contains(x, y))
}

#[cfg(test)]
mod tests;
