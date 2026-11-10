//! Timeline を矩形の並びにする。座標は Timeline の矩形の左上を (0, 0) とする。
//! 文字（[`crate::timeline_labels`]）は [`timeline_quads`] の上、[`timeline_overlay`] の下に描く。
//! 帯のサムネイル（[`crate::timeline_thumbs`]）は、文字の下の矩形のうち帯と帯の中の波形までの後に描く（[`UnderText::bars`]）。
//!
//! 領域ごとの描く関数は `paint/` の下のファイルに置き、ここから名前の順に呼ぶ。描く関数は自分の領域の矩形の外に描かず、
//! 重ね順は呼ぶ順でなく [`Depth`] で決めるので、領域を足す PR はファイルを 1 つと、呼び出しを 1 行足せばよい。

mod header;
mod keymarks;
mod playhead;
mod rows;
mod ruler;
mod scrollbar;
mod snap;
mod wave;
mod zoombar;

use crate::drag_tooltip::DragTooltip;
use crate::geom::{Color, Quad, Rect};
use crate::gesture::EditKind;
use crate::palette::Palette;
use crate::snap::SnapTarget;
use crate::timeline::{Hit, TimelineModel};
use crate::timeline_layout::Viewport;
use crate::wave::Waves;

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
    /// ドラッグで吸い付いた目標。吸い付いたときだけスナップの表示（ガイド線と上下の三角形、帯の端なら端の強調）を出す。
    pub snap: Option<SnapTarget>,
    /// ドラッグ中の見た目（離す前の編集と動かす量）。`trimLayer` は帯の端を動かして描く。
    pub preview: Option<(EditKind, f64)>,
    /// ドラッグ中のツールチップ。編集のドラッグの間だけ。
    pub drag_tooltip: Option<DragTooltip>,
    /// 色。
    pub palette: Palette,
}

/// 描く関数の入力。領域の描く関数は全部これを見る。描くものを足す PR は公開する関数を増やさずにフィールドを足し、
/// [`PaintInput::new`] は足したものを無しで置く。
pub struct PaintInput<'a> {
    pub model: &'a TimelineModel,
    pub vp: &'a Viewport<'a>,
    pub state: &'a PaintState,
    /// 帯の波形のピーク。`None` のときと、ピークの無い素材の帯には波形を描かない。
    pub waves: Option<&'a Waves>,
}

impl<'a> PaintInput<'a> {
    /// モデル、見えている所、描く時点の状態だけの入力（波形は描かない）。
    pub fn new(model: &'a TimelineModel, vp: &'a Viewport<'a>, state: &'a PaintState) -> Self {
        Self {
            model,
            vp,
            state,
            waves: None,
        }
    }
}

/// 文字の下に描く矩形と、その中の帯までの数。
#[derive(Debug, Clone, PartialEq)]
pub struct UnderText {
    /// 下から順の矩形。
    pub quads: Vec<Quad>,
    /// 先頭から帯と帯の中の波形（`Depth::Wave`）までの矩形の数。テクスチャで描くサムネイルは `quads[..bars]` の後、
    /// 残りの前に描く（波形の上、端のつまみと外枠の下。サムネイルの下の波形は隠れる）。
    pub bars: usize,
}

/// 重ね順。下から順に描く。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Depth {
    /// 背景と行の縞。
    Back,
    /// 帯とカット。
    Bar,
    /// 帯の中の波形。帯の上、端のつまみと外枠の下。
    Wave,
    /// 帯に重ねるもの（端のつまみ、選んだ外枠）。
    OnBar,
    /// 帯のマーカー（帯の下端のキーフレームの印）。端のつまみと外枠より上。
    BarMark,
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

    /// ひし形や三角（`QuadShape`）。切ると形が変わるので、行の領域に収まるかは呼ぶ側が決める。
    pub(crate) fn push_quad(&mut self, depth: Depth, quad: Quad) {
        self.depths[depth as usize].push(quad);
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

    fn into_quads(self) -> Vec<Quad> {
        self.depths.into_iter().flatten().collect()
    }

    /// 下から並べ、帯の中の波形（`Depth::Wave`）までの矩形の数も数える。
    fn into_under_text(self) -> UnderText {
        let bars = self.depths[..=Depth::Wave as usize]
            .iter()
            .map(Vec::len)
            .sum();
        UnderText {
            quads: self.into_quads(),
            bars,
        }
    }
}

/// 文字の下に描く矩形（背景、行の縞、帯、帯の波形、吸い付いた帯の端の強調、目盛りの帯、スクロールバー、ズームバー）と、
/// その中の帯までの数。
pub fn timeline_quads(input: &PaintInput) -> UnderText {
    let mut canvas = Canvas::default();
    canvas.push(
        Depth::Back,
        input.vp.layout.bounds,
        input.state.palette.panel,
    );
    header::paint(input, &mut canvas);
    keymarks::paint(input, &mut canvas);
    rows::paint(input, &mut canvas);
    ruler::paint(input, &mut canvas);
    scrollbar::paint(input, &mut canvas);
    snap::edge_highlight(input, &mut canvas);
    wave::paint(input, &mut canvas);
    zoombar::paint(input, &mut canvas);
    canvas.into_under_text()
}

/// 文字の上に重ねる矩形（再生位置の線とスナップの表示）。目盛りの帯と行の領域を縦に貫く。
pub fn timeline_overlay(input: &PaintInput) -> Vec<Quad> {
    let mut canvas = Canvas::default();
    playhead::paint(input, &mut canvas);
    snap::paint(input, &mut canvas);
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
