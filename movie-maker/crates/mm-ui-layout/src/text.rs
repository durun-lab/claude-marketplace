//! サーフェスの UI の文字。幅を測るインターフェース（[`TextMeasure`]）、幅に収める切り詰め（[`fit`]）、
//! 矩形の中に置く関数（[`place`]）、描く文字（[`Label`]）。
//! 幅を測って字を描くのは `mm-ui-wgpu`（cosmic-text と glyphon）で、ここは測った幅から置き方だけを決める。

use std::sync::Arc;

use crate::geom::{Color, Rect};

/// 切り詰めた文字列の後ろに付ける字。
pub const ELLIPSIS: &str = "\u{2026}";
/// 行の高さ（字の大きさに対する比）。
const LINE_HEIGHT: f32 = 1.25;

/// サーフェスの UI の字。theme.json の `font`（`family` の先頭と `size`）から作る。
#[derive(Debug, Clone, PartialEq)]
pub struct UiFont {
    /// フォントの名前。このフォントに無い字（日本語など）は `mm-ui-wgpu` のフォールバックのフォントで描く。
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

/// 字の大きさ `size` の 1 行の高さ。シェーピングするときの行の高さと、帯の中で縦に揃える計算が同じ値を使う。
pub fn line_height(size: f32) -> f32 {
    (size * LINE_HEIGHT).ceil()
}

/// 文字列の幅を測るもの。実装は `mm-ui-wgpu`（cosmic-text でシェーピングしてキャッシュする）。
pub trait TextMeasure {
    /// `text` を大きさ `size` で 1 行にシェーピングしたときの、切り詰めの位置ごとの左端からの距離。昇順で、最後が全体の幅。
    /// 切り詰めの位置は書記素クラスタの終わりで、空白の後ろは除く。空の文字列は空。
    fn stops(&mut self, text: &str, size: f32) -> &[f32];

    /// 再生位置のタイムコードを 1 字ずつ等幅の数字で並べたときの、[`stops`](Self::stops) と同じ形の距離。
    /// 数字は 0〜9 の一番広い送り幅で並べるので、再生中に数字が変わっても字が左右に揺れない。
    /// 並べ方は `mm-ui-wgpu` の描く所と同じ。既定は文字列ごとのシェーピングの距離。
    fn timecode_stops(&mut self, text: &str, size: f32) -> Vec<f32> {
        self.stops(text, size).to_vec()
    }

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

/// 切り詰めの位置が `stops` の文字列を幅 `max` に収める。全体が入れば全体、入らなければ「…」（幅 `ellipsis`）を足して入る所まで。
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
    /// 行の矩形（高さは [`line_height`]）の左上。
    pub x: f32,
    pub y: f32,
    /// 字の大きさ。
    pub size: f32,
    pub color: Color,
    /// 切り詰め。`Some(w)` なら左から幅 `w` だけ描き、続けて「…」を描く。
    pub cut: Option<f32>,
    /// 字を描いてよい矩形。外に出た字の部分は描かない。
    pub clip: Rect,
}

/// 文字を置く矩形の中の、横の揃え方。入らずに切り詰めるときは左に揃える。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// 文字の見た目。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// 字の大きさ（論理ピクセル）。
    pub size: f32,
    pub color: Color,
}

/// `text` を矩形 `rect` の中に 1 行で置く。縦は中央、横は左右に `pad` の余白を空けて `align` に揃える。
/// 入らなければ切り詰め、1 字と「…」も入らなければ `None`。字は `rect` の外に描かない（[`Label::clip`]）。
/// 帯の名前のほか、レイヤーの見出し、ツールバー、ステータスバー、再生の操作のバーのように、矩形に名前や時刻を出す所が共通に使う。
pub fn place(
    text: &Arc<str>,
    rect: Rect,
    pad: f32,
    align: Align,
    style: TextStyle,
    measure: &mut dyn TextMeasure,
) -> Option<Label> {
    if !has_room(text, rect, pad) {
        return None;
    }
    let ellipsis = measure.width(ELLIPSIS, style.size);
    let stops = measure.stops(text, style.size);
    place_at(text, rect, pad, align, style, (stops, ellipsis))
}

/// [`place`] と同じ置き方で、再生位置のタイムコードを置く。幅は等幅の数字で 1 字ずつ並べた幅で測る
/// （[`TextMeasure::timecode_stops`]）。再生中は毎フレーム変わる文字なので、描く所はほかの文字と頂点を分け、
/// 1 字ずつキャッシュしたシェーピングの結果を並べて描く。
pub fn place_timecode(
    text: &Arc<str>,
    rect: Rect,
    pad: f32,
    align: Align,
    style: TextStyle,
    measure: &mut dyn TextMeasure,
) -> Option<Label> {
    if !has_room(text, rect, pad) {
        return None;
    }
    let ellipsis = measure.width(ELLIPSIS, style.size);
    let stops = measure.timecode_stops(text, style.size);
    place_at(text, rect, pad, align, style, (&stops, ellipsis))
}

