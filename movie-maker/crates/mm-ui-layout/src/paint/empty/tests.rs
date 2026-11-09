use super::*;
use crate::text::tests::FakeMeasure;
use crate::timeline::{Item, ItemKind};
use crate::timeline_layout::tests::TestViewport;

fn empty() -> TimelineModel {
    TimelineModel {
        duration: 5.0,
        fps: 30.0,
        items: Vec::new(),
        markers: Vec::new(),
    }
}

fn one_layer() -> TimelineModel {
    TimelineModel {
        items: vec![Item::layer(0.0, 5.0, ItemKind::Image, true)],
        ..empty()
    }
}

fn texts(labels: &[Label]) -> Vec<&str> {
    labels.iter().map(|l| &*l.text).collect()
}

fn program(model: &TimelineModel, hints: &EmptyHints) -> Vec<Label> {
    let area = Rect::new(0.0, 0.0, 800.0, 400.0);
    let mut m = FakeMeasure::default();
    program_labels(
        model,
        area,
        hints,
        &Palette::DEFAULT,
        &UiFont::default(),
        &mut m,
    )
}

fn timeline(model: &TimelineModel, hints: &EmptyHints) -> Vec<Label> {
    let f = TestViewport::new(model, (800.0, 200.0));
    let mut m = FakeMeasure::default();
    timeline_labels(
        model,
        &f.vp(),
        hints,
        &Palette::DEFAULT,
        &UiFont::default(),
        &mut m,
    )
}

#[test]
fn レイヤーが無いとき_program_に取り込みの案内を出す() {
    let labels = program(&empty(), &EmptyHints::default());
    assert_eq!(
        texts(&labels),
        [
            "Import video and audio to start",
            "File > Import Media\u{2026} (Ctrl+I)"
        ]
    );
    assert!(labels[0].y < labels[1].y, "見出しが上");
}

#[test]
fn レイヤーを足すと_program_の案内は消える() {
    assert!(program(&one_layer(), &EmptyHints::default()).is_empty());
}

#[test]
fn program_の案内のキーは受け取ったものを出す() {
    let hints = EmptyHints {
        import_media: "Ctrl+Shift+M".into(),
        ..EmptyHints::default()
    };
    let labels = program(&empty(), &hints);
    assert_eq!(
        &*labels[1].text,
        "File > Import Media\u{2026} (Ctrl+Shift+M)"
    );
}

#[test]
fn レイヤーが無いとき_timeline_の行の領域に編集の手順を出す() {
    let f = TestViewport::new(&empty(), (800.0, 200.0));
    let labels = timeline(&empty(), &EmptyHints::default());
    assert_eq!(
        texts(&labels),
        ["Open a clip in Media, mark In (I) and Out (O), then press Insert (,)"]
    );
    assert_eq!(labels[0].clip, f.layout.row_area, "見出しの列に出ない");
}

#[test]
fn レイヤーを足すと_timeline_の案内は消える() {
    assert!(timeline(&one_layer(), &EmptyHints::default()).is_empty());
}

#[test]
fn timeline_の案内のキーは受け取ったものを出す() {
    let hints = EmptyHints {
        mark_in: "[".into(),
        mark_out: "]".into(),
        insert: "Enter".into(),
        ..EmptyHints::default()
    };
    let labels = timeline(&empty(), &hints);
    assert_eq!(
        &*labels[0].text,
        "Open a clip in Media, mark In ([) and Out (]), then press Insert (Enter)"
    );
}
