//! サーフェスの UI の文字を描く（glyphon）。`mm-ui-layout` が置いた文字（[`Label`]）を、[`Shaper`] のシェーピングの結果から描く。
//! 再生位置のタイムコードとほかの文字は、glyphon の頂点とアトラスを別のバッチ（[`Batch`]）に分けて持つ。バッチごとに、
//! 文字列と置き場所と色が前のフレームと同じなら頂点を作り直さず前のものを描くので、再生中はタイムコードのバッチだけ
//! 頂点を作り直す（Timeline の文字は動かない）。タイムコードは 1 字ずつキャッシュしたシェーピングの結果を並べて描く。

use std::sync::Arc;

use glyphon::{
    Buffer, Cache, Color as GlyphColor, ColorMode, FontSystem, Resolution, SwashCache, TextArea,
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

/// 文字の描画の数（計装と試験に使う）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextStats {
    /// シェーピングした回数。同じ文字列はシェーピングし直さず、タイムコードは 1 字ずつのシェーピングの結果を並べるので、
    /// 表示する文字列が変わらない間と再生中は増えない。
    pub shaped: u64,
    /// 再生位置のタイムコードのほかの文字の、glyphon の頂点を作った回数。置き場所と色が変わらない間は増えない
    /// （再生中も増えない）。
    pub uploads: u64,
    /// 再生位置のタイムコードの頂点を作った回数。再生中は毎フレーム増える。
    pub timecode_uploads: u64,
}

/// 頂点を作ったときの入力。
#[derive(Debug, Clone, PartialEq)]
struct Prepared {
    labels: Vec<Placed>,
    viewport: (u32, u32),
}

/// 文字 1 つの本体の描く範囲を `areas` に足す関数。シェーピングの結果がキャッシュに無ければ足さずに `false`。
type Body = for<'a> fn(&mut Vec<TextArea<'a>>, &'a ShapeCache, &Placed, TextBounds) -> bool;

/// glyphon の頂点とアトラスの一組（バッチ）。変わったバッチだけ頂点を作り直す。アトラスもバッチごとに持つので、
/// 片方の頂点を作るときに、もう片方の頂点が指すグリフをアトラスから追い出さない。
struct Batch {
    atlas: TextAtlas,
    renderer: TextRenderer,
    /// 文字 1 つの本体の描き方。
    body: Body,
    /// 今の頂点を作った入力。`None` は描くものが無い（作っていないか、作るのに失敗した）。
    prepared: Option<Prepared>,
    /// 頂点を作った回数（作り直していないことを確かめる）。
    uploads: u64,
}

/// 頂点を作るときに使う、バッチの外のもの。
struct Upload<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    fonts: &'a mut FontSystem,
    cache: &'a ShapeCache,
    swash: &'a mut SwashCache,
    viewport: &'a Viewport,
}

impl Batch {
    /// `cache` は glyphon のパイプラインのキャッシュ（バッチで共有する）、`body` は文字 1 つの本体の描き方。
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &Cache,
        format: wgpu::TextureFormat,
        body: Body,
    ) -> Self {
        // サーフェスは sRGB でないフォーマットに表示の値を書くので、色を変換せずに混ぜる（矩形と同じ混ぜ方）。
        let mut atlas = TextAtlas::with_color_mode(device, queue, cache, format, ColorMode::Web);
        let renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        Self {
            atlas,
            renderer,
            body,
            prepared: None,
            uploads: 0,
        }
    }

    /// 置いた文字 `next` が、今の頂点を作った入力と違えば頂点を作り直す。
    fn prepare(&mut self, up: &mut Upload<'_>, next: Prepared) {
        if self.prepared.as_ref() == Some(&next) {
            return;
        }
        let areas = text_areas(up.cache, &next, self.body);
        let done = self
            .renderer
            .prepare(
                up.device,
                up.queue,
                up.fonts,
                &mut self.atlas,
                up.viewport,
                areas,
                up.swash,
            )
            .is_ok();
        // 使ったグリフを使用中から外し、次にこのバッチの頂点を作るときにアトラスから追い出せるようにする。
        self.atlas.trim();
        self.uploads += 1;
        self.prepared = done.then_some(next);
    }

    /// 作った頂点で描く。
    fn draw(&self, viewport: &Viewport, pass: &mut wgpu::RenderPass<'_>) {
        if self.prepared.is_some() {
            // glyphon 0.12 の render は失敗を返さない（頂点はサーフェスの大きさが変わると作り直す）。
            let _ = self.renderer.render(&self.atlas, viewport, pass);
        }
    }
}

