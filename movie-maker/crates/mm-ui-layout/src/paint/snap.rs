//! スナップの表示。吸い付いたときだけ、時刻の列の全高のガイド線と、目盛りの帯の下端の下向きの三角、行の領域の下端の
//! 上向きの三角を出し（Premiere のスナップの線と同じく上下から挟む）、吸い付いた先が帯の端ならその端を明るくする。

use super::{vline, Canvas, Depth, PaintInput};
use crate::geom::{Quad, QuadShape, Rect};
use crate::gesture::Side;
use crate::snap::SnapKind;
use crate::timeline::HANDLE_PX;

/// ガイド線の太さ。再生位置の線と同じ太さにし、吸い付いた所がはっきり分かるようにする。
const GUIDE_W: f32 = 2.0;
/// 三角の幅と高さ。
const MARK_W: f32 = 11.0;
const MARK_H: f32 = 7.0;
/// 吸い付いた帯の端を塗る不透明度。
const EDGE_ALPHA: f32 = 0.9;

/// ガイド線と上下の三角（文字の上に重ねる）。
pub(super) fn paint(s: &PaintInput, c: &mut Canvas) {
    let Some(target) = s.state.snap else {
        return;
    };
    let color = s.state.palette.guide;
    vline(c, s.vp, target.time, GUIDE_W, color);
    let l = s.vp.layout;
    let x = s.vp.x_of(target.time);
    let half = MARK_H / 2.0;
    let marks = [
        (l.ruler.bottom() - half, QuadShape::TriDown),
        (l.row_area.bottom() - half, QuadShape::TriUp),
    ];
    for (y, shape) in marks {
        let mark = Quad::shaped((x, y), (MARK_W, MARK_H), shape, color);
        c.push_shape(Depth::Surround, mark, l.time_column());
    }
}

/// 吸い付いた帯の端（文字の下、帯の上）。端のつまみと同じ幅を、帯の内側にガイド線の色で塗る。
pub(super) fn edge(s: &PaintInput, c: &mut Canvas) {
    if let Some(strip) = edge_strip(s) {
        let color = s.state.palette.guide.with_alpha(EDGE_ALPHA);
        c.push_in(Depth::OnBar, strip, color, s.vp.layout.row_area);
    }
}

/// 吸い付いた先が見えている帯の端なら、その端に塗る矩形。
fn edge_strip(s: &PaintInput) -> Option<Rect> {
    let (item, side, (start, end)) = match s.state.snap?.kind {
        SnapKind::LayerEdge { item, side } => {
            let it = s.model.items.get(item)?;
            (item, side, (it.start, it.end))
        }
        SnapKind::ClipEdge { item, cut, side } => {
            let bar = s.model.items.get(item)?.cuts.get(cut)?;
            (item, side, (bar.start, bar.end))
        }
        _ => return None,
    };
    if !s.vp.visible_rows().contains(&item) {
        return None;
    }
    let it = &s.model.items[item];
    let layer = s.vp.bar(item, it.start, it.end);
    let bar = s.vp.bar(item, start, end);
    let edge = if side == Side::In { bar.x } else { bar.right() };
    // カットはレイヤーの帯の内側だけを描くので、帯の外に隠れた端は塗らない。
    if edge < layer.x - 0.5 || edge > layer.right() + 0.5 {
        return None;
    }
    let w = HANDLE_PX.min(bar.w / 3.0);
    let x = if side == Side::In { edge } else { edge - w };
    Some(Rect::new(x, bar.y, w, bar.h))
}

#[cfg(test)]
mod tests {
    use super::super::tests::state;
    use super::super::{timeline_overlay, timeline_quads, PaintState};
    use crate::geom::{Quad, QuadShape};
    use crate::gesture::Side;
    use crate::palette;
    use crate::snap::{SnapKind, SnapTarget};
    use crate::timeline::{CutBar, Item, ItemKind, TimelineModel};
    use crate::timeline_layout::tests::TestViewport;
    use crate::timeline_layout::RULER_H;

    /// 幅 800、高さ 120 の Timeline。
    const SIZE: (f32, f32) = (800.0, 120.0);

