use super::*;
use crate::timeline_layout::tests::TestViewport;
use crate::timeline_layout::HEADER_W;

/// 見出しの列に入らない長さの ID。
const LONG_ID: &str = "Sequence 1 layer with a very long identifier";

fn row() -> Rect {
    Rect::new(0.0, 26.0, HEADER_W, 28.0)
}

/// 音を持てる sequence のレイヤー。
fn seq(id: &str) -> Item {
    Item::layer(0.0, 1.0, ItemKind::Sequence, true)
        .with_id(id)
        .with_audio(false, false)
}

#[test]
fn 名前の長さで_mute_と_solo_を省かず_部品の矩形は名前に依らない() {
    let long = cells(row(), &seq(LONG_ID));
    assert!(long.mute.is_some() && long.solo.is_some());
    assert_eq!(
        long,
        cells(row(), &seq("v1")),
        "名前の長さで割り付けを変えない"
    );
}

#[test]
fn 長い名前のレイヤーも見出しの_mute_と_solo_を押せる() {
    let m = TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![seq(LONG_ID)],
        ..TimelineModel::default()
    };
    let f = TestViewport::new(&m, (800.0, 200.0));
    let vp = f.vp();
    for button in [HeaderButton::Mute, HeaderButton::Solo] {
        let at = button_center(&vp, &m.items[0], 0, button).expect("ボタン");
        let want = Hit::HeaderButton { index: 0, button };
        assert_eq!(hit(&vp, (0, &m.items[0]), at), want);
    }
}
