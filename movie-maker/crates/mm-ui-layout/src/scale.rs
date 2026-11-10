//! 表示の倍率。UI の寸法は論理ピクセル（WebView の CSS の px と同じ大きさ）で持ち、
//! サーフェスの物理ピクセルとの換算はこの型だけが行う（egui の `pixels_per_point`、winit の `scale_factor` と同じ分け方）。

use crate::controller::PointerEvent;
use crate::geom::Rect;
use crate::screen::SurfaceLayout;
use crate::text::Label;

/// 細い線とみなす太さの上限（論理ピクセル）。目盛り、カットの境、外枠、再生位置の線がこれ以下。
const THIN_MAX: f32 = 2.0;

/// 文字の置き場所（物理ピクセル）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextPlacement {
    /// 行の矩形の左上。整数の画素。
    pub x: f32,
    pub y: f32,
    /// グリフに掛ける表示の倍率。字は論理ピクセルの大きさでシェーピングし、描くときにこれを掛ける。
    pub scale: f32,
    /// 切り詰めたときの、描く右端と「…」の左端。整数の画素。
    pub cut: Option<f32>,
    /// 字を描いてよい矩形（[`Label::clip`]）。
    pub clip: Rect,
}

/// 表示の倍率（DPI / 96）。1 論理ピクセルあたりの物理ピクセルの数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale(f32);

impl Scale {
    /// 等倍（96 DPI）。
    pub const ONE: Self = Self(1.0);

    /// 表示の倍率の値から。0 以下や有限でない値（破棄された HWND の DPI など）は等倍にする。
    pub fn new(factor: f32) -> Self {
        if factor.is_finite() && factor > 0.0 {
            Self(factor)
        } else {
            Self::ONE
        }
    }

    /// 表示の倍率の値。
    pub fn get(self) -> f32 {
        self.0
    }

    /// 物理ピクセルの長さを論理ピクセルにする。
    pub fn to_logical(self, physical: f32) -> f32 {
        physical / self.0
    }

    /// 論理ピクセルの矩形を物理ピクセルにする。辺はラスタライザーの画素の中心の判定に任せ、
    /// 細い線（`THIN_MAX` 以下）の太さだけ整数の物理ピクセルにそろえる（egui が線の太さを画素に合わせるのと同じ）。
    pub fn rect_to_physical(self, r: Rect) -> Rect {
        let (x, w) = self.span_to_physical(r.x, r.w);
        let (y, h) = self.span_to_physical(r.y, r.h);
        Rect::new(x, y, w, h)
    }

    /// 1 軸の区間。1.25 倍の 1 論理ピクセルの線はそのままだと位置で 1 画素にも 2 画素にもなるので、
    /// 細い線は太さを丸め、中心に寄せた整数の位置に置く。
    fn span_to_physical(self, start: f32, len: f32) -> (f32, f32) {
        if len > 0.0 && len <= THIN_MAX {
            let px = (len * self.0).round().max(1.0);
            let centre = (start + len / 2.0) * self.0;
            ((centre - px / 2.0).round(), px)
        } else {
            (start * self.0, len * self.0)
        }
    }

    /// 文字の置き場所を物理ピクセルにする。行の矩形の左上を整数の画素に置き、グリフの基線を画素の境にそろえる
    /// （1.25 倍でも字が上下の画素ににじまない）。切り詰めの右端は左上から画素の境に切り捨て、次の字を描かない。
    pub fn label_to_physical(self, label: &Label) -> TextPlacement {
        let x = (label.x * self.0).round();
        let c = label.clip;
        TextPlacement {
            x,
            y: (label.y * self.0).round(),
            scale: self.0,
            cut: label.cut.map(|w| x + (w * self.0).floor()),
            clip: Rect::new(c.x * self.0, c.y * self.0, c.w * self.0, c.h * self.0),
        }
    }

    /// 論理ピクセルの割り付けを物理ピクセルにする。
    pub fn layout_to_physical(self, l: SurfaceLayout) -> SurfaceLayout {
        SurfaceLayout {
            program: self.rect_to_physical(l.program),
            program_transport: self.rect_to_physical(l.program_transport),
            timeline: self.rect_to_physical(l.timeline),
            source: l.source.map(|r| self.rect_to_physical(r)),
            source_transport: l.source_transport.map(|r| self.rect_to_physical(r)),
        }
    }

