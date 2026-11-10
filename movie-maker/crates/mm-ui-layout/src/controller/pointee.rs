//! ポインターの下にあるものの、Timeline が知っている分（[`Pointee`]）。カーソル、ステータスバー、ツールチップ、
//! 右クリックのメニューが同じ値を引く。ツールバーと再生の操作のバーの部品は `Ui` が `under` に差し替える。

use super::{Drag, TimelineController};
use crate::hint::{Dragging, EditDrag, Pointee, Under};
use crate::timeline::{Hit, TimelineModel};
use crate::tool::Tool;

impl TimelineController {
    /// 編集のドラッグ中の様子。押しただけで動かしていなければ量は `None`。編集ではないドラッグ（スクラブ、スクロール）は `None`。
    fn editing(&self) -> Option<Dragging> {
        let kind = match self.drag {
            Drag::Edge { cut: None, .. } => EditDrag::TrimLayer,
            Drag::Edge { mods, .. } if mods.alt => EditDrag::Roll,
            Drag::Edge { .. } => EditDrag::Ripple,
            Drag::Body { mods, .. } if mods.shift => EditDrag::Slide,
            Drag::Body { .. } => EditDrag::Slip,
            Drag::ShiftLayer { .. } => EditDrag::MoveInTime,
            _ => return None,
        };
        Some(Dragging {
            kind,
            by: self.drag_tooltip().map(|t| t.by),
            snap: self.snap.map(|s| s.message()),
        })
    }

    /// ポインターの下の Timeline の部品と、ツール、ドラッグの様子。
    pub fn pointee(&self, model: &TimelineModel) -> Pointee {
        let gap = match self.hover {
            Hit::Cut { item, cut, .. } => model
                .items
                .get(item)
                .and_then(|it| it.cuts.get(cut))
                .is_some_and(|bar| bar.gap),
            _ => false,
        };
        Pointee {
            under: Under::Timeline(self.hover),
            tool: self.tool,
            dragging: self.editing(),
            gap,
            blade_ready: self.tool == Tool::Blade && self.blade_line.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{Mods, PointerEvent};
    use crate::time_view::TimeView;
    use crate::timeline::{CutBar, Item, ItemKind, Part};
    use crate::timeline_layout::tests::TestViewport;

    fn model() -> TimelineModel {
        let mut seq = Item::layer(0.0, 6.0, ItemKind::Sequence, true);
        seq.cuts = vec![
            CutBar {
                start: 0.0,
                end: 3.0,
                ..CutBar::default()
            },
            CutBar {
                start: 3.0,
                end: 6.0,
                gap: true,
                ..CutBar::default()
            },
        ];
        TimelineModel {
            duration: 10.0,
            fps: 30.0,
            items: vec![seq],
            ..TimelineModel::default()
        }
    }

    fn controller(m: &TimelineModel) -> (TimelineController, TestViewport) {
        let f = TestViewport::new(m, (800.0, 200.0));
        let mut c = TimelineController::new(TimeView {
            origin: 0.0,
            px_per_sec: 100.0,
        });
        c.set_rows(f.rows.clone());
        c.resize((800.0, 200.0 + 30.0 + crate::timeline_layout::STATUS_H));
        (c, f)
    }

    #[test]
    fn ホバーしたカットの隙間と_blade_の割れる位置を持つ() {
        let m = model();
        let (mut c, f) = controller(&m);
        let y = f.in_bar(0);
        let x = f.vp().x_of(1.5);
        c.handle(PointerEvent::Move { x, y }, &m, 0.0);
        let p = c.pointee(&m);
        assert!(matches!(
            p.under,
            Under::Timeline(Hit::Cut {
                cut: 0,
                part: Part::Body,
                ..
            })
        ));
        assert!(!p.gap && !p.blade_ready);
        c.tool = Tool::Blade;
        c.handle(PointerEvent::Move { x, y }, &m, 0.0);
        assert!(c.pointee(&m).blade_ready, "割れるカットの上");
        let gap_x = f.vp().x_of(4.5);
        c.handle(PointerEvent::Move { x: gap_x, y }, &m, 0.0);
        let p = c.pointee(&m);
        assert!(p.gap && !p.blade_ready, "隙間は割れない");
    }

    #[test]
    fn カットの端のドラッグは押した修飾キーで_ripple_と_roll_になり_動かすと量が付く() {
        let m = model();
        let (mut c, f) = controller(&m);
        let y = f.in_bar(0);
        let edge = f.vp().x_of(3.0) - 2.0;
        let alt = Mods {
            shift: false,
            alt: true,
        };
        c.handle(PointerEvent::Move { x: edge, y }, &m, 0.0);
        c.handle(
            PointerEvent::Down {
                x: edge,
                y,
                mods: Mods::default(),
            },
            &m,
            0.0,
        );
        let d = c.pointee(&m).dragging.expect("ドラッグ中");
        assert_eq!((d.kind, d.by), (EditDrag::Ripple, None));
        c.handle(PointerEvent::Move { x: edge - 30.0, y }, &m, 0.0);
        let d = c.pointee(&m).dragging.expect("ドラッグ中");
        assert!(d.by.is_some_and(|by| by < 0.0), "{d:?}");
        c.handle(PointerEvent::Up, &m, 0.0);
        assert_eq!(c.pointee(&m).dragging, None);
        c.handle(
            PointerEvent::Down {
                x: edge,
                y,
                mods: alt,
            },
            &m,
            0.0,
        );
        assert_eq!(c.pointee(&m).dragging.map(|d| d.kind), Some(EditDrag::Roll));
    }
}