/// サーフェスの文字の描画。字をシェーピングしてキャッシュし、glyphon のアトラスと頂点で描く。
pub(crate) struct TextLayer {
    shaper: Shaper,
    swash: SwashCache,
    viewport: Viewport,
    /// 再生位置のタイムコードのほかの文字。文字列ごとのシェーピングの結果を描く。
    labels: Batch,
    /// 再生位置のタイムコード（見出しの列の上の角と再生の操作のバー）。1 字ずつのシェーピングの結果を並べて描く。
    timecodes: Batch,
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
        Self {
            shaper: Shaper::new(family),
            swash: SwashCache::new(),
            viewport: Viewport::new(device, &cache),
            labels: Batch::new(device, queue, &cache, format, whole),
            timecodes: Batch::new(device, queue, &cache, format, each_glyph),
        }
    }

    /// 字の幅を測るもの。`mm-ui-layout` が文字を置くときに使う。
    pub(crate) fn measure(&mut self) -> &mut Shaper {
        &mut self.shaper
    }

    /// フォントを替える。
    pub(crate) fn set_family(&mut self, family: &str) {
        self.shaper.set_family(family);
        self.labels.prepared = None;
        self.timecodes.prepared = None;
    }

    /// シェーピングした回数と頂点を作った回数。
    pub(crate) fn stats(&self) -> TextStats {
        TextStats {
            shaped: self.shaper.cache().count(),
            uploads: self.labels.uploads,
            timecode_uploads: self.timecodes.uploads,
        }
    }

    /// 文字を物理ピクセルに置き、前のフレームと違うバッチだけ頂点を作り直す。`labels` は再生位置のタイムコードのほかの
    /// 文字、`timecodes` は再生位置のタイムコード。字は文字ごとの [`Label::clip`] の外に描かない。
    /// `viewport` はサーフェスの大きさ。パスの外（`submit` の前）で 1 フレームに 1 回呼ぶ。
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        (labels, timecodes): (&[Label], &[Label]),
        scale: Scale,
        viewport: (u32, u32),
    ) {
        let resolution = Resolution {
            width: viewport.0,
            height: viewport.1,
        };
        self.viewport.update(queue, resolution);
        let placed = |labels: &[Label]| Prepared {
            labels: labels.iter().map(|l| placed(l, scale)).collect(),
            viewport,
        };
        let (fonts, cache) = self.shaper.split();
        let mut up = Upload {
            device,
            queue,
            fonts,
            cache,
            swash: &mut self.swash,
            viewport: &self.viewport,
        };
        self.labels.prepare(&mut up, placed(labels));
        self.timecodes.prepare(&mut up, placed(timecodes));
        self.shaper.end_frame();
    }

    /// 作った頂点で描く。
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.labels.draw(&self.viewport, pass);
        self.timecodes.draw(&self.viewport, pass);
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

/// 文字ごとの glyphon の描く範囲。本体は `body` で足し、切り詰めた文字は右端で切って、そこから「…」を描く。
fn text_areas<'a>(cache: &'a ShapeCache, next: &Prepared, body: Body) -> Vec<TextArea<'a>> {
    let mut areas = Vec::with_capacity(next.labels.len() + 8);
    for p in &next.labels {
        let bounds = pixel_bounds(p.at.clip);
        let cut = p.at.cut.map(|c| c as i32);
        let right = cut.map_or(bounds.right, |c| c.min(bounds.right));
        if !body(&mut areas, cache, p, TextBounds { right, ..bounds }) {
            continue;
        }
        if let (Some(c), Some(dots)) = (cut, cache.get(ELLIPSIS, p.size)) {
            areas.push(area(&dots.buffer, p, c as f32, bounds));
        }
    }
    areas
}

/// 文字列ごとのシェーピングの結果を、文字の左端から描く。
fn whole<'a>(
    areas: &mut Vec<TextArea<'a>>,
    cache: &'a ShapeCache,
    p: &Placed,
    bounds: TextBounds,
) -> bool {
    let Some(shaped) = cache.get(&p.text, p.size) else {
        return false;
    };
    areas.push(area(&shaped.buffer, p, p.at.x, bounds));
    true
}

/// 再生位置のタイムコードを、1 字ずつキャッシュしたシェーピングの結果で左から並べる。数字は一番広い数字の送り幅で
/// 進め、狭い数字（1 など）はその幅の中央に置く（置き場所は `Shaper` の `timecode_stops` と同じ）。
fn each_glyph<'a>(
    areas: &mut Vec<TextArea<'a>>,
    cache: &'a ShapeCache,
    p: &Placed,
    bounds: TextBounds,
) -> bool {
    let start = areas.len();
    // 左端からの距離（論理ピクセル）。
    let mut x = 0.0_f32;
    for c in p.text.chars() {
        let glyph = cache.get(c.encode_utf8(&mut [0; 4]), p.size);
        let (Some(advance), Some(glyph)) = (cache.timecode_advance(c, p.size), glyph) else {
            areas.truncate(start);
            return false;
        };
        if !c.is_whitespace() {
            // 文字列ごとに描くときに glyphon がグリフに掛けるのと同じ式で物理ピクセルにし、数字の幅がそろった
            // フォントでは、文字列ごとに描いたときと同じサブピクセルの位置に置く。
            let inset = (advance - glyph.advance()) / 2.0;
            let left = (x + inset).mul_add(p.at.scale, p.at.x);
            areas.push(area(&glyph.buffer, p, left, bounds));
        }
        x += advance;
    }
    true
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

#[cfg(test)]
mod tests;
