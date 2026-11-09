//! サーフェスの UI の文字を描く（glyphon）。`mm-ui-layout` が置いた文字（[`Label`]）を、[`Shaper`] が組んだ結果から描く。
//! 文字列と置き場所と色が前のフレームと同じなら、glyphon の頂点を作り直さず前のものを描く
//! （再生中は Timeline の文字が動かないので、グリフを毎フレーム引き直さない）。

use std::sync::Arc;

use glyphon::{
    Buffer, Cache, Color as GlyphColor, ColorMode, PrepareError, Resolution, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Viewport,
};
use mm_ui_layout::{Color, Label, Rect, Scale, TextPlacement, ELLIPSIS};

use crate::fonts::{ShapeCache, Shaper};

/// 描く文字 1 つ（物理ピクセルの置き場所）。前のフレームと比べて、頂点を作り直すかを決める。
#[derive(Debug, Clone, PartialEq)]
struct Placed {
    text: Arc<str>,
    size: f32,
    color: Color,
    at: TextPlacement,
}

/// 文字の層の数（計装と試験に使う）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextStats {
    /// 字を組んだ回数。同じ文字列は組み直さないので、表示する文字列が変わらない間は増えない。
    pub shaped: u64,
    /// glyphon の頂点を作った回数。文字の置き場所と色が変わらない間は増えない。
    pub uploads: u64,
}

/// 頂点を作ったときの入力。
#[derive(Debug, Clone, PartialEq)]
struct Prepared {
    labels: Vec<Placed>,
    /// 字を描いてよい矩形（物理ピクセルの左、上、右、下）。
    clip: [i32; 4],
    viewport: (u32, u32),
}

/// サーフェスの文字の層。字を組んで覚え、glyphon の atlas と頂点で描く。
pub(crate) struct TextLayer {
    shaper: Shaper,
    swash: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: Viewport,
    /// 今の頂点を作った入力。`None` は描くものが無い（作っていないか、作るのに失敗した）。
    prepared: Option<Prepared>,
    /// 頂点を作った回数（作り直していないことを確かめる）。
    uploads: u64,
}

impl TextLayer {
    /// `format` はサーフェスのフォーマット、`family` は書体の名前。
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        family: &str,
    ) -> Self {
        let cache = Cache::new(device);
        // サーフェスは sRGB でないフォーマットに表示の値を書くので、色を変換せずに混ぜる（矩形と同じ混ぜ方）。
        let mut atlas = TextAtlas::with_color_mode(device, queue, &cache, format, ColorMode::Web);
        let renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        Self {
            shaper: Shaper::new(family),
            swash: SwashCache::new(),
            atlas,
            renderer,
            viewport: Viewport::new(device, &cache),
            prepared: None,
            uploads: 0,
        }
    }

    /// 字の幅を測るもの。`mm-ui-layout` が文字を置くときに使う。
    pub(crate) fn measure(&mut self) -> &mut Shaper {
        &mut self.shaper
    }

    /// 書体を替える。
    pub(crate) fn set_family(&mut self, family: &str) {
        self.shaper.set_family(family);
        self.prepared = None;
    }

    /// 組んだ回数と頂点を作った回数。
    pub(crate) fn stats(&self) -> TextStats {
        TextStats {
            shaped: self.shaper.cache().count(),
            uploads: self.uploads,
        }
    }

    /// 文字を物理ピクセルに置き、前のフレームと違えば頂点を作り直す。`clip` は字を描いてよい矩形（物理ピクセル）、
    /// `viewport` はサーフェスの大きさ。パスの外（`submit` の前）で 1 フレームに 1 回呼ぶ。
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        labels: &[Label],
        scale: Scale,
        clip: Rect,
        viewport: (u32, u32),
    ) {
        let next = Prepared {
            labels: labels.iter().map(|l| placed(l, scale)).collect(),
            clip: pixel_bounds(clip),
            viewport,
        };
        if self.prepared.as_ref() != Some(&next) {
            let resolution = Resolution {
                width: viewport.0,
                height: viewport.1,
            };
            self.viewport.update(queue, resolution);
            let done = self.upload(device, queue, &next).is_ok();
            // 使ったグリフの印を外し、次に頂点を作るときに atlas から追い出せるようにする。
            self.atlas.trim();
            self.uploads += 1;
            self.prepared = done.then_some(next);
        }
        self.shaper.end_frame();
    }

    /// glyphon の頂点を作る。
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        next: &Prepared,
    ) -> Result<(), PrepareError> {
        let (fonts, cache) = self.shaper.split();
        let areas = text_areas(cache, next);
        self.renderer.prepare(
            device,
            queue,
            fonts,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash,
        )
    }

    /// 作った頂点で描く。
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.prepared.is_some() {
            // glyphon 0.12 の render は失敗を返さない（頂点はサーフェスの大きさが変わると作り直す）。
            let _ = self.renderer.render(&self.atlas, &self.viewport, pass);
        }
    }
}

fn placed(label: &Label, scale: Scale) -> Placed {
    Placed {
        text: label.text.clone(),
        size: label.size,
        color: label.color,
        at: scale.label_to_physical(label),
    }
}

/// 文字ごとの glyphon の描く範囲。切り詰めた文字は右端で切り、そこから「…」を描く。
fn text_areas<'a>(cache: &'a ShapeCache, next: &Prepared) -> Vec<TextArea<'a>> {
    let [left, top, right, bottom] = next.clip;
    let bounds = TextBounds {
        left,
        top,
        right,
        bottom,
    };
    let mut areas = Vec::with_capacity(next.labels.len() + 8);
    for p in &next.labels {
        let Some(shaped) = cache.get(&p.text, p.size) else {
            continue;
        };
        let cut = p.at.cut.map(|c| c as i32);
        let right = cut.map_or(bounds.right, |c| c.min(bounds.right));
        let clipped = TextBounds { right, ..bounds };
        areas.push(area(&shaped.buffer, p, p.at.x, clipped));
        if let (Some(c), Some(dots)) = (cut, cache.get(ELLIPSIS, p.size)) {
            areas.push(area(&dots.buffer, p, c as f32, bounds));
        }
    }
    areas
}

/// 組んだ `buffer` を、文字 `p` の行に左端 `left` から描く範囲。
fn area<'a>(buffer: &'a Buffer, p: &Placed, left: f32, bounds: TextBounds) -> TextArea<'a> {
    TextArea {
        buffer,
        left,
        top: p.at.y,
        scale: p.at.scale,
        bounds,
        default_color: glyph_color(p.color),
        custom_glyphs: &[],
    }
}

/// 物理ピクセルの矩形を、それを覆う整数の画素の範囲にする。
fn pixel_bounds(r: Rect) -> [i32; 4] {
    [
        r.x.floor() as i32,
        r.y.floor() as i32,
        r.right().ceil() as i32,
        r.bottom().ceil() as i32,
    ]
}

/// 表示の値の色を glyphon の 8 bit の色にする。
fn glyph_color(c: Color) -> GlyphColor {
    let [r, g, b, a] = c.0.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    GlyphColor::rgba(r, g, b, a)
}
