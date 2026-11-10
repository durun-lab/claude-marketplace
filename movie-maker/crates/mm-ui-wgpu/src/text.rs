//! サーフェスの UI の文字を描く（glyphon）。`mm-ui-layout` が置いた文字（[`Label`]）を、[`Shaper`] のシェーピングの結果から描く。
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
    tabular: bool,
}

/// 文字の描画の数（計装と試験に使う）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextStats {
    /// シェーピングした回数。同じ文字列はシェーピングし直さないので、表示する文字列が変わらない間は増えない。
    pub shaped: u64,
    /// glyphon の頂点を作った回数。文字の置き場所と色が変わらない間は増えない。
    pub uploads: u64,
}

/// 頂点を作ったときの入力。
#[derive(Debug, Clone, PartialEq)]
struct Prepared {
    labels: Vec<Placed>,
    viewport: (u32, u32),
}

/// サーフェスの文字の描画。字をシェーピングしてキャッシュし、glyphon のアトラスと頂点で描く。
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
    /// `format` はサーフェスのフォーマット、`family` はフォントの名前。
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

    /// フォントを替える。
    pub(crate) fn set_family(&mut self, family: &str) {
        self.shaper.set_family(family);
        self.prepared = None;
    }

    /// シェーピングした回数と頂点を作った回数。
    pub(crate) fn stats(&self) -> TextStats {
        TextStats {
            shaped: self.shaper.cache().count(),
            uploads: self.uploads,
        }
    }

    /// 文字を物理ピクセルに置き、前のフレームと違えば頂点を作り直す。字は文字ごとの [`Label::clip`] の外に描かない。
    /// `viewport` はサーフェスの大きさ。パスの外（`submit` の前）で 1 フレームに 1 回呼ぶ。
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        labels: &[Label],
        scale: Scale,
        viewport: (u32, u32),
    ) {
        let next = Prepared {
            labels: labels.iter().map(|l| placed(l, scale)).collect(),
            viewport,
        };
        if self.prepared.as_ref() != Some(&next) {
            let resolution = Resolution {
                width: viewport.0,
                height: viewport.1,
            };
            self.viewport.update(queue, resolution);
            let done = self.upload(device, queue, &next).is_ok();
            // 使ったグリフを使用中から外し、次に頂点を作るときにアトラスから追い出せるようにする。
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
        tabular: label.tabular,
    }
}

/// 文字ごとの glyphon の描く範囲。切り詰めた文字は右端で切り、そこから「…」を描く。
fn text_areas<'a>(cache: &'a ShapeCache, next: &Prepared) -> Vec<TextArea<'a>> {
    let mut areas = Vec::with_capacity(next.labels.len() + 8);
    for p in &next.labels {
        let bounds = pixel_bounds(p.at.clip);
        let cut = p.at.cut.map(|c| c as i32);
        let right = cut.map_or(bounds.right, |c| c.min(bounds.right));
        let clipped = TextBounds { right, ..bounds };
        if p.tabular {
            push_tabular(&mut areas, cache, p, clipped);
        } else if let Some(shaped) = cache.get(&p.text, p.size) {
            areas.push(area(&shaped.buffer, p, p.at.x, clipped));
        } else {
            continue;
        }
        if let (Some(c), Some(dots)) = (cut, cache.get(ELLIPSIS, p.size)) {
            areas.push(area(&dots.buffer, p, c as f32, bounds));
        }
    }
    areas
}

/// 時刻の字を、覚えた 1 字ずつのグリフで左から並べる。数字は一番広い数字の幅で進める（置き場所は `tabular_stops` と同じ）。
fn push_tabular<'a>(
    areas: &mut Vec<TextArea<'a>>,
    cache: &'a ShapeCache,
    p: &Placed,
    bounds: TextBounds,
) {
    let mut left = p.at.x;
    for c in p.text.chars() {
        let Some(advance) = cache.advance(c, p.size) else {
            return;
        };
        let glyph = cache.get(c.encode_utf8(&mut [0; 4]), p.size);
        if let (Some(shaped), false) = (glyph, c.is_whitespace()) {
            // 狭い数字（1 など）は幅をそろえた升の中央に置く。
            let inset = (advance - shaped.advance()) / 2.0 * p.at.scale;
            areas.push(area(&shaped.buffer, p, left + inset, bounds));
        }
        left += advance * p.at.scale;
    }
}

/// シェーピングした `buffer` を、文字 `p` の行に左端 `left` から描く範囲。
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
fn pixel_bounds(r: Rect) -> TextBounds {
    TextBounds {
        left: r.x.floor() as i32,
        top: r.y.floor() as i32,
        right: r.right().ceil() as i32,
        bottom: r.bottom().ceil() as i32,
    }
}

/// 表示の値の色を glyphon の 8 bit の色にする。
fn glyph_color(c: Color) -> GlyphColor {
    let [r, g, b, a] = c.0.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    GlyphColor::rgba(r, g, b, a)
}
