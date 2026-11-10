//! Timeline のレイヤーの行。行の高さ（Small、Medium、Large）と、行ごとの上端の並び。
//! レイヤーの行の下には、開いたキーフレームの行が続く（行の上端の並びはその分を含む）。
//! 縦の座標は行の領域の上端を 0 とする、スクロールの前の値。

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use crate::timeline::{Item, ItemKind};

/// 行の上端から帯の上端まで。上の 1 px は行どうしの境の線、次の 1 px は縞の色で帯を囲む。
const BAR_TOP: f32 = 2.0;
/// 帯の下端から行の下端まで（縞の色）。
const BAR_BOTTOM: f32 = 1.0;
/// キーフレームの行 1 本の高さ。上の 1 px は行の境界の線。
pub const KEYFRAME_ROW_H: f32 = 20.0;

/// 行の高さ（Timeline の）。高さに応じて帯に出すものを変える（Resolve の帯の見せ方の 3 つに倣う）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum RowHeight {
    /// 名前だけ。
    Small,
    /// 先頭のサムネイル、名前、音があれば下端に細い波形。
    #[default]
    Medium,
    /// フィルムストリップ、名前、下半分に波形。
    Large,
}

impl RowHeight {
    /// 行の高さ（論理ピクセル）。帯の高さは上下の間隔を引いた値。
    pub const fn px(self) -> f32 {
        match self {
            Self::Small => 18.0,
            Self::Medium => 28.0,
            Self::Large => 56.0,
        }
    }

    /// 既定の行の高さ。sequence の行は本編なので Large、ほかは Medium。
    pub fn default_for(kind: ItemKind) -> Self {
        if kind == ItemKind::Sequence {
            Self::Large
        } else {
            Self::Medium
        }
    }
}

/// 行の高さの決め方（表示の状態）。レイヤーごとの高さ、全部の行の高さ、既定の順に引く。
/// キーフレームの行を開いたレイヤーも、レイヤーの行の下に続く高さとしてここに持つ。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RowHeights {
    /// 全部の行の高さ（View > Track Height）。`None` は既定（[`RowHeight::default_for`]）。
    pub all: Option<RowHeight>,
    /// レイヤーごとの高さ（レイヤーの見出しの右クリック）。キーはレイヤーの ID。
    pub layers: BTreeMap<String, RowHeight>,
    /// キーフレームの行を開いたレイヤーの ID（見出しのキーフレームの開閉と Show Keyframe Editor）。
    pub keyframe_rows_open: BTreeSet<String>,
}

impl RowHeights {
    /// レイヤー `item` の行の高さ。
    pub fn of(&self, item: &Item) -> RowHeight {
        self.layers
            .get(&*item.id)
            .copied()
            .or(self.all)
            .unwrap_or_else(|| RowHeight::default_for(item.kind))
    }

    /// レイヤー `item` の行の下に開くキーフレームの行の数。閉じていれば 0。
    pub fn keyframe_rows_of(&self, item: &Item) -> usize {
        if self.keyframe_rows_open.contains(&*item.id) {
            item.keyframe_rows.len()
        } else {
            0
        }
    }
}

/// 行の並び。行ごとの高さと上端を持ち、縦の位置から行を二分探索で引く（数千のレイヤーでも見える行だけを扱う）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rows {
    heights: Vec<RowHeight>,
    /// 行 `i` の下に開いたキーフレームの行の数。
    keyframe_rows: Vec<usize>,
    /// 行 `i` の上端。最後に全部の行の高さの和を持つので、長さは行の数 + 1。空の並びは空。
    tops: Vec<f32>,
}

impl Rows {
    /// 上の行から順の高さで並べる（キーフレームの行は開かない）。
    pub fn new(heights: impl IntoIterator<Item = RowHeight>) -> Self {
        Self::with_keyframe_rows(heights.into_iter().map(|h| (h, 0)))
    }

