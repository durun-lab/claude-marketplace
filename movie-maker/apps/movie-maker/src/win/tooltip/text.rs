//! ツールチップとステータスバーの文を、UI のコマンドの表とキーの表から作る。ツールチップは「名前 (キー)」と、
//! 表に説明があれば 2 行目にその説明（先例は Premiere のツールのツールチップ）。表に無い部品（帯、見出しのボタン）は
//! `mm-ui-layout` が作った文をそのまま出す。

use mm_ui_layout::{HintText, TooltipContent};

use crate::win::commands::{self, TABLE};
use crate::win::keys;

/// ツールチップの文。1 行目は名前とキー、あれば 2 行目に説明。表に無い ID は ID をそのまま出す。
pub fn text_of(tip: &TooltipContent) -> String {
    match tip {
        TooltipContent::Text(title, None) => title.clone(),
        TooltipContent::Text(title, Some(detail)) => format!("{title}\n{detail}"),
        TooltipContent::Command(id) => {
            let Some(spec) = commands::find(id) else {
                return (*id).to_owned();
            };
            let key = keys::key_label_of(id);
            let title = if key.is_empty() {
                spec.name.to_owned()
            } else {
                format!("{} ({key})", spec.name)
            };
            match spec.description {
                Some(detail) => format!("{title}\n{detail}"),
                None => title,
            }
        }
    }
}

/// ステータスバーの文に使う語。コマンドの表の名前と、Snapping のキーの見た目。
pub fn hint_text() -> HintText {
    HintText {
        names: TABLE.iter().map(|s| (s.id, s.name.to_owned())).collect(),
        snapping_key: keys::key_label_of("timeline.toggle_snapping"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn コマンドは名前とキーと説明をコマンドの表から作る() {
        assert_eq!(
            text_of(&TooltipContent::Command("timeline.selection_tool")),
            "Selection Tool (A)\nDrag a layer to move it in time; select clips and trim their edges"
        );
        assert_eq!(
            text_of(&TooltipContent::Command("edit.undo")),
            "Undo (Ctrl+Z)",
            "説明の無いコマンドは 1 行"
        );
    }

    #[test]
    fn 設計書のツールのツールチップは名前とキー() {
        for (id, first) in [
            ("timeline.trim_tool", "Trim Tool (T)"),
            ("timeline.blade_tool", "Blade Tool (B)"),
            ("timeline.toggle_snapping", "Snapping (N)"),
            ("clip.split", "Split Clip (Ctrl+B)"),
        ] {
            let text = text_of(&TooltipContent::Command(id));
            assert_eq!(text.lines().next(), Some(first), "{id}");
        }
    }

    #[test]
    fn 表に無い部品の文はそのまま_表に無い_id_は_id() {
        let tip = TooltipContent::Text("Lock Layer".into(), Some("Keep it safe".into()));
        assert_eq!(text_of(&tip), "Lock Layer\nKeep it safe");
        assert_eq!(text_of(&TooltipContent::Text("a".into(), None)), "a");
        assert_eq!(text_of(&TooltipContent::Command("no.such")), "no.such");
    }

    #[test]
    fn ステータスバーの語はコマンドの表の全部の名前と_snapping_のキー() {
        let text = hint_text();
        assert_eq!(text.names.len(), TABLE.len());
        assert_eq!(text.snapping_key, "N");
        assert!(text
            .names
            .contains(&("timeline.selection_tool", "Selection Tool".to_owned())));
    }
}
