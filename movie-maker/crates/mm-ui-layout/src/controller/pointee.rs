//! ポインターの下にあるものの、Timeline が知っている分（[`Pointee`]）。カーソル、ステータスバー、ツールチップ、
//! 右クリックのメニューが同じ値を引く。ツールバーと再生の操作のバーの部品は `Ui` が `under` に差し替える。

use super::{Drag, TimelineController};
use crate::hint::{Dragging, EditDrag, Pointee, Under};
use crate::timeline::{Hit, TimelineModel};
use crate::tool::Tool;

impl TimelineController {
    /// 編集のドラッグ中の様子。押しただけで動かしていなければ量は `None`。編集ではないドラッグ（スクラブ、スクロール）と、
    /// 何も変えないドラッグ（隙間の中の slip）は `None`。
    fn editing(&self, model: &TimelineModel) -> Option<Dragging> {
        let kind = match self.drag {
            Drag::Edge { cut: None, .. } => EditDrag::TrimLayer,
            Drag::Edge { mods, .. } if mods.alt => EditDrag::Roll,
            Drag::Edge { .. } => EditDrag::Ripple,
            Drag::Body { mods, .. } if mods.shift => EditDrag::Slide,
            Drag::Body { item, cut, .. } if is_gap(model, item, cut) => return None,
            Drag::Body { .. } => EditDrag::Slip,
            Drag::ShiftLayer { .. } => EditDrag::MoveInTime,
            Drag::Keyframe(_) => EditDrag::MoveKeyframe,
            _ => return None,
        };
        let moved = self.drag_tooltip().or(self.keyframe_tooltip());
        Some(Dragging {
            kind,
            by: moved.map(|t| t.by),
            snap: self.snap.map(|s| s.message()),
        })
    }

    /// ポインターの下の Timeline の部品と、ツール、ドラッグの様子。`playhead` は Blade の割る時刻が吸い付く再生位置。
    pub fn pointee(&self, model: &TimelineModel, playhead: f64) -> Pointee {
        let (gap, blade_ready) = match self.hover {
            // 押すと Blade の線は消えるので、割れるかは今のホバーとポインターの位置から求め直す。
            Hit::Cut { item, cut, .. } => (
                is_gap(model, item, cut),
                self.tool == Tool::Blade
                    && self.drag == Drag::None
                    && self
                        .blade_at((item, cut), self.pointer.0, model, playhead)
                        .is_some(),
            ),
            _ => (false, false),
        };
        Pointee {
            under: Under::Timeline(self.hover),
            tool: self.tool,
            dragging: self.editing(model),
            gap,
            blade_ready,
        }
    }
}

/// レイヤー `item` のカット `cut` が隙間か。
fn is_gap(model: &TimelineModel, item: usize, cut: usize) -> bool {
    model
        .items
        .get(item)
        .and_then(|it| it.cuts.get(cut))
        .is_some_and(|bar| bar.gap)
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
        let p = c.pointee(&m, 0.0);
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
        assert!(c.pointee(&m, 0.0).blade_ready, "割れるカットの上");
        let mods = Mods::default();
        c.handle(PointerEvent::Down { x, y, mods }, &m, 0.0);
        c.handle(PointerEvent::Up, &m, 0.0);
        assert!(
            c.pointee(&m, 0.0).blade_ready,
            "押して線が消えても、動かす前と同じく割れる"
        );
        let gap_x = f.vp().x_of(4.5);
        c.handle(PointerEvent::Move { x: gap_x, y }, &m, 0.0);
        let p = c.pointee(&m, 0.0);
        assert!(p.gap && !p.blade_ready, "隙間は割れない");
    }

    #[test]
    fn ステータスバーを押しても選択は変わらない() {
        let m = model();
        let (mut c, f) = controller(&m);
        let (x, y) = (f.vp().x_of(1.5), f.in_bar(0));
        let mods = Mods::default();
        c.handle(PointerEvent::Down { x, y, mods }, &m, 0.0);
        c.handle(PointerEvent::Up, &m, 0.0);
        assert_eq!(c.selected, Some(0));
        let bar = c.layout().status;
        assert!(bar.h > 0.0, "{bar:?}");
        let (x, y) = (bar.x + 10.0, bar.y + bar.h / 2.0);
        let out = c.handle(PointerEvent::Down { x, y, mods }, &m, 0.0);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(c.selected, Some(0), "選択を外さない");
    }

    #[test]
    fn trim_の隙間の中のドラッグは_slide_だけを編集のドラッグにする() {
        let m = model();
        let (mut c, f) = controller(&m);
        c.tool = Tool::Trim;
        let (x, y) = (f.vp().x_of(4.5), f.in_bar(0));
        let shift = Mods {
            shift: true,
            alt: false,
        };
        c.handle(PointerEvent::Move { x, y }, &m, 0.0);
        let mods = Mods::default();
        c.handle(PointerEvent::Down { x, y, mods }, &m, 0.0);
        assert_eq!(c.pointee(&m, 0.0).dragging, None, "隙間は slip しない");
        c.handle(PointerEvent::Cancel, &m, 0.0);
        c.handle(PointerEvent::Down { x, y, mods: shift }, &m, 0.0);
        let kind = c.pointee(&m, 0.0).dragging.map(|d| d.kind);
        assert_eq!(kind, Some(EditDrag::Slide));
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
        let d = c.pointee(&m, 0.0).dragging.expect("ドラッグ中");
        assert_eq!((d.kind, d.by), (EditDrag::Ripple, None));
        c.handle(PointerEvent::Move { x: edge - 30.0, y }, &m, 0.0);
        let d = c.pointee(&m, 0.0).dragging.expect("ドラッグ中");
        assert!(d.by.is_some_and(|by| by < 0.0), "{d:?}");
        c.handle(PointerEvent::Up, &m, 0.0);
        assert_eq!(c.pointee(&m, 0.0).dragging, None);
        c.handle(
            PointerEvent::Down {
                x: edge,
                y,
                mods: alt,
            },
            &m,
            0.0,
        );
        assert_eq!(
            c.pointee(&m, 0.0).dragging.map(|d| d.kind),
            Some(EditDrag::Roll)
        );
    }
}
