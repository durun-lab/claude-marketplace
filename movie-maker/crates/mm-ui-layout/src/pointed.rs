//! ポインターの下の所を表す粗い型。右クリックのメニューが「当たった所ごとの項目」を選ぶのに使う。
//! 描画のスレッドの `TimelineController` が当たり判定（[`Hit`]）を持つので、UI のスレッドへはこの型で渡す。
//! カーソル、ツールチップ、ステータスバーが同じ所を使うときは、この型に足すか [`Place::of`] を通す。

use crate::timeline::Hit;

/// ポインターの下の所。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// シーケンスのカット。
    Cut,
    /// レイヤーの帯（端のつまみと帯のマーカーを含む）。
    Layer,
    /// レイヤーの見出し（ボタンとキーフレームの開閉を含む）。
    Header,
    /// キーフレームの行（見出しと、ひし形の無い所）。
    KeyframeRow,
    /// キーフレームの行のひし形。
    Keyframe,
    /// 目盛りの帯。
    Ruler,
    /// 行のない所。
    Empty,
}

impl Place {
    /// 当たり判定の結果が表す所。スクロールバー、ズームバー、角はメニューを出さないので `None`。
    pub fn of(hit: &Hit) -> Option<Self> {
        Some(match hit {
            Hit::Cut { .. } => Place::Cut,
            Hit::Layer { .. } | Hit::BarMark { .. } => Place::Layer,
            Hit::Header { .. } | Hit::HeaderButton { .. } | Hit::Disclosure { .. } => Place::Header,
            Hit::KeyframeRowHeader { .. } | Hit::KeyframeRow { .. } => Place::KeyframeRow,
            Hit::RowKeyframe(_) => Place::Keyframe,
            Hit::Ruler => Place::Ruler,
            Hit::Empty => Place::Empty,
            Hit::Corner | Hit::Scrollbar { .. } | Hit::ZoomBar { .. } => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframe_rows::KeyframeIndex;
    use crate::timeline::{HeaderButton, Part};
    use crate::zoombar::ZoomBarPart;

    #[test]
    fn 当たった所を右クリックのメニューの種類にまとめる() {
        let layer = Hit::Layer {
            index: 0,
            part: Part::Body,
        };
        let bar_mark = Hit::BarMark {
            item: 0,
            keyframe: 0,
        };
        let button = Hit::HeaderButton {
            index: 1,
            button: HeaderButton::Lock,
        };
        let cut = Hit::Cut {
            item: 0,
            cut: 2,
            part: Part::Body,
        };
        assert_eq!(Place::of(&cut), Some(Place::Cut));
        assert_eq!(Place::of(&layer), Some(Place::Layer));
        assert_eq!(Place::of(&bar_mark), Some(Place::Layer));
        assert_eq!(Place::of(&Hit::Header { index: 0 }), Some(Place::Header));
        assert_eq!(Place::of(&button), Some(Place::Header));
        assert_eq!(
            Place::of(&Hit::Disclosure { index: 0 }),
            Some(Place::Header)
        );
        assert_eq!(Place::of(&Hit::Ruler), Some(Place::Ruler));
        assert_eq!(Place::of(&Hit::Empty), Some(Place::Empty));
    }

    #[test]
    fn キーフレームの行は見出しとひし形の無い所を行に_ひし形をキーフレームにまとめる() {
        let (item, row) = (0, 1);
        let diamond = Hit::RowKeyframe(KeyframeIndex {
            item,
            row,
            keyframe: 2,
        });
        let header = Hit::KeyframeRowHeader { item, row };
        let lane = Hit::KeyframeRow { item, row };
        assert_eq!(Place::of(&header), Some(Place::KeyframeRow));
        assert_eq!(Place::of(&lane), Some(Place::KeyframeRow));
        assert_eq!(Place::of(&diamond), Some(Place::Keyframe));
    }

    #[test]
    fn 角とスクロールバーとズームバーはメニューを出さない() {
        let zoom = Hit::ZoomBar {
            part: ZoomBarPart::Thumb,
        };
        for hit in [Hit::Corner, Hit::Scrollbar { thumb: true }, zoom] {
            assert_eq!(Place::of(&hit), None, "{hit:?}");
        }
    }
}
