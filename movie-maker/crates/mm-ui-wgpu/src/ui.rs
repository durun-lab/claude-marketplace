//! サーフェス 1 枚分の UI。上に Program（素材を開いていれば左に Source）、下に Timeline を描き、Timeline のポインター操作を受ける。
//!
//! 外とやりとりする座標と大きさは物理ピクセル、割り付けと Timeline の寸法は論理ピクセルで持つ。
//! 換算は描く所（[`Ui::render`]）と入力を受ける所（[`Ui::pointer`]、[`Ui::region_at`]）で [`Scale`] を 1 回だけ通す。
//! Timeline は矩形（背景、帯、目盛り、ドラッグ中のツールチップの矩形。帯までの矩形の後に帯のサムネイルを挟む）、文字（目盛りの時刻、帯の名前、ツールチップの量）、
//! 矩形（再生位置の線、スナップの表示。ツールチップの矩形の上は除く）の順に重ねる。
//! Timeline の表示の状態（ズーム、スクロール、行の高さ、スナップの入/切）は [`Ui::view_state`] で出し、[`Ui::restore_view`] で戻す。
//! キーとメニューの Timeline のコマンドも、ドラッグとの前後を保つため [`Ui::pointer`] で受ける。

use mm_types::RgbaImage;
use mm_ui_layout::palette::{self, Palette};
use mm_ui_layout::{
    clear_tooltip_area, drag_tooltip, fit_rect, split_viewers, timeline_labels, timeline_overlay,
    timeline_quads, transport_labels, Action, DrawingShape, EmptyHints, EmptyKeys, EmptyState,
    PaintInput, PaintState, Peaks, PointerEvent, Quad, Rect, RowHeights, Scale, SurfaceLayout,
    TimeView, TimelineController, TimelineModel, TransportController, TransportState, UiFont,
    UnderText, ViewState, Waves,
};

use crate::drawing::{DrawingFit, DrawingLayer};
use crate::program::ProgramViewer;
use crate::quads::QuadRenderer;
use crate::text::{TextLayer, TextStats};
use crate::thumbs::ThumbRenderer;

mod thumbs;
mod toolbar;
mod transport;
mod view;

use thumbs::DroppedThumbs;
pub use thumbs::ThumbStats;

/// サーフェスの上のどの領域か。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Source,
    Program,
    Timeline,
    Outside,
}

/// サーフェスの UI。描画の資源と、Timeline の表示・操作の状態を持つ。
pub struct Ui {
    quads: QuadRenderer,
    /// 文字の描画。OS のフォントの読み込みを最初に描くときまで遅らせる。
    text: Option<TextLayer>,
    /// サーフェスのフォーマット（文字の描画を作るときに使う）。
    format: wgpu::TextureFormat,
    font: UiFont,
    program: ProgramViewer,
    /// 帯の先頭のサムネイル。
    thumbs: ThumbRenderer,
    /// 読めなかったので帯から外したサムネイルのキー。
    dropped_thumbs: DroppedThumbs,
    /// Program の上に重ねる描き込み。
    drawing: DrawingLayer,
    /// 表示の倍率（DPI / 96）。
    scale: Scale,
    source: ProgramViewer,
    model: TimelineModel,
    controller: TimelineController,
    /// 行の高さ（表示の状態）。
    heights: RowHeights,
    /// 開いたときに戻す表示の状態。最初に Timeline の大きさが決まったときに当てる。
    restore: Option<ViewState>,
    /// 割り付け（論理ピクセル）。
    layout: SurfaceLayout,
    /// 再生の操作のバーの状態。Program の再生位置、長さ、フレームレートは Timeline のモデルと再生位置から埋める。
    transport: TransportState,
    transport_controller: TransportController,
    toolbar: toolbar::Toolbar,
    /// サーフェスの大きさ（物理ピクセル）。
    size: (f32, f32),
    playhead: f64,
    /// 最初の表示（戻した表示の状態か、全体表示と sequence の行）を当てたか。
    opened: bool,
    palette: Palette,
    /// 空の状態の案内（どれを出すかと、案内に出すキー）。
    empty: EmptyHints,
    /// 帯の波形のピーク（読み込みが終わった素材から）。
    waves: Waves,
}

