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
fn 読めなかったキーはレイヤーとカットの帯から外し_読めたキーだけを元の帯に戻す() {
    let mut m = model();
    let mut dropped = DroppedThumbs::default();
    dropped.drop_where(&mut m, |k| k == "a" || k == "b");
    assert_eq!(keys(&m), [None, None, None, None]);
    assert_eq!(dropped.0.len(), 3, "レイヤーとカットで 3 つ");
    dropped.restore_key(&mut m, "a");
    assert_eq!(keys(&m), [Some("a"), None, Some("a"), None]);
    assert_eq!(dropped.0.len(), 1, "読めていない b は外したまま");
}
