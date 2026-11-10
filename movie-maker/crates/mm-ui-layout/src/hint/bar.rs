//! ステータスバーの文字。左にポインターの下でできること、右に Snapping の入/切と今のツール。座標は `bar` と同じ。

use std::sync::Arc;

use crate::geom::Rect;
use crate::palette::Palette;
use crate::text::{place, Align, Label, TextMeasure, TextStyle, UiFont};

/// 左右の余白と、左右の文の間の最小の間隔。
const PAD: f32 = 8.0;
const GAP: f32 = 16.0;

/// ステータスバー `bar` の文字。右の文を先に置き、左はその手前までに収める（入らなければ切り詰める）。
pub fn status_labels(
    bar: Rect,
    (left, right): (&str, &str),
    palette: &Palette,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    if bar.h <= 0.0 || bar.w <= 0.0 {
        return Vec::new();
    }
    let style = |color| TextStyle {
        size: font.base,
        color,
    };
    // 右の文も左の文と同じく、バーの端から余白を空ける。
    let right_w = if right.is_empty() {
        0.0
    } else {
        measure.width(right, font.base).ceil() + 2.0 * PAD
    };
    let right_area = Rect::new(bar.right() - right_w, bar.y, right_w, bar.h);
    let left_w = (bar.w - right_w - GAP).max(0.0);
    let left_area = Rect::new(bar.x, bar.y, left_w, bar.h);
    let (left, right): (Arc<str>, Arc<str>) = (left.into(), right.into());
    let mut out = Vec::new();
    let ink = style(palette.selection);
    out.extend(place(&left, left_area, PAD, Align::Left, ink, measure));
    let ink = style(palette.label);
    out.extend(place(&right, right_area, PAD, Align::Right, ink, measure));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::tests::FakeMeasure;

    fn labels(bar: Rect, left: &str, right: &str) -> Vec<Label> {
        let mut m = FakeMeasure::default();
        let font = UiFont::default();
        status_labels(bar, (left, right), &Palette::DEFAULT, &font, &mut m)
    }

    #[test]
    fn 左は左端から_右は右に揃えて_どちらも_bar_の中() {
        let bar = Rect::new(0.0, 100.0, 600.0, 22.0);
        let l = labels(bar, "Drag: Slip", "Snapping On");
        assert_eq!(l.len(), 2);
        assert_eq!(&*l[0].text, "Drag: Slip");
        assert_eq!(l[0].x, PAD);
        assert_eq!(&*l[1].text, "Snapping On");
        assert!(l[1].x > l[0].x);
        let right_end = l[1].x + FakeMeasure::default().width("Snapping On", l[1].size);
        assert_eq!(right_end, bar.right() - PAD, "右の文も端から余白を空ける");
        assert!(l
            .iter()
            .all(|l| l.clip.y >= bar.y && l.clip.bottom() <= bar.bottom()));
    }

    #[test]
    fn 左が長ければ右の手前で切り詰め_右は残す() {
        let bar = Rect::new(0.0, 0.0, 200.0, 22.0);
        let long = "Ripple Trim +12f \u{B7} Snapped to Playhead \u{B7} N: Toggle Snapping";
        let l = labels(bar, long, "Snapping On");
        let right = l
            .iter()
            .find(|l| &*l.text == "Snapping On")
            .expect("右は残す");
        let left = l.iter().find(|l| &*l.text == long).expect("左");
        assert!(left.cut.is_some(), "切り詰める");
        assert!(left.clip.right() <= right.clip.x, "右と重ならない");
    }

    #[test]
    fn 高さが無ければ何も置かない() {
        assert!(labels(Rect::new(0.0, 0.0, 600.0, 0.0), "a", "b").is_empty());
    }
}
