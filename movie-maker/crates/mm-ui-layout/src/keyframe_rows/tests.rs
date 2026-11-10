use super::*;
use crate::geom::{Quad, QuadShape};
use crate::header::disclosure_center;
use crate::paint::tests::state;
use crate::paint::{timeline_quads, PaintInput};
use crate::rows::{RowHeights, Rows};
use crate::text::tests::FakeMeasure;
use crate::time_view::TimeView;
use crate::timeline::{hit_test, ItemKind};
use crate::timeline_layout::TimelineLayout;

const SIZE: (f32, f32) = (1000.0, 300.0);

pub(crate) fn row(name: &str, times: &[f64]) -> KeyframeRow {
    KeyframeRow {
        name: name.into(),
        keyframes: times
            .iter()
            .map(|&time| RowKeyframe {
                time,
                min: 0.0,
                max: 9.9,
            })
            .collect(),
    }
}

/// 上に title（キーフレームの行が 2 本）、下に bg（キーフレーム無し）。長さ 10 秒、30 fps。
pub(crate) fn model() -> TimelineModel {
    let title = Item::layer(0.0, 8.0, ItemKind::Text, true)
        .with_id("title")
        .with_keyframes(&[1.0, 2.0, 4.0])
        .with_keyframe_rows(vec![
            row("Opacity", &[1.0, 4.0]),
            row("Transform › Position", &[2.0]),
        ]);
    let bg = Item::layer(0.0, 8.0, ItemKind::Solid, true).with_id("bg");
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![title, bg],
        ..TimelineModel::default()
    }
}

/// title のキーフレームの行を開いた並びと割り付け。
fn opened(m: &TimelineModel) -> (TimelineLayout, Rows) {
    let mut heights = RowHeights::default();
    heights.keyframe_rows_open.insert("title".into());
    let rows = Rows::of(&m.items, &heights);
    (TimelineLayout::new(SIZE, rows.content_height()), rows)
}

fn vp<'a>(layout: &'a TimelineLayout, rows: &'a Rows) -> Viewport<'a> {
    Viewport {
        layout,
        rows,
        time: TimeView {
            origin: 0.0,
            px_per_sec: 100.0,
        },
        scroll_y: 0.0,
    }
}

#[test]
fn キーフレームの行はレイヤーの行の下に並び_次のレイヤーの行を押し下げる() {
    let m = model();
    let (layout, rows) = opened(&m);
    let vp = vp(&layout, &rows);
    let first = row_rect(&vp, 0, 0);
    assert_eq!(first.y, vp.row(0).bottom());
    assert_eq!((first.x, first.w), (layout.row_area.x, layout.row_area.w));
    assert_eq!(row_rect(&vp, 0, 1).y, first.bottom());
    assert_eq!(vp.row(1).y, row_rect(&vp, 0, 1).bottom());
    let head = header_rect(&vp, 0, 1);
    assert_eq!(
        (head.x, head.w, head.y),
        (0.0, layout.headers.w, first.bottom())
    );
}

#[test]
fn ひし形はそのキーフレーム_無い所は行_見出しの列は行の見出しに当たる() {
    let m = model();
    let (layout, rows) = opened(&m);
    let vp = vp(&layout, &rows);
    let (x, y) = keyframe_center(&vp, (0, 0), 4.0);
    let key = KeyframeIndex {
        item: 0,
        row: 0,
        keyframe: 1,
    };
    assert_eq!(hit_test(&m, &vp, x, y), Hit::RowKeyframe(key));
    assert_eq!(
        hit_test(&m, &vp, x + ROW_DIAMOND_PX / 2.0 + 1.5, y),
        Hit::RowKeyframe(key),
        "ひし形の外接する正方形より 2 px 広く掴める"
    );
    assert_eq!(
        hit_test(&m, &vp, vp.x_of(3.0), y),
        Hit::KeyframeRow { item: 0, row: 0 }
    );
    let (_, y1) = keyframe_center(&vp, (0, 1), 2.0);
    assert_eq!(
        hit_test(&m, &vp, 40.0, y1),
        Hit::KeyframeRowHeader { item: 0, row: 1 }
    );
}