    /// 物理ピクセルのポインターの座標を論理ピクセルにする。ホイールの量は目盛りの数なのでそのまま。
    pub fn pointer_to_logical(self, ev: PointerEvent) -> PointerEvent {
        let l = |v: f32| self.to_logical(v);
        match ev {
            PointerEvent::Down { x, y, mods } => PointerEvent::Down {
                x: l(x),
                y: l(y),
                mods,
            },
            PointerEvent::Move { x, y } => PointerEvent::Move { x: l(x), y: l(y) },
            PointerEvent::Wheel {
                x,
                y,
                dy,
                ctrl,
                shift,
            } => PointerEvent::Wheel {
                x: l(x),
                y: l(y),
                dy,
                ctrl,
                shift,
            },
            other => other,
        }
    }
}

impl Default for Scale {
    fn default() -> Self {
        Self::ONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::Mods;
    use crate::screen::split_viewers;
    use crate::timeline::{hit_test, Hit, Item, ItemKind, Part, TimelineModel, HANDLE_PX};
    use crate::timeline_layout::tests::TestViewport;
    use crate::timeline_layout::HEADER_W;

    #[test]
    fn 等倍では矩形も割り付けもポインターも変えない() {
        let s = Scale::ONE;
        let l = split_viewers(1013.0, 777.0, true);
        assert_eq!(s.layout_to_physical(l), l);
        let r = Rect::new(10.3, 4.7, 7.0, 24.0);
        assert_eq!(s.rect_to_physical(r), r);
        let ev = PointerEvent::Move { x: 12.5, y: 3.25 };
        assert_eq!(s.pointer_to_logical(ev), ev);
    }

    #[test]
    fn 表示の倍率_150_と_200_ではつまみと帯の寸法を論理ピクセルのまま物理ピクセルに換える() {
        let model = TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items: vec![Item::layer(0.0, 5.0, ItemKind::Footage, true)],
            ..TimelineModel::default()
        };
        let f = TestViewport::new(&model, (800.0, 200.0));
        let bar = f.vp().bar(0, 1.0, 5.0);
        for factor in [1.5, 2.0] {
            let s = Scale::new(factor);
            let handle = s.rect_to_physical(Rect::new(bar.x, bar.y, HANDLE_PX, bar.h));
            assert_eq!(handle.w, HANDLE_PX * factor, "つまみの幅");
            assert_eq!(handle.h, bar.h * factor, "帯の高さ");
            assert_eq!(handle.x, (HEADER_W + 100.0) * factor);
        }
    }

    #[test]
    fn 物理ピクセルのサーフェスは論理ピクセルの大きさで割り付けてから物理ピクセルに戻す() {
        let s = Scale::new(1.5);
        let logical = split_viewers(1000.0, 600.0, false);
        let physical = s.layout_to_physical(logical);
        assert_eq!(physical.timeline.h, logical.timeline.h * 1.5);
        assert_eq!(physical.timeline.right(), 1500.0);
        assert_eq!(physical.timeline.bottom(), 900.0);
    }

