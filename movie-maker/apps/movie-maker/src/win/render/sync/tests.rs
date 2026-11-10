//! Shy の印と隠す行を Timeline のモデルに当てる所の試験。

use mm_ui_layout::{Item, ItemKind};

use super::*;

/// レイヤー a、b、c（b は sequence でカット x を持つ）の行と ID の対応。
fn rows() -> (DocSync, TimelineModel) {
    let doc = DocSync {
        layer_ids: ["a", "b", "c"].map(str::to_owned).to_vec(),
        cut_ids: vec![Vec::new(), vec!["x".to_owned()], Vec::new()],
        ..DocSync::default()
    };
    let items = [ItemKind::Image, ItemKind::Sequence, ItemKind::Text]
        .map(|kind| Item::layer(0.0, 1.0, kind, true))
        .to_vec();
    let model = TimelineModel {
        items,
        ..TimelineModel::default()
    };
    (doc, model)
}

#[test]
fn 隠さない間は印を付けた行に印を出すだけで_行は外さない() {
    let (mut doc, mut model) = rows();
    let shy = Shy {
        layers: ["b".to_owned()].into(),
        hide: false,
    };
    doc.apply_shy(&shy, &mut model);
    let marks: Vec<bool> = model.items.iter().map(|i| i.shy).collect();
    assert_eq!(marks, [false, true, false]);
    assert_eq!((model.hidden, doc.layer_ids.len()), (0, 3));
}

#[test]
fn 隠すと印の行をモデルと_id_の対応から同じだけ外し_外した数を持つ() {
    let (mut doc, mut model) = rows();
    let shy = Shy {
        layers: ["b".to_owned(), "gone".to_owned()].into(),
        hide: true,
    };
    doc.apply_shy(&shy, &mut model);
    assert_eq!(model.hidden, 1, "無くなったレイヤーの印は数えない");
    assert_eq!(model.items.len(), 2);
    assert_eq!(doc.layer_ids, ["a", "c"]);
    assert_eq!(doc.cut_ids, [Vec::<String>::new(), Vec::new()]);
    assert!(model.items.iter().all(|i| !i.shy));
}
