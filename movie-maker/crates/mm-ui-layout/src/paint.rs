//! Timeline を矩形の並びにする。座標は Timeline の矩形の左上を (0, 0) とする。
//! 文字（[`crate::timeline_labels`]）は [`timeline_quads`] の上、[`timeline_overlay`] の下に描く。
//!
//! 領域ごとの描く関数は `paint/` の下のファイルに置き、ここから名前の順に呼ぶ。描く関数は自分の領域の矩形の外に描かず、
//! 重ね順は呼ぶ順でなく [`Depth`] で決めるので、領域を足す PR はファイルを 1 つと、呼び出しを 1 行足せばよい。

mod header;
mod playhead;
mod rows;
mod ruler;
mod scrollbar;
mod snap;

use crate::drag_tip::DragTip;
use crate::geom::{Color, Quad, Rect};
use crate::gesture::EditKind;
use crate::palette::Palette;
use crate::snap::SnapTarget;
use crate::timeline::{Hit, TimelineModel};
use crate::timeline_layout::Viewport;

pub(crate) use rows::{bar_color, stripe, trimmed};

/// 描く時点の状態。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaintState {
    /// 再生位置（秒）。
    pub playhead: f64,
    /// ポインターが乗っている所。
    pub hover: Hit,
    /// 選ばれたレイヤー。
    pub selected: Option<usize>,
    /// 選ばれたカット（レイヤーの添字、カットの添字）。
    pub selected_cut: Option<(usize, usize)>,
    /// ドラッグで吸い付いた目標。吸い付いたときだけスナップの表示（ガイド線と上下の三角、帯の端なら端の強調）を出す。
    pub snap: Option<SnapTarget>,
    /// ドラッグ中の見た目（離す前の編集と動かす量）。`trimLayer` は帯の端を動かして描く。
    pub preview: Option<(EditKind, f64)>,
    /// ドラッグ中のツールチップ。編集のドラッグの間だけ。
    pub drag: Option<DragTip>,
    /// 色。
    pub palette: Palette,
}

/// 領域の描く関数が見るもの。
pub(crate) struct PaintInput<'a> {
    pub model: &'a TimelineModel,
    pub vp: &'a Viewport<'a>,
    pub state: &'a PaintState,
}

/// 重ね順。下から順に描く。帯の中に描く矩形（波形）は `Bar` と `OnBar` の間に足す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Depth {
    /// 背景と行の縞。
    Back,
    /// 帯とカット。
    Bar,
    /// 帯に重ねるもの（端のつまみ、選んだ外枠）。
    OnBar,
    /// 行に重ねる暗がり（コンポジションの終わりより右）。
    Shade,
    /// 行の領域を囲む部品（目盛りの帯、スクロールバー）と、それを貫く時刻の線。いちばん上。
    Surround,
}

/// 重ね順の数。
const DEPTHS: usize = Depth::Surround as usize + 1;

/// 重ね順ごとに矩形を貯め、最後に下から並べる。
#[derive(Debug, Default)]
pub(crate) struct Canvas {
    depths: [Vec<Quad>; DEPTHS],
}

impl Canvas {
    pub(crate) fn push(&mut self, depth: Depth, rect: Rect, color: Color) {
        self.depths[depth as usize].push(Quad::new(rect, color));
    }

    /// `rect` の `clip` に入る所だけを入れる。全部外なら入れない。
    /// 切ってよいのは矩形だけで、ひし形や三角（`QuadShape`）は切ると形が変わるのでここを通さない。
    pub(crate) fn push_in(&mut self, depth: Depth, rect: Rect, color: Color, clip: Rect) {
        let (x0, y0) = (rect.x.max(clip.x), rect.y.max(clip.y));
        let (x1, y1) = (
            rect.right().min(clip.right()),
            rect.bottom().min(clip.bottom()),
        );
        if x1 > x0 && y1 > y0 {
            self.push(depth, Rect::new(x0, y0, x1 - x0, y1 - y0), color);
        }
    }

    /// 形のある図形（ひし形、三角）を、外接する矩形が `clip` に全部入るときだけ入れる。切ると形が変わるので、
    /// はみ出すものは描かない。
    pub(crate) fn push_shape(&mut self, depth: Depth, quad: Quad, clip: Rect) {
        let (r, slack) = (quad.rect, 1e-3);
        let inside = r.x >= clip.x - slack
            && r.y >= clip.y - slack
            && r.right() <= clip.right() + slack
            && r.bottom() <= clip.bottom() + slack;
        if inside {
            self.depths[depth as usize].push(quad);
        }
    }

    fn into_quads(self) -> Vec<Quad> {
        self.depths.into_iter().flatten().collect()
    }
}

/// 文字の下に描く矩形（背景、行の縞、帯、吸い付いた帯の端、目盛りの帯、スクロールバー）。
pub fn timeline_quads(model: &TimelineModel, vp: &Viewport, state: &PaintState) -> Vec<Quad> {
    let input = PaintInput { model, vp, state };
    let mut canvas = Canvas::default();
    canvas.push(Depth::Back, vp.layout.bounds, state.palette.panel);
    header::paint(&input, &mut canvas);
    rows::paint(&input, &mut canvas);
    ruler::paint(&input, &mut canvas);
    scrollbar::paint(&input, &mut canvas);
    snap::edge(&input, &mut canvas);
    canvas.into_quads()
}

/// 文字の上に重ねる矩形（再生位置の線とスナップの表示）。目盛りの帯と行の領域を縦に貫く。
pub fn timeline_overlay(model: &TimelineModel, vp: &Viewport, state: &PaintState) -> Vec<Quad> {
    let input = PaintInput { model, vp, state };
    let mut canvas = Canvas::default();
    playhead::paint(&input, &mut canvas);
    snap::paint(&input, &mut canvas);
    canvas.into_quads()
}

/// 時刻の x に立てる幅 `width` の縦の線（目盛りの帯と行の領域の列に収める）。
fn vline(canvas: &mut Canvas, vp: &Viewport, t: f64, width: f32, color: Color) {
    let column = vp.layout.time_column();
    let x = vp.x_of(t) - width / 2.0;
    let line = Rect::new(x, column.y, width, column.h);
    canvas.push_in(Depth::Surround, line, color, column);
}

#[cfg(test)]
pub(crate) mod tests;