    #[test]
    fn 物理ピクセルの点は論理ピクセルのつまみに当たる() {
        let s = Scale::new(1.5);
        let model = TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items: vec![Item::layer(0.0, 5.0, ItemKind::Footage, true)],
            ..TimelineModel::default()
        };
        let f = TestViewport::new(&model, (800.0, 200.0));
        // 論理ピクセルの 678（帯の右端 680 の内側 2）は物理ピクセルの 1017。
        let bar = f.vp().bar(0, 0.0, 5.0);
        let y = (bar.y + bar.h / 2.0) * 1.5;
        let PointerEvent::Move { x, y } = s.pointer_to_logical(PointerEvent::Move { x: 1017.0, y })
        else {
            panic!("Move のまま");
        };
        assert_eq!(
            hit_test(&model, &f.vp(), x, y),
            Hit::Layer {
                index: 0,
                part: Part::OutHandle
            }
        );
        let down = s.pointer_to_logical(PointerEvent::Down {
            x: 3.0,
            y: 6.0,
            mods: Mods::default(),
        });
        assert!(matches!(down, PointerEvent::Down { x, y, .. } if x == 2.0 && y == 4.0));
    }

    /// ラスタライザーと同じく、画素の中心が `[start, start + len)` に入る画素の数。
    fn covered(start: f32, len: f32) -> u32 {
        let first = (start - 0.5).ceil();
        let end = (start + len - 0.5).ceil();
        (end - first).max(0.0) as u32
    }

    #[test]
    fn 細い線は表示の倍率_125_と_150_でも位置に依らず同じ画素の数で描く() {
        for (factor, want_1, want_2) in [(1.25, 1, 3), (1.5, 2, 3), (1.0, 1, 2), (2.0, 2, 4)] {
            let s = Scale::new(factor);
            for i in 0..400 {
                let x = i as f32 * 0.37;
                let tick = s.rect_to_physical(Rect::new(x, 17.0, 1.0, 9.0));
                let playhead = s.rect_to_physical(Rect::new(x, 0.0, 2.0, 40.0));
                assert_eq!(
                    covered(tick.x, tick.w),
                    want_1,
                    "表示の倍率 {factor}、x {x}: 1 の線"
                );
                assert_eq!(
                    covered(playhead.x, playhead.w),
                    want_2,
                    "表示の倍率 {factor}、x {x}: 2 の線"
                );
                assert_eq!(tick.h, 9.0 * factor, "長さの向きは丸めない");
            }
        }
    }

    #[test]
    fn 目盛りの線の太さは表示の倍率_125_でもそろう() {
        let model = TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items: vec![Item::layer(0.0, 5.0, ItemKind::Footage, true)],
            ..TimelineModel::default()
        };
        let mut f = TestViewport::new(&model, (800.0, 200.0));
        f.time.px_per_sec = 37.3;
        let state = crate::paint::tests::state(0.0);
        let s = Scale::new(1.25);
        let vp = f.vp();
        let input = crate::paint::PaintInput::new(&model, &vp, &state);
        let widths: Vec<u32> = crate::paint::timeline_quads(&input)
            .quads
            .iter()
            .filter(|q| q.rect.w == 1.0)
            .map(|q| {
                let r = s.rect_to_physical(q.rect);
                covered(r.x, r.w)
            })
            .collect();
        assert!(widths.len() > 10, "目盛りがある");
        assert!(widths.iter().all(|w| *w == 1), "{widths:?}");
    }

    #[test]
    fn 文字は表示の倍率_125_から_200_で左上を整数の画素に置き_切り詰めの右端を左上から測る() {
        let label = Label {
            text: "name".into(),
            x: 10.3,
            y: 4.7,
            size: 12.0,
            color: crate::geom::Color([1.0; 4]),
            cut: Some(30.0),
            clip: Rect::new(8.0, 2.0, 100.0, 24.0),
        };
        let cases = [
            (1.25, 13.0, 6.0, 50.0),
            (1.5, 15.0, 7.0, 60.0),
            (2.0, 21.0, 9.0, 81.0),
        ];
        for (factor, x, y, cut) in cases {
            let p = Scale::new(factor).label_to_physical(&label);
            assert_eq!((p.x, p.y, p.scale), (x, y, factor), "表示の倍率 {factor}");
            assert_eq!(
                p.cut,
                Some(cut),
                "表示の倍率 {factor}: 左上 + 30 論理ピクセル"
            );
            let c = p.clip;
            assert_eq!(
                (c.x, c.y, c.w, c.h),
                (8.0 * factor, 2.0 * factor, 100.0 * factor, 24.0 * factor)
            );
        }
        let whole = Label { cut: None, ..label };
        assert_eq!(Scale::new(1.25).label_to_physical(&whole).cut, None);
    }

    #[test]
    fn 有限でない表示の倍率や_0_は等倍にする() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(Scale::new(bad), Scale::ONE, "{bad}");
        }
    }
}
