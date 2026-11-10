use super::*;
use mm_ui_layout::{CutBar, Item, ItemKind};

/// 撮影素材のレイヤー（キー `a`）と、カットのキーが `a`、`b` のシーケンス。
fn model() -> TimelineModel {
    let mut seq = Item::layer(0.0, 4.0, ItemKind::Sequence, true);
    seq.cuts = ["a", "b"]
        .iter()
        .map(|key| CutBar {
            thumb: Some((*key).into()),
            ..CutBar::default()
        })
        .collect();
    TimelineModel {
        items: vec![
            Item::layer(0.0, 2.0, ItemKind::Footage, true).with_thumb("a"),
            seq,
        ],
        ..TimelineModel::default()
    }
}

fn keys(m: &TimelineModel) -> Vec<Option<&str>> {
    m.items
        .iter()
        .flat_map(|i| std::iter::once(&i.thumb).chain(i.cuts.iter().map(|c| &c.thumb)))
        .map(Option::as_deref)
        .collect()
}

#[test]
fn 読めなかったキーはレイヤーとカットの帯から外し_戻すと元の帯に付く() {
    let mut m = model();
    let mut dropped = DroppedThumbs::default();
    dropped.drop_where(&mut m, |k| k == "a");
    assert_eq!(keys(&m), [None, None, None, Some("b")]);
    assert_eq!(dropped.0.len(), 2, "レイヤーとカットで 2 つ");
    dropped.restore(&mut m);
    assert_eq!(keys(&m), keys(&model()));
    assert!(dropped.0.is_empty());
}