    /// 上の行から順に、レイヤーの行の高さとその下に開くキーフレームの行の数で並べる。
    pub fn with_keyframe_rows(rows: impl IntoIterator<Item = (RowHeight, usize)>) -> Self {
        let (heights, keyframe_rows): (Vec<RowHeight>, Vec<usize>) = rows.into_iter().unzip();
        let mut tops = Vec::with_capacity(heights.len() + 1);
        let mut y = 0.0;
        tops.push(y);
        for (h, k) in heights.iter().zip(&keyframe_rows) {
            y += h.px() + *k as f32 * KEYFRAME_ROW_H;
            tops.push(y);
        }
        Self {
            heights,
            keyframe_rows,
            tops,
        }
    }

    /// Timeline のレイヤー `items` を、表示の状態 `heights` の高さで並べる。
    pub fn of(items: &[Item], heights: &RowHeights) -> Self {
        let rows = items.iter().map(|item| {
            let open = heights.keyframe_rows_of(item);
            (heights.of(item), open)
        });
        Self::with_keyframe_rows(rows)
    }

    /// 行の数。
    pub fn len(&self) -> usize {
        self.heights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    /// 行 `i` の高さ。
    pub fn height(&self, i: usize) -> RowHeight {
        self.heights[i]
    }

    /// 行 `i` の上端。
    pub fn top(&self, i: usize) -> f32 {
        self.tops[i]
    }

    /// 行 `i` の下端。キーフレームの行を開いていれば、その上端。
    pub fn bottom(&self, i: usize) -> f32 {
        self.tops[i] + self.heights[i].px()
    }

    /// 行 `i` の下に開いたキーフレームの行の下端（次の行の上端）。開いていなければ [`Rows::bottom`] と同じ。
    pub fn below(&self, i: usize) -> f32 {
        self.tops[i + 1]
    }

    /// 行 `i` の下に開いたキーフレームの行の数。
    pub fn keyframe_rows(&self, i: usize) -> usize {
        self.keyframe_rows[i]
    }

    /// 行 `i` の下のキーフレームの行 `k` の上端。
    pub fn keyframe_row_top(&self, i: usize, k: usize) -> f32 {
        self.bottom(i) + k as f32 * KEYFRAME_ROW_H
    }

    /// 縦の位置 `y` が行 `i` の下のキーフレームの行に入っていれば、その行の添字。
    pub fn keyframe_row_at(&self, i: usize, y: f32) -> Option<usize> {
        let below = y - self.bottom(i);
        let k = (below / KEYFRAME_ROW_H).floor();
        (below >= 0.0 && y < self.below(i)).then_some(k as usize)
    }

    /// 全部の行の高さの和。
    pub fn content_height(&self) -> f32 {
        self.tops.last().copied().unwrap_or(0.0)
    }

    /// 行 `i` の帯の上端と高さ。
    pub fn bar(&self, i: usize) -> (f32, f32) {
        let (top, bottom) = (self.top(i), self.bottom(i));
        (
            top + BAR_TOP,
            (bottom - top - BAR_TOP - BAR_BOTTOM).max(0.0),
        )
    }

    /// 縦の位置 `y` を含む行。並びの外なら `None`。
    pub fn at(&self, y: f32) -> Option<usize> {
        if !(0.0..self.content_height()).contains(&y) {
            return None;
        }
        Some(self.tops.partition_point(|t| *t <= y) - 1)
    }

    /// 縦の範囲 `[y0, y1)` に掛かる行。
    pub fn overlapping(&self, y0: f32, y1: f32) -> Range<usize> {
        let n = self.len();
        if n == 0 {
            return 0..0;
        }
        let start = self.tops[1..].partition_point(|bottom| *bottom <= y0);
        let end = self.tops[..n].partition_point(|top| *top < y1);
        start..end.max(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Rows {
        Rows::new([RowHeight::Small, RowHeight::Medium, RowHeight::Large])
    }

    #[test]
    fn 行の高さは_small_18_medium_28_large_56() {
        let r = rows();
        assert_eq!((r.top(0), r.top(1), r.top(2)), (0.0, 18.0, 46.0));
        assert_eq!(r.content_height(), 102.0);
        assert_eq!(r.bar(1), (20.0, 25.0), "帯は上に 2、下に 1 の間隔");
        assert_eq!(
            r.bar(0).1,
            15.0,
            "Small の帯は字の大きさ 12 の行の矩形と同じ高さ"
        );
    }

    #[test]
    fn 縦の位置から行を引き_並びの外は無し() {
        let r = rows();
        assert_eq!(r.at(0.0), Some(0));
        assert_eq!(r.at(17.9), Some(0));
        assert_eq!(r.at(18.0), Some(1));
        assert_eq!(r.at(101.0), Some(2));
        assert_eq!((r.at(102.0), r.at(-0.1)), (None, None));
        assert_eq!(Rows::default().at(0.0), None);
    }

    #[test]
    fn 範囲に掛かる行だけを返す() {
        let r = rows();
        assert_eq!(r.overlapping(0.0, 18.0), 0..1, "下端は含まない");
        assert_eq!(r.overlapping(17.0, 19.0), 0..2);
        assert_eq!(r.overlapping(50.0, 60.0), 2..3);
        assert_eq!(r.overlapping(102.0, 200.0), 3..3, "並びより下は空");
        assert_eq!(r.overlapping(-50.0, -1.0), 0..0, "並びより上は空");
        assert_eq!(Rows::default().overlapping(0.0, 100.0), 0..0);
    }

    #[test]
    fn キーフレームの行はレイヤーの行の下に続き_次の行を押し下げる() {
        let r = Rows::with_keyframe_rows([(RowHeight::Medium, 2), (RowHeight::Small, 0)]);
        assert_eq!((r.top(0), r.bottom(0), r.below(0)), (0.0, 28.0, 68.0));
        assert_eq!((r.top(1), r.bottom(1), r.below(1)), (68.0, 86.0, 86.0));
        assert_eq!(r.content_height(), 86.0);
        assert_eq!(r.bar(0), (2.0, 25.0), "帯はレイヤーの行の中");
        assert_eq!(r.keyframe_row_top(0, 1), 48.0);
        let at = |y| r.keyframe_row_at(0, y);
        assert_eq!(
            (at(27.9), at(28.0), at(67.9), at(68.0)),
            (None, Some(0), Some(1), None)
        );
        assert_eq!(
            r.at(50.0),
            Some(0),
            "キーフレームの行の所はそのレイヤーの行"
        );
        assert_eq!(r.overlapping(40.0, 41.0), 0..1);
    }

    #[test]
    fn 開いたレイヤーだけキーフレームの行を数える() {
        let row = crate::keyframe_rows::KeyframeRow {
            name: "Opacity".into(),
            keyframes: Vec::new(),
        };
        let item = Item::layer(0.0, 1.0, ItemKind::Text, true)
            .with_id("a")
            .with_keyframe_rows(vec![row.clone(), row]);
        let mut h = RowHeights::default();
        assert_eq!(h.keyframe_rows_of(&item), 0);
        h.keyframe_rows_open.insert("a".into());
        assert_eq!(h.keyframe_rows_of(&item), 2);
        let rows = Rows::of(&[item], &h);
        assert_eq!(rows.content_height(), 28.0 + 2.0 * KEYFRAME_ROW_H);
    }

    #[test]
    fn 既定は_sequence_の行だけ_large() {
        let h = RowHeights::default();
        let seq = Item::layer(0.0, 1.0, ItemKind::Sequence, true);
        let text = Item::layer(0.0, 1.0, ItemKind::Text, true);
        assert_eq!(
            (h.of(&seq), h.of(&text)),
            (RowHeight::Large, RowHeight::Medium)
        );
    }

    #[test]
    fn レイヤーごとの高さが全部の行の高さより先() {
        let mut h = RowHeights {
            all: Some(RowHeight::Small),
            ..RowHeights::default()
        };
        let seq = Item::layer(0.0, 1.0, ItemKind::Sequence, true).with_id("main");
        let text = Item::layer(0.0, 1.0, ItemKind::Text, true).with_id("title");
        assert_eq!(
            (h.of(&seq), h.of(&text)),
            (RowHeight::Small, RowHeight::Small)
        );
        h.layers.insert("title".into(), RowHeight::Large);
        assert_eq!(
            (h.of(&seq), h.of(&text)),
            (RowHeight::Small, RowHeight::Large)
        );
        let rows = Rows::of(&[seq, text], &h);
        assert_eq!(rows.content_height(), 18.0 + 56.0);
    }
}
