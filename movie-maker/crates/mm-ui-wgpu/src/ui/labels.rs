//! `Ui` の文字。Timeline、Program と Source、再生の操作のバーの文字を置き、文字の描画に渡す。
//! 再生位置のタイムコード（見出しの列の上の角と再生の操作のバー）は再生中に毎フレーム変わるので、ほかの文字と分けて渡し、
//! 文字の描画がその頂点だけを作り直す。

use mm_ui_layout::{
    clear_tooltip_labels, drag_tooltip, pending_badge, status_labels, timeline_labels,
    timeline_timecode, transport_labels, transport_timecodes, Label, Quad, Rect,
};

use super::Ui;
use crate::fonts::Shaper;
use crate::text::TextLayer;

impl Ui {
    /// 文字を置き、文字の描画に渡す。文字の描画は最初に描くときに作る（OS のフォントを読み込む）。
    /// ドラッグ中のツールチップの矩形は文字を測って決まるので、ここで置いて返す（Timeline の座標）。
    pub(super) fn prepare_text(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<Quad> {
        let _span = tracing::info_span!("ui.text").entered();
        let (format, family) = (self.format, &self.font.family);
        let mut text = self
            .text
            .take()
            .unwrap_or_else(|| TextLayer::new(device, queue, format, family));
        let (labels, timecodes, tooltip) = self.place_text(text.measure());
        let viewport = (self.size.0 as u32, self.size.1 as u32);
        text.prepare(device, queue, (&labels, &timecodes), self.scale, viewport);
        self.text = Some(text);
        tooltip
    }

    /// ほかの文字とタイムコードを置き、ドラッグ中のツールチップの矩形を返す。素材を開いている途中の印の矩形も決める。
    fn place_text(&mut self, measure: &mut Shaper) -> (Vec<Label>, Vec<Label>, Vec<Quad>) {
        let t = self.layout.timeline;
        let state = self.paint_state();
        self.hint.shown = self.status_strings();
        let vp = self.controller.viewport();
        let (model, font) = (&self.model, &self.font);
        let mut labels = timeline_labels(model, &vp, &state, font, measure);
        let corner = timeline_timecode(model, &vp, &state, font, measure);
        let mut timecodes: Vec<_> = corner.into_iter().collect();
        labels.extend(self.empty.timeline(&vp, &state, font, measure));
        let (timeline, palette) = (&self.controller, &self.palette);
        labels.extend(self.toolbar.labels(timeline, palette, font, measure));
        let (shown, bar) = (&self.hint.shown, timeline.layout().status);
        let status = (shown.0.as_str(), shown.1.as_str());
        labels.extend(status_labels(bar, status, palette, font, measure));
        let tooltip = drag_tooltip(model, &vp, &state, font, measure, &mut labels);
        clear_tooltip_labels(&mut timecodes, &tooltip, measure);
        for l in labels.iter_mut().chain(&mut timecodes) {
            l.x += t.x;
            l.y += t.y;
            l.clip = l.clip.translated(t.x, t.y);
        }
        let program = self.layout.program;
        labels.extend(self.empty.program(program, &state, font, measure));
        let badges: Vec<(Label, Quad)> = (self.pending_areas().into_iter())
            .filter_map(|area| pending_badge(area, palette, font, measure))
            .collect();
        self.pending_quads = badges.iter().map(|(_, quad)| *quad).collect();
        labels.extend(badges.into_iter().map(|(label, _)| label));
        let (bars, buttons) = (&self.transport, &self.transport_controller);
        labels.extend(transport_labels(
            &self.layout,
            bars,
            buttons,
            palette,
            font,
            measure,
        ));
        timecodes.extend(transport_timecodes(
            &self.layout,
            bars,
            palette,
            font,
            measure,
        ));
        (labels, timecodes, tooltip)
    }

    /// 素材を開いている途中のレイヤーを除いたフレームを出しているビューアーの矩形（Program、Source の順）。
    fn pending_areas(&self) -> Vec<Rect> {
        let program = self.program_pending.then_some(self.layout.program);
        let source = self.layout.source.filter(|_| self.source_pending);
        program.into_iter().chain(source).collect()
    }
}
