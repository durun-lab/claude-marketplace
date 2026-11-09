//! Timeline のレイヤーの行。行の高さ（Small、Medium、Large）と、行ごとの上端の並び。
//! 縦の座標は行の領域の上端を 0 とする、スクロールの前の値。

use std::collections::BTreeMap;
use std::ops::Range;

use crate::timeline::{Item, ItemKind};

/// 行の上端から帯の上端まで。上の 1 px は行どうしの境の線、次の 1 px は縞の色で帯を囲む。
const BAR_TOP: f32 = 2.0;
/// 帯の下端から行の下端まで（縞の色）。
const BAR_BOTTOM: f32 = 1.0;

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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RowHeights {
    /// 全部の行の高さ（View > Track Height）。`None` は既定（[`RowHeight::default_for`]）。
    pub all: Option<RowHeight>,
    /// レイヤーごとの高さ（レイヤーの見出しの右クリック）。キーはレイヤーの ID。
    pub layers: BTreeMap<String, RowHeight>,
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
}

/// 行の並び。行ごとの高さと上端を持ち、縦の位置から行を二分探索で引く（数千のレイヤーでも見える行だけを扱う）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rows {
    heights: Vec<RowHeight>,
    /// 行 `i` の上端。最後に全部の行の高さの和を持つので、長さは行の数 + 1。空の並びは空。
    tops: Vec<f32>,
}

impl Rows {
    /// 上の行から順の高さで並べる。
    pub fn new(heights: impl IntoIterator<Item = RowHeight>) -> Self {
        let heights: Vec<RowHeight> = heights.into_iter().collect();
        let mut tops = Vec::with_capacity(heights.len() + 1);
        let mut y = 0.0;
        tops.push(y);
        for h in &heights {
            y += h.px();
            tops.push(y);
        }
        Self { heights, tops }
    }

    /// Timeline のレイヤー `items` を、表示の状態 `heights` の高さで並べる。
    pub fn of(items: &[Item], heights: &RowHeights) -> Self {
        Self::new(items.iter().map(|item| heights.of(item)))
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

    /// 行 `i` の下端（次の行の上端）。
    pub fn bottom(&self, i: usize) -> f32 {
        self.tops[i + 1]
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
