//! 空の状態の案内の文字。コンポジションに中身が無いとき、Program に取り込みの案内を、Timeline の行の領域に
//! 3 点編集の手順を出す。どれを出すかはアプリがプロジェクトのデータから決め（[`EmptyState`]）、
//! キーはアプリのキーの表から受ける（[`EmptyKeys`]）。キーを受けるのは、割り当てを変えても案内がずれないようにするため。

use std::sync::Arc;

use crate::geom::Rect;
use crate::paint::PaintState;
use crate::text::{line_height, place, Align, Label, TextMeasure, TextStyle, UiFont};
use crate::timeline_layout::Viewport;

/// 案内の左右の余白。
const PAD: f32 = 8.0;
/// Program の案内の 2 行の間隔。
const LINE_GAP: f32 = 4.0;
/// Program の案内の 1 行目の字の大きさ（`font.base` に対する比）。
const FIRST_LINE_SCALE: f32 = 1.5;

/// 空の状態の案内のどれを出すか。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmptyState {
    /// Program の取り込みの案内。
    pub program: bool,
    /// Timeline の行の領域の 3 点編集の手順。
    pub timeline: bool,
}

/// 空の状態の案内に出すキー。画面に出す見た目（`Ctrl+I`、`,`）の文字列。
#[derive(Debug, Clone, PartialEq)]
pub struct EmptyKeys {
    /// File > Import Media…。
    pub import_media: Arc<str>,
    pub mark_in: Arc<str>,
    pub mark_out: Arc<str>,
    pub insert: Arc<str>,
}

impl Default for EmptyKeys {
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

/// 空の状態の案内。どれを出すかと、案内に出すキー。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EmptyHints {
    pub show: EmptyState,
    pub keys: EmptyKeys,
}

impl EmptyHints {
    /// Program（矩形 `area`）の案内の 2 行。矩形の縦の中央に置く。出さないときは空。
    pub fn program(
        &self,
        area: Rect,
        state: &PaintState,
        font: &UiFont,
        measure: &mut dyn TextMeasure,
    ) -> Vec<Label> {
        if !self.show.program {
            return Vec::new();
        }
        // Program の背景はテーマに依らない暗い色なので、1 行目は見出しの列の上の角のタイムコードと同じ明るい字にする。
        let first = TextStyle {
            size: (font.base * FIRST_LINE_SCALE).round(),
            color: state.palette.selection,
        };
        let second = TextStyle {
            size: font.base,
            color: state.palette.label,
        };
        let lines: [(Arc<str>, TextStyle); 2] = [
            ("Import video and audio to start".into(), first),
            (
                format!("File > Import Media\u{2026} ({})", self.keys.import_media).into(),
                second,
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

    /// Timeline の行の領域の案内（Timeline の座標）。行の領域の縦の中央に置く。出さないときは空。
    pub fn timeline(
        &self,
        vp: &Viewport,
        state: &PaintState,
        font: &UiFont,
        measure: &mut dyn TextMeasure,
    ) -> Vec<Label> {
        if !self.show.timeline {
            return Vec::new();
        }
        let keys = &self.keys;
        let text: Arc<str> = format!(
            "Open a clip in Media, mark In ({}) and Out ({}), then press Insert ({})",
            keys.mark_in, keys.mark_out, keys.insert
        )
        .into();
        let style = TextStyle {
            size: font.base,
            color: state.palette.label,
        };
        let area = vp.layout.row_area;
        place(&text, area, PAD, Align::Center, style, measure)
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests;
