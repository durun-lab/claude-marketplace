//! 表示の倍率。UI の寸法は論理ピクセル（WebView の CSS の px と同じ大きさ）で持ち、
//! サーフェスの物理ピクセルとの換算はこの型だけが行う（egui の `pixels_per_point`、winit の `scale_factor` と同じ分け方）。

use crate::controller::PointerEvent;
use crate::geom::Rect;
use crate::screen::SurfaceLayout;

/// 表示の倍率（DPI / 96）。1 論理ピクセルあたりの物理ピクセルの数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale(f32);

impl Scale {
    /// 等倍（96 DPI）。
    pub const ONE: Self = Self(1.0);

    /// 倍率から。0 以下や有限でない値（破棄された HWND の DPI など）は等倍にする。
    pub fn new(factor: f32) -> Self {
        if factor.is_finite() && factor > 0.0 {
            Self(factor)
        } else {
            Self::ONE
        }
    }

    /// 倍率の値。
    pub fn get(self) -> f32 {
        self.0
    }

    /// 物理ピクセルの長さを論理ピクセルにする。
    pub fn to_logical(self, physical: f32) -> f32 {
        physical / self.0
    }

    /// 論理ピクセルの矩形を物理ピクセルにする。辺の画素への丸めはラスタライザーの画素の中心の判定に任せる。
    pub fn rect_to_physical(self, r: Rect) -> Rect {
        Rect::new(r.x * self.0, r.y * self.0, r.w * self.0, r.h * self.0)
    }

    /// 論理ピクセルの割り付けを物理ピクセルにする。
    pub fn layout_to_physical(self, l: SurfaceLayout) -> SurfaceLayout {
        SurfaceLayout {
            program: self.rect_to_physical(l.program),
            timeline: self.rect_to_physical(l.timeline),
            source: l.source.map(|r| self.rect_to_physical(r)),
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
            PointerEvent::Wheel { x, y, dy, ctrl } => PointerEvent::Wheel {
                x: l(x),
                y: l(y),
                dy,
                ctrl,
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
    use crate::time_view::TimeView;
    use crate::timeline::{
        hit_test, row_top, Hit, Item, ItemKind, Part, TimelineModel, HANDLE_PX, ROW_H,
    };

    #[test]
    fn 等倍では矩形も割り付けもポインターも変えない() {
        let s = Scale::ONE;
        let l = split_viewers(1013.0, 777.0, 120.0, true);
        assert_eq!(s.layout_to_physical(l), l);
        let r = Rect::new(10.3, 4.7, 7.0, 24.0);
        assert_eq!(s.rect_to_physical(r), r);
        let ev = PointerEvent::Move { x: 12.5, y: 3.25 };
        assert_eq!(s.pointer_to_logical(ev), ev);
    }

    #[test]
    fn 倍率_150_と_200_ではつまみと帯が論理の寸法のまま物理の大きさになる() {
        for factor in [1.5, 2.0] {
            let s = Scale::new(factor);
            let handle = s.rect_to_physical(Rect::new(100.0, row_top(0), HANDLE_PX, ROW_H));
            assert_eq!(handle.w, HANDLE_PX * factor, "つまみの幅");
            assert_eq!(handle.h, ROW_H * factor, "帯の高さ");
            assert_eq!(handle.x, 100.0 * factor);
        }
    }

    #[test]
    fn 物理ピクセルのサーフェスは論理の大きさで割り付けてから物理に戻す() {
        let s = Scale::new(1.5);
        let logical = split_viewers(1000.0, 600.0, 80.0, false);
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
        };
        let view = TimeView {
            origin: 0.0,
            px_per_sec: 100.0,
        };
        // 論理の 498（帯の右端 500 の内側 2）は物理の 747。
        let y = (row_top(0) + ROW_H / 2.0) * 1.5;
        let PointerEvent::Move { x, y } = s.pointer_to_logical(PointerEvent::Move { x: 747.0, y })
        else {
            panic!("Move のまま");
        };
        assert_eq!(
            hit_test(&model, &view, x, y),
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

    #[test]
    fn 有限でない倍率や_0_は等倍にする() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(Scale::new(bad), Scale::ONE, "{bad}");
        }
    }
}
