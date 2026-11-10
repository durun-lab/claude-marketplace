use super::*;
use crate::paint::tests::state;
use crate::text::tests::FakeMeasure;
use crate::timeline::{Item, ItemKind, TimelineModel};
use crate::timeline_layout::tests::TestViewport;

/// Program の矩形。
const AREA: Rect = Rect::new(0.0, 0.0, 800.0, 400.0);

/// どちらの案内も出す。
fn both() -> EmptyHints {
    EmptyHints {
        show: EmptyState {
            program: true,
            timeline: true,
        },
        keys: EmptyKeys::default(),
    }
}

/// カットの無い sequence のレイヤーだけの Timeline。
fn sequence_only() -> TimelineModel {
    TimelineModel {
        duration: 5.0,
        fps: 30.0,
        items: vec![Item::layer(0.0, 5.0, ItemKind::Sequence, true)],
        markers: Vec::new(),
    }
}

fn texts(labels: &[Label]) -> Vec<&str> {
    labels.iter().map(|l| &*l.text).collect()
}

fn program(hints: &EmptyHints) -> Vec<Label> {
    let mut m = FakeMeasure::default();
    hints.program(AREA, &state(0.0), &UiFont::default(), &mut m)
}

fn timeline(hints: &EmptyHints) -> Vec<Label> {
    let model = sequence_only();
    let f = TestViewport::new(&model, (800.0, 200.0));
    let mut m = FakeMeasure::default();
    hints.timeline(&f.vp(), &state(0.0), &UiFont::default(), &mut m)
}

#[test]
fn 出すと決めていない案内は置かない() {
    let hints = EmptyHints::default();
    assert!(program(&hints).is_empty());
    assert!(timeline(&hints).is_empty());
}

#[test]
fn program_の案内は取り込みの_2_行を矩形の縦の中央に置く() {
    let labels = program(&both());
    assert_eq!(
        texts(&labels),
        [
            "Import video and audio to start",
            "File > Import Media\u{2026} (Ctrl+I)"
        ]
    );
    assert!(labels[0].size > labels[1].size, "1 行目が大きい");
    let top = labels[0].y;
    let bottom = labels[1].y + line_height(labels[1].size);
    assert!(
        (top - AREA.y - (AREA.bottom() - bottom)).abs() < 1.0,
        "上下の余白が同じ: {top} と {bottom}"
    );
}

#[test]
fn program_の案内のキーは受け取ったものを出す() {
    let mut hints = both();
    hints.keys.import_media = "Ctrl+Shift+M".into();
    let labels = program(&hints);
    assert_eq!(
        &*labels[1].text,
        "File > Import Media\u{2026} (Ctrl+Shift+M)"
    );
}

#[test]
fn timeline_の案内は行の領域に_3_点編集の手順を出し_見出しの列に掛けない() {
    let f = TestViewport::new(&sequence_only(), (800.0, 200.0));
    let labels = timeline(&both());
    assert_eq!(
        texts(&labels),
        ["Open a clip in Media, mark In (I) and Out (O), then press Insert (,)"]
    );
    assert_eq!(labels[0].clip, f.layout.row_area);
}

#[test]
fn timeline_の案内のキーは受け取ったものを出す() {
    let mut hints = both();
    hints.keys = EmptyKeys {
        mark_in: "[".into(),
        mark_out: "]".into(),
        insert: "Enter".into(),
        ..EmptyKeys::default()
    };
    let labels = timeline(&hints);
    assert_eq!(
        &*labels[0].text,
        "Open a clip in Media, mark In ([) and Out (]), then press Insert (Enter)"
    );
}
