//! ツールチップの中身。ボタンはコマンドの表の ID（名前、キー、説明はアプリが表から引く）、帯は種類、名前、範囲、長さ。
//! 先例は Premiere のツールのツールチップ（名前とキー）と、Resolve と Premiere のクリップのツールチップ。

use super::Under;
use crate::header::timecode;
use crate::timeline::{HeaderButton, Hit, ItemKind, TimelineModel};
use crate::toolbar::ToolbarPart;
use crate::transport::TransportPart;

/// ツールチップの中身。同じ値なら同じ対象とみなし、変わったら文と位置を差し替える。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TooltipContent {
    /// コマンドの表の行（「名前 (キー)」と、あれば 1 行の説明）。
    Command(&'static str),
    /// 表に無い部品の文（1 行目、あれば 2 行目）。
    Text(String, Option<String>),
}

/// ポインターの下 `under` のツールチップ。出さない部品は `None`。
pub fn tooltip_of(under: Under, model: &TimelineModel) -> Option<TooltipContent> {
    match under {
        Under::Toolbar(ToolbarPart::Button(id))
        | Under::Transport(_, TransportPart::Button(id)) => Some(TooltipContent::Command(id)),
        Under::Toolbar(ToolbarPart::Slider) => Some(TooltipContent::Text(
            "Zoom".into(),
            Some("Drag to zoom the Timeline around the playhead".into()),
        )),
        Under::Timeline(Hit::Layer { index, .. }) => bar_tooltip(model, index, None),
        Under::Timeline(Hit::Cut { item, cut, .. }) => bar_tooltip(model, item, Some(cut)),
        Under::Timeline(Hit::HeaderButton { button, .. }) => Some(header_tooltip(button)),
        _ => None,
    }
}

fn header_tooltip(button: HeaderButton) -> TooltipContent {
    let (title, detail) = match button {
        HeaderButton::Enabled => (
            "Enable Layer",
            "Show or hide the layer in the Program and the export",
        ),
        HeaderButton::Lock => ("Lock Layer", "Keep Claude's operations away from the layer"),
        HeaderButton::Mute => ("Mute Layer", "Silence the layer's audio"),
        HeaderButton::Solo => ("Solo Layer", "Play only the soloed layers' audio"),
    };
    TooltipContent::Text(title.into(), Some(detail.into()))
}

fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Footage => "Footage",
        ItemKind::Image => "Image",
        ItemKind::Solid => "Solid",
        ItemKind::Text => "Text",
        ItemKind::Composition => "Composition",
        ItemKind::Sequence => "Sequence",
        ItemKind::Other => "Layer",
    }
}

/// 長さ（秒）の「秒:フレーム」（秒は 59 を超えてもそのまま）。
fn length(secs: f64, fps: f64) -> String {
    let base = fps.round().max(1.0) as u64;
    let frames = (secs.max(0.0) * fps.max(0.0)).round() as u64;
    format!("{}:{:02}", frames / base, frames % base)
}

/// 帯のツールチップ「Text · Opening Title · 00:00:05:00–00:00:08:00 (3:00)」。`cut` があればそのカット（種類は Clip）。
/// 名前が空なら、レイヤーの ID。
pub fn bar_tooltip(
    model: &TimelineModel,
    item: usize,
    cut: Option<usize>,
) -> Option<TooltipContent> {
    let it = model.items.get(item)?;
    let (kind, label, start, end) = match cut {
        Some(c) => {
            let bar = it.cuts.get(c)?;
            ("Clip", &bar.label, bar.start, bar.end)
        }
        None => (kind_name(it.kind), &it.label, it.start, it.end),
    };
    let name: &str = if label.is_empty() { &it.id } else { label };
    let fps = model.fps;
    Some(TooltipContent::Text(
        format!(
            "{kind} \u{B7} {name} \u{B7} {}\u{2013}{} ({})",
            timecode(start, fps),
            timecode(end, fps),
            length(end - start, fps)
        ),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{CutBar, Item, Part};

    fn model() -> TimelineModel {
        let text = Item::layer(5.0, 8.0, ItemKind::Text, true)
            .with_label("Opening Title")
            .with_id("text-1");
        let mut seq = Item::layer(0.0, 4.0, ItemKind::Sequence, true).with_id("main");
        seq.cuts.push(CutBar {
            start: 0.0,
            end: 1.5,
            label: "interview.mp4".into(),
            ..CutBar::default()
        });
        TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items: vec![text, seq],
            ..TimelineModel::default()
        }
    }

    #[test]
    fn 帯は種類と名前と範囲と長さ() {
        let tip = bar_tooltip(&model(), 0, None);
        assert_eq!(
            tip,
            Some(TooltipContent::Text(
                "Text \u{B7} Opening Title \u{B7} 00:00:05:00\u{2013}00:00:08:00 (3:00)".into(),
                None
            ))
        );
    }

    #[test]
    fn カットは_clip_で_名前が空ならレイヤーの_id() {
        let m = model();
        let Some(TooltipContent::Text(cut, _)) = bar_tooltip(&m, 1, Some(0)) else {
            panic!("カットのツールチップ");
        };
        assert_eq!(
            cut,
            "Clip \u{B7} interview.mp4 \u{B7} 00:00:00:00\u{2013}00:00:01:15 (1:15)"
        );
        let Some(TooltipContent::Text(layer, _)) = bar_tooltip(&m, 1, None) else {
            panic!("レイヤーのツールチップ");
        };
        assert!(layer.starts_with("Sequence \u{B7} main \u{B7} "), "{layer}");
    }

    #[test]
    fn ボタンはコマンドの_id_で_目盛りと空きには出さない() {
        let m = model();
        let button = Under::Toolbar(ToolbarPart::Button("timeline.blade_tool"));
        assert_eq!(
            tooltip_of(button, &m),
            Some(TooltipContent::Command("timeline.blade_tool"))
        );
        let transport = Under::Transport(
            crate::transport::Viewer::Program,
            TransportPart::Button("playback.stop"),
        );
        assert_eq!(
            tooltip_of(transport, &m),
            Some(TooltipContent::Command("playback.stop"))
        );
        assert_eq!(tooltip_of(Under::Timeline(Hit::Ruler), &m), None);
        assert_eq!(tooltip_of(Under::Outside, &m), None);
        let cut = Under::Timeline(Hit::Cut {
            item: 1,
            cut: 0,
            part: Part::Body,
        });
        assert!(tooltip_of(cut, &m).is_some());
        let gone = Under::Timeline(Hit::Layer {
            index: 9,
            part: Part::Body,
        });
        assert_eq!(tooltip_of(gone, &m), None, "範囲の外の添字は出さない");
    }
}