impl Ui {
    /// `format` はサーフェスのフォーマット。
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            quads: QuadRenderer::new(device, format),
            text: None,
            format,
            font: UiFont::default(),
            program: ProgramViewer::new(device, format),
            thumbs: ThumbRenderer::new(format),
            dropped_thumbs: DroppedThumbs::default(),
            drawing: DrawingLayer::new(device, format),
            scale: Scale::ONE,
            source: ProgramViewer::new(device, format),
            model: TimelineModel::default(),
            controller: TimelineController::new(TimeView {
                origin: 0.0,
                px_per_sec: 100.0,
            }),
            heights: RowHeights::default(),
            restore: None,
            layout: SurfaceLayout {
                program: Rect::default(),
                program_transport: Rect::default(),
                timeline: Rect::default(),
                source: None,
                source_transport: None,
            },
            transport: TransportState::default(),
            transport_controller: TransportController::default(),
            toolbar: toolbar::Toolbar::default(),
            size: (0.0, 0.0),
            playhead: 0.0,
            opened: false,
            palette: Palette::DEFAULT,
            empty: EmptyHints::default(),
            waves: Waves::default(),
        }
    }

    /// 空の状態の案内に出すキー（アプリのキーの表から作ったもの）。
    pub fn set_empty_keys(&mut self, keys: EmptyKeys) {
        self.empty.keys = keys;
    }

    /// 空の状態の案内のどれを出すか（プロジェクトのデータから決めたもの。[`crate::empty_state`]）。
    pub fn set_empty_state(&mut self, show: EmptyState) {
        self.empty.show = show;
    }

    /// Timeline の色（`ui/theme` の theme.json から作ったもの）。
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    /// Timeline の字（`ui/theme` の theme.json の `font` から作ったもの）。
    pub fn set_font(&mut self, font: UiFont) {
        if let Some(text) = &mut self.text {
            text.set_family(&font.family);
        }
        self.font = font;
    }

    /// 文字の描画の数（シェーピングした回数、頂点を作った回数）。まだ描いていなければ 0。
    pub fn text_stats(&self) -> TextStats {
        self.text
            .as_ref()
            .map_or_else(TextStats::default, TextLayer::stats)
    }

    /// サーフェスの大きさ（物理ピクセル）。割り付けを論理ピクセルで計算し直す。
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = (width as f32, height as f32);
        self.layout = split_viewers(
            self.scale.to_logical(self.size.0),
            self.scale.to_logical(self.size.1),
            self.source.has_image(),
        );
        let t = self.layout.timeline;
        self.controller.resize((t.w, t.h));
        if !self.opened {
            self.open_view();
        }
    }

    /// Timeline に出すもの。最初に渡されたときに全体表示にし、sequence の行が見える所までスクロールする。
    pub fn set_model(&mut self, model: TimelineModel) {
        self.model = model;
        self.drop_missing_thumbs();
        self.opened = false;
        self.relayout_rows();
    }

    /// 編集で並びや長さが変わったモデルに置き換える。拡大とスクロールは保ち、範囲の外の選択だけ外す。
    pub fn update_model(&mut self, model: TimelineModel) {
        self.controller.model_changed(&model);
        self.model = model;
        self.drop_missing_thumbs();
        self.relayout_rows();
    }

    /// コンテンツハッシュ `content_hash` の素材のピークを置く。置くと、その素材の帯に波形が出る。
    pub fn set_wave(&mut self, content_hash: std::sync::Arc<str>, peaks: Peaks) {
        self.waves.insert(content_hash, peaks);
    }

    /// 選択を置く（`None` で外す）。添字は `TimelineModel::items` のもの。
    pub fn select(&mut self, index: Option<usize>) {
        self.controller.selected = index.filter(|i| *i < self.model.items.len());
    }

    /// 選んだカットを置く（`None` で外す）。添字は `TimelineModel::items` とそのカットのもの。
    pub fn select_cut(&mut self, cut: Option<(usize, usize)>) {
        self.controller.selected_cut = cut.filter(|(i, c)| {
            self.model
                .items
                .get(*i)
                .is_some_and(|item| *c < item.cuts.len())
        });
    }

    /// 再生位置（秒）。再生とキーで行の領域の端を越えたら、その向きへ 1 画面ずつ送る（`TimelineController::page_scroll`）。
    pub fn set_playhead(&mut self, secs: f64) {
        self.controller.page_scroll(self.playhead, secs);
        self.playhead = secs;
    }

    /// Program と Source のトーンマップの素材の上限（線形、1.0 が SDR の白）。1 以下はトーンマップしない。
    pub fn set_hdr_peak(&mut self, peak: f32) {
        self.program.set_source_peak(peak);
        self.source.set_source_peak(peak);
    }

    /// Program に出すフレーム。
    pub fn set_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, image: &RgbaImage) {
        self.program.set_image(device, queue, image);
    }

    /// Source に出すフレーム。最初のフレームが来ると Source の領域が現れる。
    pub fn set_source_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &RgbaImage,
    ) {
        let had = self.source.has_image();
        self.source.set_image(device, queue, image);
        if had != self.source.has_image() {
            self.resize(self.size.0 as u32, self.size.1 as u32);
        }
    }

    /// Source を閉じる（領域を畳んで Program を広げる）。
    pub fn close_source(&mut self) {
        if self.source.has_image() {
            self.source.clear();
            self.resize(self.size.0 as u32, self.size.1 as u32);
        }
    }

    /// Program の映像に重ねる描き込み。空で消す。座標は映像の左上を (0, 0)、右下を (1, 1) に正規化したもの。
    pub fn set_drawing(&mut self, drawing: Vec<DrawingShape>) {
        self.drawing.set_shapes(drawing);
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

    /// Program に出すフレームを、同じデバイスのテクスチャのまま渡す。
    pub fn set_frame_texture(&mut self, device: &wgpu::Device, texture: &wgpu::Texture) {
        self.program.set_texture(device, texture);
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

    /// サーフェスの座標（物理ピクセル）のポインターの入力を受ける。Timeline の外から始まったドラッグは捨てる。
    pub fn pointer(&mut self, ev: PointerEvent) -> Vec<Action> {
        let t = self.layout.timeline;
        let local = match self.scale.pointer_to_logical(ev) {
            PointerEvent::Down { x, y, .. } if !t.contains(x, y) => return Vec::new(),
            PointerEvent::Down { x, y, .. } if self.on_toolbar(x, y) => return Vec::new(),
            PointerEvent::Wheel { x, y, .. } if !t.contains(x, y) => return Vec::new(),
            PointerEvent::Down { x, y, mods } => PointerEvent::Down {
                x: x - t.x,
                y: y - t.y,
                mods,
            },
            PointerEvent::Move { x, y } => PointerEvent::Move {
                x: x - t.x,
                y: y - t.y,
            },
            PointerEvent::Wheel {
                x,
                y,
                dy,
                ctrl,
                shift,
            } => PointerEvent::Wheel {
                x: x - t.x,
                y: y - t.y,
                dy,
                ctrl,
                shift,
            },
            other => other,
        };
        self.controller.handle(local, &self.model, self.playhead)
    }

    /// 1 フレーム描く。`target` はサーフェスのテクスチャのビュー。
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
    ) {
        let counts = self.prepare(device, queue);
        let _submit = tracing::info_span!("ui.submit").entered();
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = begin_pass(&mut enc, target);
            self.program.draw(&mut pass);
            self.drawing.draw(&mut pass, counts.drawing);
            self.source.draw(&mut pass);
            self.quads.draw(&mut pass, 0..counts.bars);
            self.thumbs.draw(&mut pass, counts.thumbs);
            self.quads.draw(&mut pass, counts.bars..counts.under);
            if let Some(text) = &self.text {
                text.draw(&mut pass);
            }
            self.quads.draw(&mut pass, counts.under..counts.quads);
        }
        queue.submit([enc.finish()]);
    }

    /// 描くもの（矩形、文字、Program と Source の映像、描き込み）を GPU に渡し、パスで描く数を返す。
    fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Counts {
        let (w, h) = self.size;
        // 再生の操作のバーの文字と矩形が、同じ再生位置と長さを見る。
        self.sync_program();
        self.sync_toolbar();
        let tooltip = self.prepare_text(device, queue);
        let (quads, under, bars) = self.timeline_quads(tooltip);
        let count = self.quads.prepare(device, queue, &quads, (w, h));
        let layout = self.layout();
        let mut drawing = 0;
        if let Some((iw, ih)) = self.program.image_size() {
            let image = (iw as f32, ih as f32);
            let rect = fit_rect(image.0, image.1, layout.program);
            self.program.prepare(queue, rect, (w, h));
            let fit = DrawingFit {
                image,
                area: layout.program,
                viewport: (w, h),
                scale: self.scale.get(),
                color: self.palette.drawing,
            };
            drawing = self.drawing.prepare(device, queue, fit);
        }
        if let (Some((iw, ih)), Some(area)) = (self.source.image_size(), layout.source) {
            let rect = fit_rect(iw as f32, ih as f32, area);
            self.source.prepare(queue, rect, (w, h));
        }
        let thumbs = self.prepare_thumbs(device, queue);
        Counts {
            under,
            bars,
            thumbs,
            quads: count,
            drawing,
        }
    }

    /// 描く時点の Timeline の状態。
    fn paint_state(&self) -> PaintState {
        PaintState {
            palette: self.palette,
            ..self.controller.paint_state(self.playhead)
        }
    }

    /// Timeline の矩形を、サーフェスの座標（物理ピクセル）に置いた並びにする。文字の下に描く矩形が先で、
    /// その数と、そのうち帯までの数も返す（残りは文字の上に重ねる再生位置の線とスナップの表示）。
    /// `tooltip` はドラッグ中のツールチップの矩形で、文字の下に描く矩形のいちばん上に重ね、文字の上に重ねる矩形はその外だけに描く。
    fn timeline_quads(&self, tooltip: Vec<Quad>) -> (Vec<Quad>, u32, u32) {
        let t = self.layout.timeline;
        let state = self.paint_state();
        let vp = self.controller.viewport();
        let input = PaintInput {
            waves: Some(&self.waves),
            toolbar: Some(self.toolbar.view()),
            ..PaintInput::new(&self.model, &vp, &state)
        };
        let UnderText { mut quads, bars } = timeline_quads(&input);
        let overlay = clear_tooltip_area(timeline_overlay(&input), &tooltip);
        quads.extend(tooltip);
        let under = quads.len() as u32;
        quads.extend(overlay);
        for q in &mut quads {
            q.rect = self.scale.rect_to_physical(q.rect.translated(t.x, t.y));
        }
        // 再生の操作のバーの矩形も文字の下に描く。帯までの数もその分だけ後ろへずれる。
        let mut under_quads = self.transport_quads();
        let front = under_quads.len() as u32;
        under_quads.extend(quads);
        (under_quads, under + front, bars as u32 + front)
    }

    /// Timeline の文字を置き、文字の描画に渡す。文字の描画は最初に描くときに作る（OS のフォントを読み込む）。
    /// ドラッグ中のツールチップの矩形は文字を測って決まるので、ここで置いて返す（Timeline の座標）。
    fn prepare_text(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<Quad> {
        let _span = tracing::info_span!("ui.text").entered();
        let t = self.layout.timeline;
        let state = self.paint_state();
        let (format, family) = (self.format, &self.font.family);
        let text = self
            .text
            .get_or_insert_with(|| TextLayer::new(device, queue, format, family));
        let vp = self.controller.viewport();
        let (model, font) = (&self.model, &self.font);
        let mut labels = timeline_labels(model, &vp, &state, font, text.measure());
        labels.extend(self.empty.timeline(&vp, &state, font, text.measure()));
        let (timeline, palette) = (&self.controller, &self.palette);
        labels.extend(self.toolbar.labels(timeline, palette, font, text.measure()));
        let tooltip = drag_tooltip(model, &vp, &state, font, text.measure(), &mut labels);
        for l in &mut labels {
            l.x += t.x;
            l.y += t.y;
            l.clip = l.clip.translated(t.x, t.y);
        }
        labels.extend(
            self.empty
                .program(self.layout.program, &state, font, text.measure()),
        );
        labels.extend(transport_labels(
            &self.layout,
            &self.transport,
            &self.transport_controller,
            &self.palette,
            &self.font,
            text.measure(),
        ));
        let viewport = (self.size.0 as u32, self.size.1 as u32);
        text.prepare(device, queue, &labels, self.scale, viewport);
        tooltip
    }
}

/// 1 フレームのパスで描く数。
struct Counts {
    /// 文字の下に描く矩形の数。残りの矩形（`under..quads`）は文字の上に重ねる。
    under: u32,
    /// 帯までの矩形の数。サムネイルはその後ろに描く。
    bars: u32,
    /// 描くサムネイルの数。
    thumbs: u32,
    quads: u32,
    /// 描き込みの線分の数。
    drawing: u32,
}

/// サーフェス全体を Program の背景色で塗るパスを始める。
fn begin_pass<'a>(
    enc: &'a mut wgpu::CommandEncoder,
    target: &'a wgpu::TextureView,
) -> wgpu::RenderPass<'a> {
    let c = palette::PROGRAM_BACKDROP.0;
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("ui"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: f64::from(c[0]),
                    g: f64::from(c[1]),
                    b: f64::from(c[2]),
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
