//! ステータスバーの文。左はポインターの下でできること、ドラッグ中は動かした量と吸い付いた先、右は Snapping の入/切と今のツール。
//! 先例は Blender と Kdenlive のステータスバー（ポインターの下でできる操作を出す）。

use super::{Dragging, Pointee, Under};
use crate::drag_tooltip::drag_amount;
use crate::timeline::{HeaderButton, Hit, Part};
use crate::tool::Tool;
use crate::toolbar::ToolbarPart;
use crate::transport::TransportPart;
use crate::zoombar::ZoomBarPart;

/// 文の区切り。
const SEP: &str = " \u{B7} ";

/// ステータスバーとツールチップの文に使う、アプリの表（コマンドの表、キーの表）から取った語。
#[derive(Debug, Clone, PartialEq)]
pub struct HintText {
    /// コマンドの ID と画面の名前。ツールバーのボタンの文と、右のツールの名前が引く。
    pub names: Vec<(&'static str, String)>,
    /// Snapping のキーの見た目（`N`）。
    pub snapping_key: String,
}

impl Default for HintText {
    /// アプリのコマンドの表とキーの表の既定と同じ。
    fn default() -> Self {
        let names = [
            ("timeline.selection_tool", "Selection Tool"),
            ("timeline.trim_tool", "Trim Tool"),
            ("timeline.blade_tool", "Blade Tool"),
        ];
        Self {
            names: names.iter().map(|(i, n)| (*i, (*n).to_owned())).collect(),
            snapping_key: "N".into(),
        }
    }
}

impl HintText {
    fn name<'a>(&'a self, id: &'a str) -> &'a str {
        self.names
            .iter()
            .find(|(i, _)| *i == id)
            .map_or(id, |(_, n)| n.as_str())
    }
}

/// 右に出す、Snapping の入/切と今のツール。
pub fn status_right(snapping: bool, tool: Tool, text: &HintText) -> String {
    let snap = if snapping { "On" } else { "Off" };
    format!("Snapping {snap}{SEP}{}", text.name(tool.command()))
}

/// 左に出す、ポインターの下でできること。ドラッグ中は動かした量と吸い付いた先。
pub fn status_left(p: &Pointee, fps: f64, text: &HintText) -> String {
    match (p.dragging, p.under) {
        (Some(d), _) => dragging(d, fps, text),
        (None, Under::Timeline(hit)) => timeline(hit, p),
        (None, Under::Toolbar(ToolbarPart::Button(id))) => format!("Click: {}", text.name(id)),
        (None, Under::Toolbar(ToolbarPart::Slider)) => "Drag: Zoom".into(),
        (None, Under::Transport(_, TransportPart::Button(id))) => {
            format!("Click: {}", text.name(id))
        }
        (None, Under::Transport(_, TransportPart::Jog)) => "Click or Drag: Move Playhead".into(),
        (None, Under::Outside) => String::new(),
    }
}

/// 「Ripple Trim +12f · Snapped to Playhead · N: Toggle Snapping · Esc: Cancel」。
fn dragging(d: Dragging, fps: f64, text: &HintText) -> String {
    let mut out = d.kind.name().to_owned();
    if let Some(by) = d.by {
        out.push(' ');
        out.push_str(&drag_amount(by, fps));
    }
    if let Some(snap) = d.snap {
        out.push_str(SEP);
        out.push_str(snap);
    }
    format!(
        "{out}{SEP}{}: Toggle Snapping{SEP}Esc: Cancel",
        text.snapping_key
    )
}

fn timeline(hit: Hit, p: &Pointee) -> String {
    let s = match (hit, p.tool) {
        (Hit::Layer { .. }, Tool::Blade) => "Blade Tool: Layers Can't Be Split",
        (Hit::Cut { .. }, Tool::Blade) if p.gap => "Nothing to Split in a Gap",
        // 割る時刻がカットの端に丸まる所は割らない（カーソルも禁止の形）。
        (Hit::Cut { .. }, Tool::Blade) if !p.blade_ready => "Nothing to Split at a Clip Edge",
        (Hit::Cut { .. }, Tool::Blade) => "Click: Split at Pointer",
        (Hit::Cut { part, .. }, _) if part != Part::Body => {
            "Drag: Ripple Trim \u{B7} Alt+Drag: Roll"
        }
        (Hit::Cut { .. }, Tool::Trim) if p.gap => "Shift+Drag: Slide",
        (Hit::Cut { .. }, Tool::Trim) => "Drag: Slip \u{B7} Shift+Drag: Slide",
        (Hit::Cut { .. }, Tool::Selection) => "Click: Select",
        (Hit::Layer { part, .. }, _) if part != Part::Body => "Drag: Trim Layer",
        (Hit::Layer { .. }, Tool::Selection) => "Drag: Move in Time \u{B7} Click: Select",
        (Hit::Layer { .. }, Tool::Trim) => "Click: Select",
        (Hit::Ruler, _) => "Click or Drag: Move Playhead",
        (Hit::Header { .. }, _) => "Drag: Reorder Layers",
        (Hit::HeaderButton { button, .. }, _) => header_button(button),
        (Hit::Disclosure { .. }, _) => "Click: Open or Close Keyframe Rows",
        (Hit::KeyframeRowHeader { .. }, _) => "Click: Select Property",
        (Hit::KeyframeRow { .. }, _) => "Click: Select Property \u{B7} Alt+Click: Add Keyframe",
        (Hit::RowKeyframe(_), _) => {
            "Drag: Move Keyframe \u{B7} Alt+Click: Add Keyframe \u{B7} Delete: Remove"
        }
        (Hit::BarMark { .. }, _) => "Click: Move Playhead to Keyframe",
        (Hit::Scrollbar { .. }, _) => "Drag: Scroll Layers",
        (Hit::ZoomBar { part }, _) => zoom_bar(part),
        (Hit::Corner | Hit::Empty, _) => "",
    };
    s.to_owned()
}

fn header_button(button: HeaderButton) -> &'static str {
    match button {
        HeaderButton::Enabled => "Click: Enable or Disable Layer",
        HeaderButton::Lock => "Click: Lock or Unlock Layer",
        HeaderButton::Mute => "Click: Mute or Unmute Layer",
        HeaderButton::Solo => "Click: Solo or Unsolo Layer",
    }
}

