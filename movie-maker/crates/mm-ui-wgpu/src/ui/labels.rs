//! `Ui` の文字。Timeline、Program、再生の操作のバーの文字を置き、文字の描画に渡す。
//! 再生位置のタイムコード（見出しの列の上の角と再生の操作のバー）は再生中に毎フレーム変わるので、ほかの文字と分けて渡し、
//! 文字の描画がその頂点だけを作り直す。

use mm_ui_layout::{
    clear_tooltip_labels, drag_tooltip, status_labels, timeline_labels, timeline_timecode,
    transport_labels, transport_timecodes, Quad,
};

use super::Ui;
use crate::text::TextLayer;

impl Ui {
    /// 文字を置き、文字の描画に渡す。文字の描画は最初に描くときに作る（OS のフォントを読み込む）。
    /// ドラッグ中のツールチップの矩形は文字を測って決まるので、ここで置いて返す（Timeline の座標）。
    pub(super) fn prepare_text(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<Quad> {
        let _span = tracing::info_span!("ui.text").entered();
        let t = self.layout.timeline;
        let state = self.paint_state();
        self.hint.shown = self.status_strings();
        let (format, family) = (self.format, &self.font.family);
        let text = self
            .text
            .get_or_insert_with(|| TextLayer::new(device, queue, format, family));
        let vp = self.controller.viewport();
        let (model, font) = (&self.model, &self.font);
        let mut labels = timeline_labels(model, &vp, &state, font, text.measure());
        let corner = timeline_timecode(model, &vp, &state, font, text.measure());
        let mut timecodes: Vec<_> = corner.into_iter().collect();
        labels.extend(self.empty.timeline(&vp, &state, font, text.measure()));
        let (timeline, palette) = (&self.controller, &self.palette);
        labels.extend(self.toolbar.labels(timeline, palette, font, text.measure()));
        let (shown, bar) = (&self.hint.shown, timeline.layout().status);
        let status = (shown.0.as_str(), shown.1.as_str());
        labels.extend(status_labels(bar, status, palette, font, text.measure()));
        let tooltip = drag_tooltip(model, &vp, &state, font, text.measure(), &mut labels);
        clear_tooltip_labels(&mut timecodes, &tooltip, text.measure());
        for l in labels.iter_mut().chain(&mut timecodes) {
            l.x += t.x;
            l.y += t.y;
            l.clip = l.clip.translated(t.x, t.y);
        }
        let program = self.layout.program;
        labels.extend(self.empty.program(program, &state, font, text.measure()));
        let (bars, buttons) = (&self.transport, &self.transport_controller);
        let measure = text.measure();
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
        let viewport = (self.size.0 as u32, self.size.1 as u32);
        text.prepare(device, queue, (&labels, &timecodes), self.scale, viewport);
        tooltip
    }
}
