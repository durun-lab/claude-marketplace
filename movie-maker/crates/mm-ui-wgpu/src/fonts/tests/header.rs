//! レイヤーの見出しの列の幅（`HEADER_W`）を、OS のフォントで測った名前で確かめる。

use mm_ui_layout::{
    timeline_labels, Item, ItemKind, RowHeights, Rows, TimeView, TimelineController, TimelineModel,
    UiFont,
};

use super::{installed, shaper};

/// sequence の自動作成が付ける、最初の sequence のレイヤーの ID。
const DEFAULT_ID: &str = "Sequence 1 layer";

#[test]
fn 既定の_sequence_の_id_は_mute_と_solo_と開閉を残しても切り詰めない() {
    let mut s = shaper();
    if !installed(&s, "Tahoma") {
        return;
    }
    let item = Item::layer(0.0, 2.0, ItemKind::Sequence, true)
        .with_id(DEFAULT_ID)
        .with_audio(false, false)
        .with_keyframes(&[1.0]);
    let model = TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![item],
        ..TimelineModel::default()
    };
    let mut c = TimelineController::new(TimeView {
        origin: 0.0,
        px_per_sec: 50.0,
    });
    c.set_rows(Rows::of(&model.items, &RowHeights::default()));
    c.resize((800.0, 200.0));
    let (vp, state) = (c.viewport(), c.paint_state(0.0));
    let labels = timeline_labels(&model, &vp, &state, &UiFont::default(), &mut s);
    let name = labels
        .iter()
        .find(|l| &*l.text == DEFAULT_ID)
        .expect("名前");
    assert_eq!(name.cut, None, "{DEFAULT_ID} を切り詰めた");
    for letter in ["M", "S"] {
        assert!(labels.iter().any(|l| &*l.text == letter), "{letter} を残す");
    }
}
