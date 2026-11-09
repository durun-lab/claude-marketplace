use super::*;
use crate::gesture::Side;
use crate::palette;
use crate::time_view::TimeView;
use crate::timeline::{CutBar, Item, ItemKind};
use crate::timeline_layout::tests::TestViewport;
use crate::timeline_layout::RULER_H;
use crate::zoombar::ZoomBarPart;

/// 幅 800、高さ 120 の Timeline。
const SIZE: (f32, f32) = (800.0, 120.0);

fn model() -> TimelineModel {
    TimelineModel {
        duration: 5.0,
        fps: 30.0,
        items: vec![Item::layer(1.0, 3.0, ItemKind::Footage, true)],
    }
}

pub(crate) fn state(playhead: f64) -> PaintState {
    PaintState {
        playhead,
        hover: Hit::Empty,
        selected: None,
        selected_cut: None,
        guide: None,
        preview: None,
        palette: Palette::DEFAULT,
    }
}

fn quads(m: &TimelineModel, st: &PaintState) -> Vec<Quad> {
    let f = TestViewport::new(m, SIZE);
    timeline_quads(m, &f.vp(), st)
}

/// 同じ高さ Medium の画像のレイヤーを `n` 本。
fn many(n: usize) -> TimelineModel {
    TimelineModel {
        duration: 5.0,
        fps: 30.0,
        items: (0..n)
            .map(|_| Item::layer(0.0, 5.0, ItemKind::Image, true))
            .collect(),
    }
}

#[test]
fn テーマの色で_surface_と帯の段を塗る() {
    let surface = palette::parse_hex("#123456").unwrap();
    let themed = Palette::themed(|name| (name == "surface").then_some(surface));
    let st = PaintState {
        palette: themed,
        ..state(0.0)
    };
    assert_eq!(quads(&model(), &st)[0].color, surface, "surface");
    assert_eq!(
        themed.playhead,
        palette::PLAYHEAD,
        "テーマに無い色は既定のまま"
    );
}

#[test]
fn 再生位置の線は時刻の_x_に立ち_文字の上に重ねる側に入る() {
    let m = model();
    let f = TestViewport::new(&m, SIZE);
    let over = timeline_overlay(&m, &f.vp(), &state(2.0));
    let column = f.layout.time_column();
    let line = over
        .iter()
        .find(|q| q.color == palette::PLAYHEAD && q.rect.h == column.h)
        .expect("再生位置の線");
    assert!((line.rect.x + line.rect.w / 2.0 - 200.0).abs() < 1e-3);
    let under = timeline_quads(&m, &f.vp(), &state(2.0));
    let in_column = |q: &&Quad| q.rect.y < column.bottom();
    assert!(!under
        .iter()
        .filter(in_column)
        .any(|q| q.color == palette::PLAYHEAD));
}

#[test]
fn ズームバーは見えている範囲のスクロールボックスと再生位置の線を描き_乗ると明るくする() {
    let m = model();
    let mut f = TestViewport::new(&m, SIZE);
    // 5 秒のうち 1 秒から 3 秒が見えている。スクロールボックスは x 160 から 480。
    f.time = TimeView {
        origin: 1.0,
        px_per_sec: 400.0,
    };
    let bar = f.layout.zoombar;
    let in_bar = |q: &&Quad| q.rect.y >= bar.y;
    let all = timeline_quads(&m, &f.vp(), &state(4.0));
    let thumb = all
        .iter()
        .filter(in_bar)
        .find(|q| q.color == palette::LABEL)
        .expect("スクロールボックス");
    assert!((thumb.rect.x - 162.0).abs() < 1e-3 && (thumb.rect.right() - 478.0).abs() < 1e-3);
    let line = all
        .iter()
        .filter(in_bar)
        .find(|q| q.color == palette::PLAYHEAD)
        .expect("見えている範囲の外の再生位置");
    assert!((line.rect.x + line.rect.w / 2.0 - 640.0).abs() < 1e-3);
    let lit = PaintState {
        hover: Hit::ZoomBar {
            part: ZoomBarPart::EndEdge,
        },
        ..state(4.0)
    };
    let quads = timeline_quads(&m, &f.vp(), &lit);
    let bright = quads.iter().filter(in_bar);
    assert_eq!(
        bright.filter(|q| q.color == lit.palette.selection).count(),
        1
    );
}