#[test]
fn 見出しのキーフレームの開閉はキーフレームのあるレイヤーだけ() {
    let m = model();
    let (layout, rows) = opened(&m);
    let vp = vp(&layout, &rows);
    let (x, y) = disclosure_center(&vp, &m.items[0], 0);
    assert_eq!(hit_test(&m, &vp, x, y), Hit::Disclosure { index: 0 });
    let (x, y) = disclosure_center(&vp, &m.items[1], 1);
    assert_eq!(hit_test(&m, &vp, x, y), Hit::Header { index: 1 });
}

#[test]
fn 開閉は_1_つのレイヤーか_キーフレームの行のある全部のレイヤー() {
    let m = model();
    let mut open = BTreeSet::new();
    assert!(toggle_open(&mut open, &m.items, Some(0)));
    assert!(open.contains("title"));
    assert!(!toggle_open(&mut open, &m.items, Some(0)));
    assert!(open.is_empty());
    assert!(
        !toggle_open(&mut open, &m.items, Some(9)),
        "無い行は何もしない"
    );
    assert!(
        toggle_open(&mut open, &m.items, None),
        "閉じたものがあれば全部を開く"
    );
    assert_eq!(
        open.iter().collect::<Vec<_>>(),
        ["title"],
        "行の無い bg は開かない"
    );
    assert!(
        !toggle_open(&mut open, &m.items, None),
        "全部開いていれば全部を閉じる"
    );
    assert!(open.is_empty());
}

#[test]
fn 行の名前はレイヤーの名前と左をそろえ_選んだ行は選択の色() {
    let m = model();
    let (layout, rows) = opened(&m);
    let vp = vp(&layout, &rows);
    let st = PaintState {
        keyframe_rows: KeyframeRowState {
            selected: Some(RowPick {
                item: 0,
                row: 1,
                keyframe: None,
            }),
            dragging: None,
        },
        ..state(0.0)
    };
    let font = UiFont::default();
    let labels = labels(&m, &vp, &st, &font, &mut FakeMeasure::default());
    let names: Vec<&str> = labels.iter().map(|l| &*l.text).collect();
    assert_eq!(names, ["Opacity", "Transform › Position"]);
    let name_x = header::cells(header::row_rect(&vp, 0), &m.items[0]).name.x;
    assert!(labels.iter().all(|l| l.x == name_x && l.size == font.small));
    let p = st.palette;
    assert_eq!((labels[0].color, labels[1].color), (p.label, p.selection));
    assert!(labels[1].clip.y >= header_rect(&vp, 0, 1).y);
}

#[test]
fn 閉じたレイヤーの行の名前は置かない() {
    let m = model();
    let rows = Rows::of(&m.items, &RowHeights::default());
    let layout = TimelineLayout::new(SIZE, rows.content_height());
    let vp = vp(&layout, &rows);
    let font = UiFont::default();
    let labels = labels(&m, &vp, &state(0.0), &font, &mut FakeMeasure::default());
    assert!(labels.is_empty());
}

fn diamonds(quads: &[Quad], size: f32) -> Vec<Quad> {
    quads
        .iter()
        .filter(|q| q.shape == QuadShape::Diamond && q.rect.w == size)
        .copied()
        .collect()
}

fn center(q: &Quad) -> (f32, f32) {
    (q.rect.x + q.rect.w / 2.0, q.rect.y + q.rect.h / 2.0)
}

