//! サーフェスの UI の文字。幅を測る口（[`TextMeasure`]）、幅に収める切り詰め（[`fit`]）、描く文字（[`Label`]）。
//! 幅を測って字を描くのは `mm-ui-wgpu`（cosmic-text と glyphon）で、ここは測った幅から置き方だけを決める。

use std::sync::Arc;

use crate::geom::Color;

/// 切り詰めた文字列の後ろに付ける字。
pub const ELLIPSIS: &str = "\u{2026}";
/// 行の高さ（字の大きさに対する比）。
const LINE_HEIGHT: f32 = 1.25;

/// サーフェスの UI の字。theme.json の `font`（`family` の先頭と `size`）から作る。
#[derive(Debug, Clone, PartialEq)]
pub struct UiFont {
    /// 書体の名前。この書体に無い字（日本語など）は `mm-ui-wgpu` の代わりの書体で描く。
    pub family: String,
    /// 目盛りの時刻の字の大きさ（`size.sm`）。
    pub small: f32,
    /// 帯の名前の字の大きさ（`size.base`）。
    pub base: f32,
}

impl Default for UiFont {
    /// theme.json と同じ値。
    fn default() -> Self {
        Self {
            family: "Tahoma".into(),
            small: 11.0,
            base: 12.0,
        }
    }
}

/// 字の大きさ `size` の 1 行の高さ。組むときの行の高さと、帯の中で縦に揃える計算が同じ値を使う。
pub fn line_height(size: f32) -> f32 {
    (size * LINE_HEIGHT).ceil()
}

/// 文字列の幅を測るもの。実装は `mm-ui-wgpu`（cosmic-text で組んで覚える）。
pub trait TextMeasure {
    /// `text` を大きさ `size` で 1 行に組んだときの、切ってよい所ごとの左端からの距離。昇順で、最後が全体の幅。
    /// 切ってよい所は字のまとまり（書記素）の終わりで、空白の後ろは除く。空の文字列は空。
    fn stops(&mut self, text: &str, size: f32) -> &[f32];

    /// 全体の幅。
    fn width(&mut self, text: &str, size: f32) -> f32 {
        self.stops(text, size).last().copied().unwrap_or(0.0)
    }
}

/// 文字列を幅に収める置き方。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fit {
    /// 全体が入る。
    Whole,
    /// 左からこの幅までを描き、続けて「…」を描く。
    Cut(f32),
    /// 1 字と「…」も入らないので描かない。
    Hidden,
}

/// 切ってよい所 `stops` の文字列を幅 `max` に収める。全体が入れば全体、入らなければ「…」（幅 `ellipsis`）を足して入る所まで。
pub fn fit(stops: &[f32], ellipsis: f32, max: f32) -> Fit {
    let Some(&whole) = stops.last() else {
        return Fit::Hidden;
    };
    if whole <= max {
        return Fit::Whole;
    }
    match stops.partition_point(|s| s + ellipsis <= max) {
        0 => Fit::Hidden,
        n => Fit::Cut(stops[n - 1]),
    }
}

/// 描く文字 1 つ。座標は論理ピクセル（物理ピクセルへは [`crate::Scale::label_to_physical`]）。
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: Arc<str>,
    /// 行の箱の左上。
    pub x: f32,
    pub y: f32,
    /// 字の大きさ。
    pub size: f32,
    pub color: Color,
    /// 切り詰め。`Some(w)` なら左から幅 `w` だけ描き、続けて「…」を描く。
    pub cut: Option<f32>,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 半角は字の大きさの半分、それ以外は字の大きさの幅で測る偽物。
    #[derive(Default)]
    pub(crate) struct FakeMeasure {
        stops: Vec<f32>,
        /// 測った文字列（呼ばれた回数を確かめる）。
        pub(crate) asked: Vec<String>,
    }

    impl TextMeasure for FakeMeasure {
        fn stops(&mut self, text: &str, size: f32) -> &[f32] {
            self.asked.push(text.to_owned());
            self.stops.clear();
            let mut x = 0.0;
            for c in text.chars() {
                x += if c.is_ascii() { size / 2.0 } else { size };
                if !c.is_whitespace() {
                    self.stops.push(x);
                }
            }
            &self.stops
        }
    }

    #[test]
    fn 全体が入れば切らない() {
        assert_eq!(fit(&[5.0, 10.0, 15.0], 6.0, 15.0), Fit::Whole);
    }

    #[test]
    fn 入らなければ三点を足して入る所までにする() {
        let stops = [5.0, 10.0, 15.0, 20.0];
        assert_eq!(fit(&stops, 6.0, 19.0), Fit::Cut(10.0), "10 + 6 <= 19");
        assert_eq!(fit(&stops, 6.0, 16.0), Fit::Cut(10.0), "ちょうど入る");
        assert_eq!(fit(&stops, 6.0, 15.9), Fit::Cut(5.0));
    }

    #[test]
    fn 一字と三点も入らなければ出さない() {
        assert_eq!(
            fit(&[5.0, 10.0, 15.0], 6.0, 10.9),
            Fit::Hidden,
            "5 + 6 > 10.9"
        );
        assert_eq!(fit(&[], 6.0, 100.0), Fit::Hidden, "空の文字列");
    }

    #[test]
    fn 空白の後ろでは切らない() {
        let mut m = FakeMeasure::default();
        // "ab cd" の切ってよい所は a、b、c、d の後ろ（空白の後ろは無い）。
        let stops = m.stops("ab cd", 10.0).to_vec();
        assert_eq!(stops, [5.0, 10.0, 20.0, 25.0]);
        assert_eq!(
            fit(&stops, 5.0, 22.0),
            Fit::Cut(10.0),
            "「ab …」でなく「ab…」"
        );
    }

    #[test]
    fn 行の高さは字の大きさから整数の論理ピクセルで決まる() {
        assert_eq!(line_height(12.0), 15.0);
        assert_eq!(line_height(11.0), 14.0);
    }
}