/// 空でない `text` を、`rect` に左右 `pad` の余白を空けて置ける幅があるか。無ければ測らない。
fn has_room(text: &str, rect: Rect, pad: f32) -> bool {
    !text.is_empty() && rect.w - 2.0 * pad > 0.0
}

/// 切り詰めの位置が `stops`、省略記号の幅が `ellipsis` の文字を置く（[`place`]）。
fn place_at(
    text: &Arc<str>,
    rect: Rect,
    pad: f32,
    align: Align,
    style: TextStyle,
    (stops, ellipsis): (&[f32], f32),
) -> Option<Label> {
    let room = rect.w - 2.0 * pad;
    let whole = stops.last().copied().unwrap_or(0.0);
    let (x, cut) = match fit(stops, ellipsis, room) {
        Fit::Whole => {
            let free = room - whole;
            let offset = match align {
                Align::Left => 0.0,
                Align::Center => free / 2.0,
                Align::Right => free,
            };
            (rect.x + pad + offset, None)
        }
        Fit::Cut(w) => (rect.x + pad, Some(w)),
        Fit::Hidden => return None,
    };
    Some(Label {
        text: text.clone(),
        x,
        y: rect.y + (rect.h - line_height(style.size)) / 2.0,
        size: style.size,
        color: style.color,
        cut,
        clip: rect,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 半角は字の大きさの半分、それ以外は字の大きさの幅で測る、テスト用の実装。
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
    fn 入らなければ省略記号を足して入る所までにする() {
        let stops = [5.0, 10.0, 15.0, 20.0];
        assert_eq!(fit(&stops, 6.0, 19.0), Fit::Cut(10.0), "10 + 6 <= 19");
        assert_eq!(fit(&stops, 6.0, 16.0), Fit::Cut(10.0), "ちょうど入る");
        assert_eq!(fit(&stops, 6.0, 15.9), Fit::Cut(5.0));
    }

    #[test]
    fn 一字と省略記号も入らなければ出さない() {
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
        // "ab cd" の切り詰めの位置は a、b、c、d の後ろ（空白の後ろは無い）。
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

    const STYLE: TextStyle = TextStyle {
        size: 10.0,
        color: Color([1.0; 4]),
    };

    fn placed(text: &str, rect: Rect, align: Align) -> Option<Label> {
        place(
            &text.into(),
            rect,
            4.0,
            align,
            STYLE,
            &mut FakeMeasure::default(),
        )
    }

    #[test]
    fn 矩形に置く文字は縦の中央で_入れば揃え方の位置に置き_矩形の外に描かない() {
        // "abcd" は 20、置ける幅は 100 - 8 = 92。
        let rect = Rect::new(10.0, 20.0, 100.0, 30.0);
        let xs = [Align::Left, Align::Center, Align::Right].map(|a| {
            let l = placed("abcd", rect, a).expect("入る");
            assert_eq!((l.cut, l.clip), (None, rect));
            assert_eq!(l.y, 20.0 + (30.0 - line_height(10.0)) / 2.0);
            l.x
        });
        assert_eq!(xs, [14.0, 14.0 + 36.0, 14.0 + 72.0]);
    }

    #[test]
    fn 矩形に入らない文字は左に揃えて切り詰め_一字と省略記号も入らなければ置かない() {
        // 省略記号は 10、置ける幅は 30 - 8 = 22 なので 2 字（10）と省略記号まで。
        let rect = Rect::new(0.0, 0.0, 30.0, 20.0);
        for align in [Align::Left, Align::Center, Align::Right] {
            let l = placed("abcdefgh", rect, align).expect("切り詰めて置く");
            assert_eq!((l.x, l.cut), (4.0, Some(10.0)), "{align:?}");
        }
        assert_eq!(
            placed("abc", Rect::new(0.0, 0.0, 20.0, 20.0), Align::Left),
            None
        );
        assert_eq!(placed("", rect, Align::Left), None, "空の文字列");
    }

    /// 等幅の数字で並べた幅を、文字列ごとの幅の 2 倍で返すテスト用の実装。
    struct WideTimecode(FakeMeasure);

    impl TextMeasure for WideTimecode {
        fn stops(&mut self, text: &str, size: f32) -> &[f32] {
            self.0.stops(text, size)
        }

        fn timecode_stops(&mut self, text: &str, size: f32) -> Vec<f32> {
            self.0.stops(text, size).iter().map(|s| s * 2.0).collect()
        }
    }

    #[test]
    fn 再生位置のタイムコードは等幅の数字で並べた幅で置く() {
        // "12:34" は文字列ごとの幅が 25、等幅の数字で 50。置ける幅は 100 - 8 = 92。
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let mut m = WideTimecode(FakeMeasure::default());
        let text: Arc<str> = "12:34".into();
        let plain = place(&text, rect, 4.0, Align::Right, STYLE, &mut m).expect("入る");
        let timecode = place_timecode(&text, rect, 4.0, Align::Right, STYLE, &mut m).expect("入る");
        assert_eq!(
            (plain.x, timecode.x),
            (4.0 + 92.0 - 25.0, 4.0 + 92.0 - 50.0)
        );
    }
}
