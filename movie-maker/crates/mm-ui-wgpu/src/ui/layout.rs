//! `Ui` の割り付け。サーフェスの大きさと表示の倍率、2 つの仕切り（Timeline の高さの割合、縦の仕切りの有無）から領域を決め、
//! 点がどの領域かを答え、仕切りの矩形を描く。

use mm_ui_layout::palette;
use mm_ui_layout::{
    content_width, splitter_rect, surface_layout, vertical_splitter_rect, Quad, Rect, Scale,
    Splitter, SurfaceLayout, TIMELINE_RATIO,
};

use super::{Region, Ui};

/// 仕切りの状態。アプリが毎フレーム渡す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitterState {
    /// Timeline の高さの、サーフェスの高さに対する割合。
    pub timeline_ratio: f32,
    /// サーフェスと WebView の仕切りを出すか（WebView の列を開けているとき）。
    pub vertical: bool,
    /// ポインターが乗っているか、つかんでいる仕切り。色がアクセントになる。
    pub hot: Option<Splitter>,
}

impl Default for SplitterState {
    fn default() -> Self {
        Self {
            timeline_ratio: TIMELINE_RATIO,
            vertical: false,
            hot: None,
        }
    }
}

impl Ui {
    /// サーフェスの大きさ（物理ピクセル）。割り付けを論理ピクセルで計算し直す。
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = (width as f32, height as f32);
        self.layout = surface_layout(
            self.scale.to_logical(self.size.0),
            self.scale.to_logical(self.size.1),
            self.source.has_image(),
            self.splitter.timeline_ratio,
            self.splitter.vertical,
        );
        let t = self.layout.timeline;
        self.controller.resize((t.w, t.h));
        if !self.opened {
            self.open_view();
        }
    }

    /// 仕切りの状態。Timeline の高さの割合と縦の仕切りの有無が変わったときだけ割り付けを計算し直し、色だけの変化は描くときに反映する。
    pub fn set_splitters(&mut self, state: SplitterState) {
        let relayout = (self.splitter.timeline_ratio, self.splitter.vertical)
            != (state.timeline_ratio, state.vertical);
        self.splitter = state;
        if relayout {
            self.resize(self.size.0 as u32, self.size.1 as u32);
        }
    }

    /// 表示の倍率（DPI / 96）。Timeline の寸法と描き込みの太さを論理ピクセルで揃える。
    /// 表示の倍率の違うモニターへ動かしたときは、物理ピクセルの大きさが同じでも割り付けを計算し直す。
    pub fn set_scale_factor(&mut self, factor: f32) {
        let scale = Scale::new(factor);
        if scale != self.scale {
            self.scale = scale;
            self.resize(self.size.0 as u32, self.size.1 as u32);
        }
    }

    /// 現在の割り付け（物理ピクセル）。
    pub fn layout(&self) -> SurfaceLayout {
        self.scale.layout_to_physical(self.layout)
    }

    /// サーフェスの座標（物理ピクセル）の点がどの領域か。
    pub fn region_at(&self, x: f32, y: f32) -> Region {
        let (x, y) = (self.scale.to_logical(x), self.scale.to_logical(y));
        let l = self.layout;
        let in_source = |r: Option<Rect>| r.is_some_and(|r| r.contains(x, y));
        if l.timeline.contains(x, y) {
            Region::Timeline
        } else if in_source(l.source) || in_source(l.source_transport) {
            Region::Source
        } else if l.program.contains(x, y) || l.program_transport.contains(x, y) {
            Region::Program
        } else {
            Region::Outside
        }
    }

    /// 仕切りの矩形（物理ピクセル）。ポインターが乗っているものはアクセントの色、普段は枠の色。
    pub(super) fn splitter_quads(&self) -> Vec<Quad> {
        let (w, h) = (
            self.scale.to_logical(self.size.0),
            self.scale.to_logical(self.size.1),
        );
        let s = self.splitter;
        let color = |which| {
            if s.hot == Some(which) {
                self.palette.accent
            } else {
                palette::BORDER
            }
        };
        let mut rects = vec![(
            splitter_rect(content_width(w, s.vertical), h, s.timeline_ratio),
            color(Splitter::Horizontal),
        )];
        if s.vertical {
            rects.push((vertical_splitter_rect(w, h), color(Splitter::Vertical)));
        }
        rects
            .into_iter()
            .map(|(r, c)| Quad::new(self.scale.rect_to_physical(r), c))
            .collect()
    }
}
