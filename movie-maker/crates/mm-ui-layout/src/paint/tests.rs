use super::*;
use crate::gesture::Side;
use crate::palette;
use crate::time_view::TimeView;
use crate::timeline::{CutBar, Item, ItemKind};
use crate::timeline_layout::tests::TestViewport;
use crate::timeline_layout::{HEADER_W, RULER_H};
use crate::zoombar::ZoomBarPart;

/// 幅 800、高さ 120 の Timeline。
const SIZE: (f32, f32) = (800.0, 120.0);

fn model() -> TimelineModel {
    TimelineModel {
        duration: 5.0,
        fps: 30.0,
        items: vec![Item::layer(1.0, 3.0, ItemKind::Footage, true)],
        ..TimelineModel::default()
    }
}

pub(crate) fn state(playhead: f64) -> PaintState {
    PaintState {
        playhead,
        hover: Hit::Empty,
        selected: None,
        selected_cut: None,
        snap: None,
        preview: None,
        drag_tooltip: None,
        header_drop: None,
        keyframe_rows: KeyframeRowState::default(),
        palette: Palette::DEFAULT,
    }
}

/// 行の領域と目盛りの帯の矩形（見出しの列の矩形は除く。背景は含む）。
fn quads(m: &TimelineModel, st: &PaintState) -> Vec<Quad> {
    let f = TestViewport::new(m, SIZE);
    timeline_quads(&PaintInput::new(m, &f.vp(), st))
        .quads
        .into_iter()
        .filter(|q| q.rect.x >= HEADER_W || q.rect.w >= SIZE.0)
        .collect()
}

/// 同じ高さ Medium の画像のレイヤーを `n` 本。
fn many(n: usize) -> TimelineModel {
    TimelineModel {
        duration: 5.0,
        fps: 30.0,
        items: (0..n)
            .map(|_| Item::layer(0.0, 5.0, ItemKind::Image, true))
            .collect(),
        ..TimelineModel::default()
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
    let over = timeline_overlay(&PaintInput::new(&m, &f.vp(), &state(2.0)));
    let column = f.layout.time_column();
    let line = over
        .iter()
        .find(|q| q.color == palette::PLAYHEAD && q.rect.h == column.h)
        .expect("再生位置の線");
    assert!((line.rect.x + line.rect.w / 2.0 - (HEADER_W + 200.0)).abs() < 1e-3);
    let under = timeline_quads(&PaintInput::new(&m, &f.vp(), &state(2.0))).quads;
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
    // 5 秒のうち 1 秒から 3 秒が見えている。スクロールボックスは溝の 1/5 から 3/5（縁から 2 内側）。
    f.time = TimeView {
        origin: 1.0,
        px_per_sec: f64::from(f.layout.row_area.w) / 2.0,
    };
    let bar = f.layout.zoombar;
    let at = |fraction: f32| bar.x + bar.w * fraction;
    let in_bar = |q: &&Quad| q.rect.y >= bar.y;
    let all = timeline_quads(&PaintInput::new(&m, &f.vp(), &state(4.0))).quads;
    let thumb = all
        .iter()
        .filter(in_bar)
        .find(|q| q.color == palette::LABEL)
        .expect("スクロールボックス");
    assert!((thumb.rect.x - (at(0.2) + 2.0)).abs() < 1e-3);
    assert!((thumb.rect.right() - (at(0.6) - 2.0)).abs() < 1e-3);
    let line = all
        .iter()
        .filter(in_bar)
        .find(|q| q.color == palette::PLAYHEAD)
        .expect("見えている範囲の外の再生位置");
    assert!((line.rect.x + line.rect.w / 2.0 - at(0.8)).abs() < 1e-3);
    let hovered = PaintState {
        hover: Hit::ZoomBar {
            part: ZoomBarPart::EndEdge,
        },
        ..state(4.0)
    };
    let quads = timeline_quads(&PaintInput::new(&m, &f.vp(), &hovered)).quads;
    let bright = quads.iter().filter(in_bar);
    assert_eq!(
        bright
            .filter(|q| q.color == hovered.palette.selection)
            .count(),
        1
    );
}

#[test]
fn 時刻の列の外にある再生位置は描かない() {
    let m = model();
    let f = TestViewport::new(&m, SIZE);
    let over = timeline_overlay(&PaintInput::new(&m, &f.vp(), &state(-1.0)));
    assert!(over.iter().all(|q| q.rect.x >= HEADER_W), "{over:?}");
    let far = timeline_overlay(&PaintInput::new(&m, &f.vp(), &state(100.0)));
    assert!(far.is_empty(), "{far:?}");
}

#[test]
fn 帯は始まりから終わりの幅で描く() {
    let color = palette::item_color(ItemKind::Footage);
    let bar = quads(&model(), &state(0.0))
        .into_iter()
        .find(|q| q.color == color)
        .expect("レイヤーの帯");
    assert_eq!((bar.rect.x, bar.rect.w), (HEADER_W + 100.0, 200.0));
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
    assert_eq!((r.x, r.w), (HEADER_W + 50.0, 250.0), "イン点を前へ");
    let r = bar_of(&model(), trim(Side::Out, -1.0));
    assert_eq!((r.x, r.w), (HEADER_W + 100.0, 100.0), "アウト点を前へ");
    let other = Some((
        EditKind::TrimLayer {
            item: 1,
            side: Side::In,
        },
        1.0,
    ));
    assert_eq!(
        bar_of(&model(), other).x,
        HEADER_W + 100.0,
        "別のレイヤーの編集は映さない"
    );
}

#[test]
fn ドラッグ中の_shift_layer_は帯を同じ幅のまま動かして描く() {
    let shift = |item, by| Some((EditKind::ShiftLayer { item }, by));
    let r = bar_of(&model(), shift(0, 0.5));
    assert_eq!((r.x, r.w), (HEADER_W + 150.0, 200.0), "後ろへ");
    let r = bar_of(&model(), shift(0, -1.0));
    assert_eq!((r.x, r.w), (HEADER_W, 200.0), "前へ");
    assert_eq!(bar_of(&model(), shift(1, 1.0)).x, HEADER_W + 100.0);
}

/// Blade の線 `blade_line` を、再生位置 `playhead` と一緒に描いた文字の上の矩形。
fn blade_overlay(blade_line: Option<crate::tool::BladeLine>, playhead: f64) -> Vec<Quad> {
    let m = model();
    let f = TestViewport::new(&m, SIZE);
    let st = state(playhead);
    let vp = f.vp();
    timeline_overlay(&PaintInput {
        blade_line,
        ..PaintInput::new(&m, &vp, &st)
    })
}

#[test]
fn blade_の線は指したレイヤーの帯の高さで割る時刻に立ち_再生位置と見分けられる色() {
    use crate::tool::BladeLine;
    let at = |time, item| BladeLine {
        item,
        time,
        snapped: false,
    };
    let text = Palette::DEFAULT.selection;
    let lines = |blade_line| -> Vec<Quad> {
        let over = blade_overlay(blade_line, 0.0);
        over.into_iter().filter(|q| q.color == text).collect()
    };
    assert!(lines(None).is_empty());
    let line = lines(Some(at(2.0, 0)));
    assert_eq!(line.len(), 1, "吸い付いていない線はテーマの text の色");
    let m = model();
    let bar = TestViewport::new(&m, SIZE).vp().bar(0, 1.0, 3.0);
    let r = line[0].rect;
    assert_eq!(r.x + r.w / 2.0, HEADER_W + 200.0, "2 秒の位置");
    assert!(
        r.y < bar.y && r.bottom() > bar.bottom(),
        "帯の上下に少し出る"
    );
    assert!(lines(Some(at(2.0, 9))).is_empty());
}

#[test]
fn 再生位置に吸い付いた_blade_の線はガイド線の色で再生位置の線より上に描く() {
    use crate::tool::BladeLine;
    let line = BladeLine {
        item: 0,
        time: 2.0,
        snapped: true,
    };
    let over = blade_overlay(Some(line), 2.0);
    let last = over.last().expect("線");
    assert_eq!(last.color, palette::GUIDE, "いちばん上に描く");
    assert_eq!(last.rect.x + last.rect.w / 2.0, HEADER_W + 200.0);
    let playhead = over.iter().position(|q| q.color == palette::PLAYHEAD);
    assert!(playhead.is_some_and(|i| i + 1 < over.len()));
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
        ..TimelineModel::default()
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
    assert_eq!((r.x, r.w), (HEADER_W, 150.0));
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
    assert_eq!(
        (cut.rect.x, cut.rect.w),
        (HEADER_W, 150.0),
        "帯の右端で切る"
    );
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
        (HEADER_W, 53.0),
        "sequence の行は Large"
    );
    let gap = all
        .iter()
        .find(|q| q.color == palette::GAP.with_alpha(0.5))
        .expect("隙間");
    assert_eq!((gap.rect.x, gap.rect.w), (HEADER_W + 200.0, 100.0));
    let outline = all
        .iter()
        .filter(|q| {
            q.color == st.palette.selection
                && q.rect.x >= HEADER_W
                && q.rect.right() <= HEADER_W + 200.5
        })
        .count();
    assert_eq!(outline, 4, "選んだカットの外枠は 4 辺");
}