fn zoom_bar(part: ZoomBarPart) -> &'static str {
    match part {
        ZoomBarPart::StartEdge | ZoomBarPart::EndEdge => "Drag: Zoom Timeline",
        ZoomBarPart::Thumb => "Drag: Scroll Timeline",
        ZoomBarPart::TrackBefore | ZoomBarPart::TrackAfter => "Click: Scroll One Page",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hint::EditDrag;

    fn under(hit: Hit, tool: Tool) -> Pointee {
        Pointee {
            under: Under::Timeline(hit),
            tool,
            ..Pointee::default()
        }
    }

    fn left(p: &Pointee) -> String {
        status_left(p, 30.0, &HintText::default())
    }

    fn cut(part: Part) -> Hit {
        Hit::Cut {
            item: 0,
            cut: 0,
            part,
        }
    }

    #[test]
    fn 設計書の表の全部の行() {
        let sel = Tool::Selection;
        assert_eq!(
            left(&under(cut(Part::OutHandle), sel)),
            "Drag: Ripple Trim \u{B7} Alt+Drag: Roll"
        );
        assert_eq!(
            left(&under(cut(Part::Body), Tool::Trim)),
            "Drag: Slip \u{B7} Shift+Drag: Slide"
        );
        let layer = Hit::Layer {
            index: 0,
            part: Part::Body,
        };
        assert_eq!(
            left(&under(layer, sel)),
            "Drag: Move in Time \u{B7} Click: Select"
        );
        assert_eq!(
            left(&under(Hit::Ruler, sel)),
            "Click or Drag: Move Playhead"
        );
        assert_eq!(
            left(&under(Hit::Header { index: 0 }, sel)),
            "Drag: Reorder Layers"
        );
        let drag = Pointee {
            dragging: Some(Dragging {
                kind: EditDrag::Ripple,
                by: Some(12.0 / 30.0),
                snap: Some("Snapped to Playhead"),
            }),
            ..under(cut(Part::OutHandle), sel)
        };
        assert_eq!(
            left(&drag),
            "Ripple Trim +12f \u{B7} Snapped to Playhead \u{B7} N: Toggle Snapping \u{B7} Esc: Cancel"
        );
    }

    #[test]
    fn ドラッグを始めたばかりで動かしていなければ量と吸い付きを出さない() {
        let drag = Pointee {
            dragging: Some(Dragging {
                kind: EditDrag::Roll,
                by: None,
                snap: None,
            }),
            ..Pointee::default()
        };
        assert_eq!(
            left(&drag),
            "Roll \u{B7} N: Toggle Snapping \u{B7} Esc: Cancel"
        );
    }

    #[test]
    fn blade_のカットは割る_レイヤーと隙間とカットの端は割れない() {
        let ready = Pointee {
            blade_ready: true,
            ..under(cut(Part::Body), Tool::Blade)
        };
        assert_eq!(left(&ready), "Click: Split at Pointer");
        let gap = Pointee { gap: true, ..ready };
        assert_eq!(left(&gap), "Nothing to Split in a Gap");
        let edge = Pointee {
            blade_ready: false,
            ..ready
        };
        assert_eq!(
            left(&edge),
            "Nothing to Split at a Clip Edge",
            "カーソルが禁止の形の所で割ると出さない"
        );
    }

    #[test]
    fn trim_の隙間は_slide_だけ() {
        let gap = Pointee {
            gap: true,
            ..under(cut(Part::Body), Tool::Trim)
        };
        assert_eq!(left(&gap), "Shift+Drag: Slide");
    }

    #[test]
    fn キーフレームの行は選ぶ_打つ_動かす() {
        let sel = Tool::Selection;
        let at = crate::keyframe_rows::KeyframeIndex {
            item: 0,
            row: 0,
            keyframe: 0,
        };
        assert_eq!(
            left(&under(Hit::RowKeyframe(at), sel)),
            "Drag: Move Keyframe \u{B7} Alt+Click: Add Keyframe \u{B7} Delete: Remove"
        );
        assert_eq!(
            left(&under(Hit::KeyframeRow { item: 0, row: 0 }, sel)),
            "Click: Select Property \u{B7} Alt+Click: Add Keyframe"
        );
        let drag = Pointee {
            dragging: Some(Dragging {
                kind: EditDrag::MoveKeyframe,
                by: Some(-0.5),
                snap: None,
            }),
            ..under(Hit::RowKeyframe(at), sel)
        };
        assert_eq!(
            left(&drag),
            "Move Keyframe \u{2212}15f \u{B7} N: Toggle Snapping \u{B7} Esc: Cancel"
        );
    }

    #[test]
    fn ボタンの文はコマンドの表の名前と_何も無い所は空() {
        let mut text = HintText::default();
        text.names.push(("clip.lift", "Lift".into()));
        let p = Pointee {
            under: Under::Toolbar(ToolbarPart::Button("clip.lift")),
            ..Pointee::default()
        };
        assert_eq!(status_left(&p, 30.0, &text), "Click: Lift");
        assert_eq!(left(&Pointee::default()), "");
    }

    #[test]
    fn 右は_snapping_の入切と今のツールの名前() {
        let text = HintText::default();
        assert_eq!(
            status_right(true, Tool::Selection, &text),
            "Snapping On \u{B7} Selection Tool"
        );
        assert_eq!(
            status_right(false, Tool::Blade, &text),
            "Snapping Off \u{B7} Blade Tool"
        );
    }
}
