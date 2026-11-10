//! ツールバーの文字。字のアイコンとボタンの名前。座標は `bar` と同じ。

use std::sync::Arc;

use super::{cells, ButtonLook, Icon, ToolbarController, ToolbarState};
use crate::geom::Rect;
use crate::palette::Palette;
use crate::text::{place, Align, Label, TextMeasure, TextStyle, UiFont};

/// 字のアイコンの大きさ。
const GLYPH_SIZE: f32 = 14.0;

/// ツールバー `bar` の文字。名前を出すボタンの名前と、字のアイコン（記号のフォントが無ければ標準の記号の字）。
pub fn toolbar_labels(
    bar: Rect,
    state: &ToolbarState,
    controller: &ToolbarController,
    palette: &Palette,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    let mut out = Vec::new();
    for cell in cells(bar, &state.names).buttons {
        let look = ButtonLook::of(&cell, state, controller);
        let color = look.ink(palette);
        if let Icon::Glyph { symbol, plain } = cell.spec.icon {
            let c = if state.symbols { symbol } else { plain };
            let glyph: Arc<str> = c.to_string().into();
            let style = TextStyle {
                size: GLYPH_SIZE,
                color,
            };
            let area = look.place(cell.icon_box());
            out.extend(place(&glyph, area, 0.0, Align::Center, style, measure));
        }
        if let Some(area) = cell.name_box() {
            let style = TextStyle {
                size: font.base,
                color,
            };
            let name: Arc<str> = cell.spec.name.into();
            out.extend(place(
                &name,
                look.place(area),
                0.0,
                Align::Left,
                style,
                measure,
            ));
        }
    }
    out
}