#[test]
fn 時刻の列の外にある再生位置は描かない() {
    let m = model();
    let f = TestViewport::new(&m, SIZE);
    let over = timeline_overlay(&m, &f.vp(), &state(-1.0));
    assert!(over.iter().all(|q| q.rect.x >= 0.0), "{over:?}");
    let far = timeline_overlay(&m, &f.vp(), &state(100.0));
    assert!(far.is_empty(), "{far:?}");
}

#[test]
fn 帯は始まりから終わりの幅で描く() {
    let color = palette::item_color(ItemKind::Footage);
    let bar = quads(&model(), &state(0.0))
        .into_iter()
        .find(|q| q.color == color)
        .expect("レイヤーの帯");
    assert_eq!((bar.rect.x, bar.rect.w), (100.0, 200.0));
    assert_eq!(bar.rect.h, 25.0, "Medium の行の帯");
}

fn bar_of(m: &TimelineModel, preview: Option<(EditKind, f64)>) -> Rect {
    let st = PaintState {
        preview,
        ..state(0.0)
    };
    let color = palette::item_color(m.items[0].kind);
    quads(m, &st)
        .into_iter()
        .find(|q| q.color == color || q.color == color.with_alpha(0.25))
        .expect("レイヤーの帯")
        .rect
}

#[test]
fn ドラッグ中の_trim_layer_は帯の端を動かして描く() {
    let trim = |side, by| Some((EditKind::TrimLayer { item: 0, side }, by));
    let r = bar_of(&model(), trim(Side::In, -0.5));
    assert_eq!((r.x, r.w), (50.0, 250.0), "イン点を前へ");
    let r = bar_of(&model(), trim(Side::Out, -1.0));
    assert_eq!((r.x, r.w), (100.0, 100.0), "アウト点を前へ");
    let other = Some((
        EditKind::TrimLayer {
            item: 1,
            side: Side::In,
        },
        1.0,
    ));
    assert_eq!(
        bar_of(&model(), other).x,
        100.0,
        "別のレイヤーの編集は映さない"
    );
}

fn seq_model() -> TimelineModel {
    let cut = |start, end, gap| CutBar {
        start,
        end,
        gap,
        ..CutBar::default()
    };
    let mut seq = Item::layer(0.0, 5.0, ItemKind::Sequence, true);
    seq.cuts = vec![cut(0.0, 2.0, false), cut(2.0, 3.0, true)];
    TimelineModel {
        duration: 5.0,
        fps: 30.0,
        items: vec![seq],
    }
}

#[test]
fn シーケンスのアウト点を縮めると帯の外のカットは描かない() {
    let trim = Some((
        EditKind::TrimLayer {
            item: 0,
            side: Side::Out,
        },
        -3.5,
    ));
    let r = bar_of(&seq_model(), trim);
    assert_eq!((r.x, r.w), (0.0, 150.0));
    let st = PaintState {
        preview: trim,
        ..state(0.0)
    };
    let all = quads(&seq_model(), &st);
    let base = palette::item_color(ItemKind::Sequence);
    let cut = all
        .iter()
        .find(|q| q.color == base && q.rect.y > RULER_H)
        .expect("カット 0");
    assert_eq!((cut.rect.x, cut.rect.w), (0.0, 150.0), "帯の右端で切る");
    assert!(!all.iter().any(|q| q.color == palette::GAP.with_alpha(0.5)));
}