    /// シーケンス（0-5 秒、カット 0-2、2-3.5）と、画像のレイヤー（1-3 秒）。
    fn model() -> TimelineModel {
        let cut = |start, end| CutBar {
            start,
            end,
            ..CutBar::default()
        };
        let mut seq = Item::layer(0.0, 5.0, ItemKind::Sequence, true);
        seq.cuts = vec![cut(0.0, 2.0), cut(2.0, 3.5)];
        TimelineModel {
            duration: 5.0,
            fps: 30.0,
            items: vec![seq, Item::layer(1.0, 3.0, ItemKind::Image, true)],
            ..TimelineModel::default()
        }
    }

    fn snapped(time: f64, kind: SnapKind) -> PaintState {
        PaintState {
            snap: Some(SnapTarget { time, kind }),
            ..state(0.0)
        }
    }

    fn overlay(st: &PaintState) -> Vec<Quad> {
        let m = model();
        timeline_overlay(&m, &TestViewport::new(&m, SIZE).vp(), st)
    }

    fn guide_quads(quads: &[Quad]) -> Vec<Quad> {
        let guide = palette::GUIDE;
        quads.iter().filter(|q| q.color == guide).copied().collect()
    }

    #[test]
    fn 吸い付いたら太さ_2_で全高のガイド線と上下の三角を出す() {
        let marks = guide_quads(&overlay(&snapped(1.5, SnapKind::Playhead)));
        let line = marks
            .iter()
            .find(|q| q.shape == QuadShape::Rect)
            .expect("ガイド線");
        assert_eq!((line.rect.x, line.rect.w), (149.0, 2.0));
        assert_eq!((line.rect.y, line.rect.h), (0.0, SIZE.1), "時刻の列の全高");
        let down = marks
            .iter()
            .find(|q| q.shape == QuadShape::TriDown)
            .expect("目盛りの帯の三角");
        assert_eq!(down.rect.bottom(), RULER_H, "目盛りの帯の下端から下を指す");
        assert_eq!(down.rect.x + down.rect.w / 2.0, 150.0);
        let up = marks
            .iter()
            .find(|q| q.shape == QuadShape::TriUp)
            .expect("下端の三角");
        assert_eq!(up.rect.bottom(), SIZE.1, "行の領域の下端から上を指す");
    }

    #[test]
    fn 吸い付いていなければスナップの表示を出さない() {
        assert!(guide_quads(&overlay(&state(0.0))).is_empty());
        let m = model();
        let under = timeline_quads(&m, &TestViewport::new(&m, SIZE).vp(), &state(0.0));
        let lit = palette::GUIDE.with_alpha(super::EDGE_ALPHA);
        assert!(!under.iter().any(|q| q.color == lit));
    }

    #[test]
    fn 列の端からはみ出す三角は描かずに線だけ出す() {
        let marks = guide_quads(&overlay(&snapped(0.0, SnapKind::Start)));
        assert!(
            marks.iter().all(|q| q.shape == QuadShape::Rect),
            "{marks:?}"
        );
        assert_eq!(marks.len(), 1, "線の見えている半分");
    }

    fn edge_strip(kind: SnapKind, time: f64) -> Option<Quad> {
        let m = model();
        let f = TestViewport::new(&m, SIZE);
        let lit = palette::GUIDE.with_alpha(super::EDGE_ALPHA);
        timeline_quads(&m, &f.vp(), &snapped(time, kind))
            .into_iter()
            .find(|q| q.color == lit)
    }

    #[test]
    fn 吸い付いた先が帯の端ならその端を帯の内側に明るくする() {
        let layer = SnapKind::LayerEdge {
            item: 1,
            side: Side::Out,
        };
        let strip = edge_strip(layer, 3.0).expect("レイヤーの出");
        let m = model();
        let bar = TestViewport::new(&m, SIZE).vp().bar(1, 1.0, 3.0);
        assert_eq!(strip.rect.right(), bar.right());
        assert_eq!((strip.rect.y, strip.rect.h), (bar.y, bar.h));
        let clip = SnapKind::ClipEdge {
            item: 0,
            cut: 1,
            side: Side::In,
        };
        assert_eq!(edge_strip(clip, 2.0).expect("カットの入り").rect.x, 200.0);
        assert!(
            edge_strip(SnapKind::Marker, 2.0).is_none(),
            "端でなければ塗らない"
        );
    }
}