#[test]
fn 終わりより右は暗くする() {
    let all = quads(&model(), &state(0.0));
    assert!(all
        .iter()
        .any(|q| q.color == palette::OUTSIDE && q.rect.x == HEADER_W + 500.0));
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
    let all = timeline_quads(&PaintInput::new(&m, &f.vp(), &state(0.0))).quads;
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

#[test]
fn 長さ_1_フレームの帯は最小の幅で描き_再生位置の線の外に残す() {
    let mut m = model();
    m.items = vec![Item::layer(0.0, 1.0 / 30.0, ItemKind::Sequence, true)];
    let f = TestViewport::new(&m, SIZE);
    let bar = f.vp().bar(0, 0.0, 1.0 / 30.0);
    assert_eq!(
        bar.w,
        crate::timeline::MIN_BAR_PX,
        "描く幅は当たり判定と同じ"
    );
    let at = |x: f32| crate::timeline::hit_test(&m, &f.vp(), x, bar.y + 5.0);
    assert!(matches!(at(bar.right() - 1.0), Hit::Layer { .. }));
    assert!(!matches!(at(bar.right() + 1.0), Hit::Layer { .. }));
    let playhead_half = 1.0;
    assert!(bar.w >= playhead_half * 2.0 + 2.0, "線の右に 2 px 以上残る");
}

#[test]
fn カットの無い_sequence_のレイヤーは薄い塗りに種類の色の枠を引いた空の箱で描く() {
    let mut m = model();
    m.items = vec![Item::layer(0.0, 1.0 / 30.0, ItemKind::Sequence, true)];
    let color = palette::item_color(ItemKind::Sequence);
    let q = quads(&m, &state(2.0));
    let fill = color.with_alpha(rows::EMPTY_FILL_ALPHA);
    let edge = palette::lighten(color, rows::EMPTY_EDGE_LIGHTEN);
    assert!(q.iter().any(|q| q.color == fill), "薄い塗り");
    assert!(q.iter().any(|q| q.color == edge), "明るい枠");
    assert!(q.iter().all(|q| q.color != color), "べた塗りの帯は無い");
}
