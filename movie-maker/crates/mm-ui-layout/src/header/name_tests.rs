use super::*;
use crate::timeline_layout::tests::TestViewport;
use crate::timeline_layout::HEADER_W;

fn row() -> Rect {
    Rect::new(0.0, 26.0, HEADER_W, 28.0)
}

fn seq(id: &str) -> Item {
    Item::layer(0.0, 1.0, ItemKind::Sequence, true)
        .with_id(id)
        .with_audio(false, false)
}

#[test]
fn sequence_の既定の_id_は_mute_と_solo_を省いて切らずに出す() {
    let c = cells(row(), &seq("Sequence 1 layer"));
    assert_eq!((c.mute, c.solo), (None, None), "名前を優先してボタンを省く");
    assert!(c.name.w >= name_width("Sequence 1 layer"));
    assert!(c.name.right() <= c.enabled.x && c.enabled.right() <= c.lock.x);
}

#[test]
fn 短い名前は_mute_と_solo_を残す() {
    for id in ["voice-10", "lyrics-3430"] {
        let c = cells(row(), &seq(id));
        assert!(c.mute.is_some() && c.solo.is_some(), "{id}");
        assert!(c.name.w >= name_width(id), "{id} は切れない");
    }
}

#[test]
fn 省いた_mute_と_solo_は当たり判定にも出ない() {
    let m = TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![seq("Sequence 1 layer")],
        ..TimelineModel::default()
    };
    let f = TestViewport::new(&m, (800.0, 200.0));
    let vp = f.vp();
    assert_eq!(button_center(&vp, &m.items[0], 0, HeaderButton::Mute), None);
    assert_eq!(button_center(&vp, &m.items[0], 0, HeaderButton::Solo), None);
    assert!(button_center(&vp, &m.items[0], 0, HeaderButton::Lock).is_some());
}

#[test]
fn 全角の名前は半角より広く見積もる() {
    assert!(name_width("見出し") > name_width("abc"));
    assert_eq!(name_width(""), TEXT_PAD * 2.0);
}
