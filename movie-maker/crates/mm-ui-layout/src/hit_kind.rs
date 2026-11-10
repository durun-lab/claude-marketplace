//! 当たった所の種類。当たった所（[`Hit`]）を、右クリックのメニューの種類ごとにまとめる。
//! 描画のスレッドの `TimelineController` が当たり判定を持つので、UI のスレッドへは `Hit` を渡し、種類はこの型で引く。
//! カーソル、ツールチップ、ステータスバーが右クリックのメニューの種類を使うときも、[`HitKind::of`] を通す。

use crate::timeline::Hit;

/// 当たった所の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
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
    /// 行の無い所。
    Empty,
}

impl HitKind {
    /// 当たった所の種類。スクロールバー、ズームバー、角はメニューを出さないので `None`。
    pub fn of(hit: &Hit) -> Option<Self> {
        Some(match hit {
            Hit::Cut { .. } => HitKind::Cut,
            Hit::Layer { .. } | Hit::BarMark { .. } => HitKind::Layer,
            Hit::Header { .. } | Hit::HeaderButton { .. } | Hit::Disclosure { .. } => {
                HitKind::Header
            }
            Hit::KeyframeRowHeader { .. } | Hit::KeyframeRow { .. } => HitKind::KeyframeRow,
            Hit::RowKeyframe(_) => HitKind::Keyframe,
            Hit::Ruler => HitKind::Ruler,
            Hit::Empty => HitKind::Empty,
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
        assert_eq!(HitKind::of(&cut), Some(HitKind::Cut));
        assert_eq!(HitKind::of(&layer), Some(HitKind::Layer));
        assert_eq!(HitKind::of(&bar_mark), Some(HitKind::Layer));
        assert_eq!(
            HitKind::of(&Hit::Header { index: 0 }),
            Some(HitKind::Header)
        );
        assert_eq!(HitKind::of(&button), Some(HitKind::Header));
        assert_eq!(
            HitKind::of(&Hit::Disclosure { index: 0 }),
            Some(HitKind::Header)
        );
        assert_eq!(HitKind::of(&Hit::Ruler), Some(HitKind::Ruler));
        assert_eq!(HitKind::of(&Hit::Empty), Some(HitKind::Empty));
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
        assert_eq!(HitKind::of(&header), Some(HitKind::KeyframeRow));
        assert_eq!(HitKind::of(&lane), Some(HitKind::KeyframeRow));
        assert_eq!(HitKind::of(&diamond), Some(HitKind::Keyframe));
    }

    #[test]
    fn 角とスクロールバーとズームバーはメニューを出さない() {
        let zoom = Hit::ZoomBar {
            part: ZoomBarPart::Thumb,
        };
        for hit in [Hit::Corner, Hit::Scrollbar { thumb: true }, zoom] {
            assert_eq!(HitKind::of(&hit), None, "{hit:?}");
        }
    }
}
