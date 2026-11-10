//! 素材を開いている途中の印。プレビューの描画は長い録画の素材を開き終わるのを待たずにそのレイヤーを除くので、除いた
//! フレームを出している間は、ビューアーの左上に「Media Pending」を出す（Premiere Pro が素材の索引を作る間に Program に
//! 出す Media Pending に倣う）。除いたレイヤーが消えたように見えないよう、映像の上でも読めるようパネルの色の矩形を敷く。

use std::sync::Arc;

use crate::geom::{Quad, Rect};
use crate::palette::Palette;
use crate::text::{line_height, place, Align, Label, TextMeasure, TextStyle, UiFont};

/// 印の文字。
pub const MEDIA_PENDING: &str = "Media Pending";
/// ビューアーの角から印までの間隔。
const MARGIN: f32 = 8.0;
/// 印の矩形の中の左右の余白。
const PAD: f32 = 6.0;

/// ビューアー（矩形 `area`）の左上に置く印の文字と、その下に敷く矩形。ビューアーに入らなければ `None`。
pub fn pending_badge(
    area: Rect,
    palette: &Palette,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Option<(Label, Quad)> {
    let style = TextStyle {
        size: font.base,
        color: palette.label,
    };
    let w = measure.width(MEDIA_PENDING, style.size) + 2.0 * PAD;
    let h = line_height(style.size);
    if w + 2.0 * MARGIN > area.w || h + 2.0 * MARGIN > area.h {
        return None;
    }
    let rect = Rect::new(area.x + MARGIN, area.y + MARGIN, w, h);
    let text: Arc<str> = MEDIA_PENDING.into();
    let label = place(&text, rect, PAD, Align::Left, style, measure)?;
    Some((label, Quad::new(rect, palette.panel)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::tests::FakeMeasure;

    fn badge(area: Rect) -> Option<(Label, Quad)> {
        let mut m = FakeMeasure::default();
        pending_badge(area, &Palette::DEFAULT, &UiFont::default(), &mut m)
    }

    #[test]
    fn 印はビューアーの左上に置き_文字の下にパネルの色の矩形を敷く() {
        let area = Rect::new(100.0, 50.0, 640.0, 360.0);
        let (label, quad) = badge(area).expect("入る");
        assert_eq!(&*label.text, MEDIA_PENDING);
        assert_eq!((quad.rect.x, quad.rect.y), (108.0, 58.0));
        assert_eq!(quad.color, Palette::DEFAULT.panel);
        assert!(label.x >= quad.rect.x && label.x < quad.rect.right());
        assert_eq!(label.clip, quad.rect, "矩形の外に字を描かない");
    }

    #[test]
    fn ビューアーに入らなければ置かない() {
        assert!(badge(Rect::new(0.0, 0.0, 60.0, 360.0)).is_none());
        assert!(badge(Rect::new(0.0, 0.0, 640.0, 20.0)).is_none());
    }
}
