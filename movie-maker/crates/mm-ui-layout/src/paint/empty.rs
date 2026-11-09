//! レイヤーが無いときの案内の文字。Program の中と Timeline の行の領域に、次にすることとキーを出す。
//! キーは呼ぶ側（アプリのキーの表）から [`EmptyHints`] で受け、キーを変えても案内がずれないようにする。

use std::sync::Arc;

use crate::geom::Rect;
use crate::palette::Palette;
use crate::text::{line_height, place, Align, Label, TextMeasure, TextStyle, UiFont};
use crate::timeline::TimelineModel;
use crate::timeline_layout::Viewport;

/// 案内の左右の余白。
const PAD: f32 = 8.0;
/// Program の案内の 2 行の間隔。
const LINE_GAP: f32 = 4.0;
/// Program の案内の見出しの字の大きさ（`font.base` に対する比）。
const TITLE_SCALE: f32 = 1.5;

/// 案内に出すキー。画面に出す見た目（`Ctrl+I`、`,`）の文字列。
#[derive(Debug, Clone, PartialEq)]
pub struct EmptyHints {
    /// File > Import Media…。
    pub import_media: Arc<str>,
    pub mark_in: Arc<str>,
    pub mark_out: Arc<str>,
    /// Insert。
    pub insert: Arc<str>,
}

impl Default for EmptyHints {
    /// アプリのキーの表の既定の割り当てと同じ。
    fn default() -> Self {
        Self {
            import_media: "Ctrl+I".into(),
            mark_in: "I".into(),
            mark_out: "O".into(),
            insert: ",".into(),
        }
    }
}

/// Program（矩形 `area`）の案内。レイヤーが 1 つでもあれば空。
pub fn program_labels(
    model: &TimelineModel,
    area: Rect,
    hints: &EmptyHints,
    palette: &Palette,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    if !model.items.is_empty() {
        return Vec::new();
    }
    let lines: [(Arc<str>, TextStyle); 2] = [
        (
            "Import video and audio to start".into(),
            TextStyle {
                size: (font.base * TITLE_SCALE).round(),
                color: palette.selection,
            },
        ),
        (
            format!("File > Import Media\u{2026} ({})", hints.import_media).into(),
            TextStyle {
                size: font.base,
                color: palette.label,
            },
        ),
    ];
    let heights = lines.each_ref().map(|(_, s)| line_height(s.size));
    let mut y = area.y + (area.h - heights[0] - heights[1] - LINE_GAP) / 2.0;
    let mut out = Vec::new();
    for ((text, style), h) in lines.iter().zip(heights) {
        let row = Rect::new(area.x, y, area.w, h);
        out.extend(place(text, row, PAD, Align::Center, *style, measure));
        y += h + LINE_GAP;
    }
    out
}

/// Timeline の行の領域の案内。レイヤーが 1 つでもあれば空。
pub fn timeline_labels(
    model: &TimelineModel,
    vp: &Viewport,
    hints: &EmptyHints,
    palette: &Palette,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
) -> Vec<Label> {
    if !model.items.is_empty() {
        return Vec::new();
    }
    let text: Arc<str> = format!(
        "Open a clip in Media, mark In ({}) and Out ({}), then press Insert ({})",
        hints.mark_in, hints.mark_out, hints.insert
    )
    .into();
    let style = TextStyle {
        size: font.base,
        color: palette.label,
    };
    place(
        &text,
        vp.layout.row_area,
        PAD,
        Align::Center,
        style,
        measure,
    )
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests;