#[test]
fn 開いた行にひし形を描き_選んだものは_accent_の輪で囲む() {
    let m = model();
    let (layout, rows) = opened(&m);
    let vp = vp(&layout, &rows);
    let picked = RowPick {
        item: 0,
        row: 0,
        keyframe: Some(1),
    };
    let st = PaintState {
        keyframe_rows: KeyframeRowState {
            selected: Some(picked),
            dragging: None,
        },
        ..state(0.0)
    };
    let quads = timeline_quads(&PaintInput::new(&m, &vp, &st)).quads;
    let rims: Vec<(f32, f32)> = diamonds(&quads, ROW_DIAMOND_PX)
        .iter()
        .map(center)
        .collect();
    let want = [
        keyframe_center(&vp, (0, 0), 1.0),
        keyframe_center(&vp, (0, 0), 4.0),
        keyframe_center(&vp, (0, 1), 2.0),
    ];
    assert_eq!(rims.len(), 3);
    assert!(want.iter().all(|c| rims.contains(c)), "{rims:?}");
    let rings = diamonds(&quads, ROW_DIAMOND_PX + 4.0);
    assert_eq!(rings.len(), 1);
    assert_eq!(
        (center(&rings[0]), rings[0].color),
        (want[1], st.palette.accent)
    );
}

#[test]
fn ドラッグ中のキーフレームは動かした先に描き_元の位置には描かない() {
    let m = model();
    let (layout, rows) = opened(&m);
    let vp = vp(&layout, &rows);
    let moving = KeyframeIndex {
        item: 0,
        row: 0,
        keyframe: 0,
    };
    let st = PaintState {
        keyframe_rows: KeyframeRowState {
            selected: None,
            dragging: Some((moving, 2.5)),
        },
        ..state(0.0)
    };
    let quads = timeline_quads(&PaintInput::new(&m, &vp, &st)).quads;
    let rims: Vec<(f32, f32)> = diamonds(&quads, ROW_DIAMOND_PX)
        .iter()
        .map(center)
        .collect();
    assert!(!rims.contains(&keyframe_center(&vp, (0, 0), 1.0)));
    let rings: Vec<(f32, f32)> = diamonds(&quads, ROW_DIAMOND_PX + 4.0)
        .iter()
        .map(center)
        .collect();
    assert_eq!(rings, [keyframe_center(&vp, (0, 0), 2.5)]);
}

#[test]
fn 行の領域に収まらないひし形は縦線にして切る() {
    let mut m = model();
    // 行の領域は 180 から 1000（8.2 秒）。8.18 秒のひし形は右端からはみ出す。
    m.items[0].keyframe_rows[1] = row("Transform › Position", &[8.18]);
    let (layout, rows) = opened(&m);
    let vp = vp(&layout, &rows);
    let quads = timeline_quads(&PaintInput::new(&m, &vp, &state(0.0))).quads;
    let (x, _) = keyframe_center(&vp, (0, 1), 8.18);
    assert!(diamonds(&quads, ROW_DIAMOND_PX)
        .iter()
        .all(|q| center(q).0 != x));
    let line = quads.iter().find(|q| {
        q.shape == QuadShape::Rect && q.rect.w == 2.0 && (q.rect.x - (x - 1.0)).abs() < 1e-3
    });
    assert!(line.is_some_and(|q| q.rect.right() <= layout.row_area.right()));
}

#[test]
fn 開いたレイヤーの見出しの開閉は下向きの三角() {
    let m = model();
    let shapes = |open: bool| {
        let mut heights = RowHeights::default();
        if open {
            heights.keyframe_rows_open.insert("title".into());
        }
        let rows = Rows::of(&m.items, &heights);
        let layout = TimelineLayout::new(SIZE, rows.content_height());
        let quads = timeline_quads(&PaintInput::new(&m, &vp(&layout, &rows), &state(0.0))).quads;
        let tri = |s: QuadShape| quads.iter().filter(|q| q.shape == s).count();
        (tri(QuadShape::TriRight), tri(QuadShape::TriDown))
    };
    assert_eq!(shapes(false), (1, 0));
    assert_eq!(shapes(true), (0, 1));
}