#[test]
fn シーケンスはカットを並べ_隙間は空きの色で_選んだカットは外枠を付ける() {
    let st = PaintState {
        selected_cut: Some((0, 0)),
        ..state(0.0)
    };
    let all = quads(&seq_model(), &st);
    let base = palette::item_color(ItemKind::Sequence);
    let cut = all
        .iter()
        .find(|q| q.color == base && q.rect.w == 200.0)
        .expect("カット 0 の帯");
    assert_eq!(
        (cut.rect.x, cut.rect.h),
        (0.0, 53.0),
        "sequence の行は Large"
    );
    let gap = all
        .iter()
        .find(|q| q.color == palette::GAP.with_alpha(0.5))
        .expect("隙間");
    assert_eq!((gap.rect.x, gap.rect.w), (200.0, 100.0));
    let outline = all
        .iter()
        .filter(|q| q.color == st.palette.selection && q.rect.x >= 0.0 && q.rect.right() <= 200.5)
        .count();
    assert_eq!(outline, 4, "選んだカットの外枠は 4 辺");
}

#[test]
fn 終わりより右は暗くする() {
    let all = quads(&model(), &state(0.0));
    assert!(all
        .iter()
        .any(|q| q.color == palette::OUTSIDE && q.rect.x == 500.0));
}

#[test]
fn 見えない行は描かない() {
    let m = many(5000);
    let stripes = |st: &[Quad]| {
        st.iter()
            .filter(|q| q.color == palette::ROW_EVEN || q.color == palette::ROW_ODD)
            .count()
    };
    let all = quads(&m, &state(0.0));
    // 行の領域は目盛りの帯とズームバーを除いた 120 - 26 - 14 = 80 px。Medium の行（28 px）は 3 本まで掛かる。
    assert_eq!(stripes(&all), 3);
    assert_eq!(
        all.len(),
        quads(&many(50), &state(0.0)).len(),
        "行の数に依らない"
    );
}

#[test]
fn スクロールで目盛りの帯の下に隠れた行は行の領域の上端で切る() {
    let m = many(50);
    let mut f = TestViewport::new(&m, SIZE);
    f.scroll_y = 14.0;
    let all = timeline_quads(&m, &f.vp(), &state(0.0));
    let area = f.layout.row_area;
    let outside_rows = [palette::RULER, palette::TICK, palette::LABEL];
    let in_rows = |q: &&Quad| !outside_rows.contains(&q.color);
    for q in all.iter().skip(1).filter(in_rows) {
        assert!(
            q.rect.y >= area.y - 1e-3 && q.rect.right() <= area.right() + 1e-3,
            "{q:?}"
        );
    }
    let top = all
        .iter()
        .find(|q| q.color == palette::item_color(ItemKind::Image))
        .expect("行 0 の帯");
    assert_eq!(top.rect.y, area.y, "上の 14 px を切った行 0 の帯");
}

#[test]
fn 中身が入らなければ縦のスクロールバーを描き_入ればスクロールボックスも無い() {
    let thumb = |m: &TimelineModel| {
        let bar = TestViewport::new(m, SIZE).layout.zoombar;
        quads(m, &state(0.0))
            .into_iter()
            .filter(|q| q.color == palette::LABEL && q.rect.bottom() <= bar.y)
            .count()
    };
    assert_eq!(thumb(&many(50)), 1, "スクロールボックス");
    assert_eq!(thumb(&many(2)), 0);
}

#[test]
fn スクロールボックスは溝と見分けられ_乗ると選んだ外枠の色になる() {
    let p = Palette::DEFAULT;
    assert!(palette::contrast(p.label, p.ruler) >= 3.0);
    let m = many(50);
    let st = PaintState {
        hover: Hit::Scrollbar { thumb: true },
        ..state(0.0)
    };
    let lit = quads(&m, &st)
        .into_iter()
        .filter(|q| q.color == st.palette.selection && q.rect.w > 1.0)
        .count();
    assert_eq!(lit, 1);
}
