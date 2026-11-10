//! ドラッグ中のツールチップ。編集のドラッグの間、ポインターの横に動かした量（「+12f」「−00:00:01:05」）を出す
//! （Resolve のドラッグ中のフレーム数と同じ）。矩形の幅は文字を測って決めるので、矩形と文字を 1 つの関数で置く。

use std::sync::Arc;

use crate::geom::{Quad, QuadShape, Rect};
use crate::paint::PaintState;
use crate::text::{line_height, place, Align, Label, TextMeasure, TextStyle, UiFont, ELLIPSIS};
use crate::timeline::TimelineModel;
use crate::timeline_layout::Viewport;

/// ポインターと矩形の間隔。矢印のカーソルと左右の矢印のカーソル（`IDC_SIZEWE`）に掛からない幅。
const OFFSET: f32 = 16.0;
/// 矩形の縁と文字の間の余白（横、縦）。
const PAD_X: f32 = 6.0;
const PAD_Y: f32 = 2.0;
/// 負の量の符号（U+2212 MINUS SIGN。ハイフンより幅が数字の + とそろう）。
const MINUS: &str = "\u{2212}";

/// ドラッグ中のツールチップに出すもの。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragTooltip {
    /// ポインターの位置（Timeline の座標）。
    pub at: (f32, f32),
    /// ポインターの向きに動かした量（秒。右が正）。
    pub by: f64,
}

/// 動かした量の文字。1 秒より短ければフレーム数（「+12f」）、1 秒からは時:分:秒:フレーム（「−00:00:01:05」）。
/// ステータスバーのドラッグ中の文（「Ripple Trim +12f」）も同じ形で出す。
pub fn drag_amount(by: f64, fps: f64) -> String {
    let frames = if fps > 0.0 {
        (by * fps).round() as i64
    } else {
        0
    };
    let sign = match frames.signum() {
        1 => "+",
        -1 => MINUS,
        _ => "",
    };
    let n = frames.unsigned_abs();
    let base = fps.round().max(1.0) as u64;
    if n < base {
        return format!("{sign}{n}f");
    }
    let secs = n / base;
    let (h, m, s, f) = (secs / 3600, secs / 60 % 60, secs % 60, n % base);
    format!("{sign}{h:02}:{m:02}:{s:02}:{f:02}")
}

/// ドラッグ中のツールチップ。文字の下に描く矩形を返し、量の文字を `labels` に足す。ドラッグしていなければ空。
/// 文字は 1 回でまとめて描くので、矩形に重なるほかの文字（帯の名前、目盛りの時刻）は矩形の手前で切るか外す。
pub fn drag_tooltip(
    model: &TimelineModel,
    vp: &Viewport,
    state: &PaintState,
    font: &UiFont,
    measure: &mut dyn TextMeasure,
    labels: &mut Vec<Label>,
) -> Vec<Quad> {
    let Some(tooltip) = state.drag_tooltip else {
        return Vec::new();
    };
    let text: Arc<str> = drag_amount(tooltip.by, model.fps).into();
    let size = font.base;
    // 切り上げて、余白を除いた幅が測った幅を下回らないようにする（下回ると切り詰めの「…」になる）。
    let w = (measure.width(&text, size) + 2.0 * PAD_X).ceil();
    let h = line_height(size) + 2.0 * PAD_Y;
    let rect = tooltip_rect(vp.layout.bounds, tooltip.at, (w, h));
    let p = &state.palette;
    let style = TextStyle {
        size,
        color: p.selection,
    };
    let Some(label) = place(&text, rect, PAD_X, Align::Center, style, measure) else {
        return Vec::new();
    };
    clip_labels(labels, rect, measure);
    labels.push(label);
    vec![Quad::new(rect, p.tick), Quad::new(rect.inset(1.0), p.ruler)]
}

/// 矩形の置き場所。ポインターの右（入らなければ左）に、縦はポインターの高さを中央にして、Timeline の中に収める。
fn tooltip_rect(bounds: Rect, (x, y): (f32, f32), (w, h): (f32, f32)) -> Rect {
    let right = x + OFFSET;
    let left = if right + w <= bounds.right() {
        right
    } else {
        x - OFFSET - w
    };
    let top = (y - h / 2.0).min(bounds.bottom() - h).max(bounds.y);
    Rect::new(left.max(bounds.x).round(), top.round(), w, h)
}

/// ツールチップの矩形 `rect` に重なる文字を、矩形の手前で切る。文字の左端（`Label::x`）が矩形の中か右にあれば外す。
fn clip_labels(labels: &mut Vec<Label>, rect: Rect, measure: &mut dyn TextMeasure) {
    labels.retain_mut(|l| {
        // 字を描いてよい矩形が重ならなければ、測らずに残す（帯の名前の多くはここで済む）。
        if !overlaps(l.clip, rect) {
            return true;
        }
        let w = match l.cut {
            Some(cut) => cut + measure.width(ELLIPSIS, l.size),
            None => measure.width(&l.text, l.size),
        };
        if !overlaps(Rect::new(l.x, l.y, w, line_height(l.size)), rect) {
            return true;
        }
        l.clip.w = l.clip.w.min(rect.x - l.clip.x);
        l.x < rect.x && l.clip.w > 0.0
    });
}

/// 文字の上に重ねる矩形（再生位置の線、スナップの表示）から、ドラッグ中のツールチップ `tooltip` に重なる所を除く。
/// 文字は 1 回でまとめて描くのでツールチップを線より上には描けず、線の側を矩形の外だけにしてツールチップを手前に見せる。
pub fn clear_tooltip_area(overlay: Vec<Quad>, tooltip: &[Quad]) -> Vec<Quad> {
    let Some(hole) = tooltip.first().map(|q| q.rect) else {
        return overlay;
    };
    overlay.into_iter().flat_map(|q| outside(q, hole)).collect()
}

/// `q` の、`hole` の外の部分（上、下、左、右の矩形）。三角形などの形は切ると形が変わるので、重なれば描かない。
fn outside(q: Quad, hole: Rect) -> Vec<Quad> {
    if !overlaps(q.rect, hole) {
        return vec![q];
    }
    if q.shape != QuadShape::Rect {
        return Vec::new();
    }
    let r = q.rect;
    let (top, bottom) = (r.y.max(hole.y), r.bottom().min(hole.bottom()));
    [
        Rect::new(r.x, r.y, r.w, hole.y - r.y),
        Rect::new(r.x, hole.bottom(), r.w, r.bottom() - hole.bottom()),
        Rect::new(r.x, top, hole.x - r.x, bottom - top),
        Rect::new(hole.right(), top, r.right() - hole.right(), bottom - top),
    ]
    .into_iter()
    .filter(|p| p.w > 0.0 && p.h > 0.0)
    .map(|rect| Quad { rect, ..q })
    .collect()
}

/// 2 つの矩形が重なるか（辺が接するだけなら重ならない）。
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::tests::state;
    use crate::text::tests::FakeMeasure;
    use crate::timeline_layout::tests::TestViewport;
    use crate::timeline_layout::TOOLBAR_H;

    #[test]
    fn 一秒より短い量はフレーム数_一秒からは時刻() {
        assert_eq!(drag_amount(12.0 / 30.0, 30.0), "+12f");
        assert_eq!(drag_amount(-35.0 / 30.0, 30.0), "\u{2212}00:00:01:05");
        assert_eq!(drag_amount(29.0 / 30.0, 30.0), "+29f", "1 秒の手前");
        assert_eq!(drag_amount(1.0, 30.0), "+00:00:01:00");
        assert_eq!(drag_amount(3725.5, 30.0), "+01:02:05:15");
        assert_eq!(
            drag_amount(0.001, 30.0),
            "0f",
            "動かしていなければ符号を付けない"
        );
    }

    #[test]
    fn 分数の_fps_はフレームの数を丸めた_fps_で時刻にする() {
        // 29.97 fps の 40 フレームは 1 秒と 10 フレーム（30 フレームで 1 秒）。
        assert_eq!(drag_amount(40.0 / 29.97, 29.97), "+00:00:01:10");
        assert_eq!(drag_amount(1.0, 0.0), "0f", "fps が無ければ 0");
    }

    fn model() -> TimelineModel {
        TimelineModel {
            duration: 10.0,
            fps: 30.0,
            ..TimelineModel::default()
        }
    }

    /// 幅 800、高さ 200 の Timeline で、`at` から `by` 秒動かしたドラッグのツールチップ。
    fn tooltip(at: (f32, f32), by: f64, labels: &mut Vec<Label>) -> (Vec<Quad>, FakeMeasure) {
        let m = model();
        let f = TestViewport::new(&m, (800.0, 200.0));
        let st = PaintState {
            drag_tooltip: Some(DragTooltip { at, by }),
            ..state(0.0)
        };
        let mut measure = FakeMeasure::default();
        let font = UiFont::default();
        let quads = drag_tooltip(&m, &f.vp(), &st, &font, &mut measure, labels);
        (quads, measure)
    }

    #[test]
    fn 矩形はポインターの右に置き_量の文字を矩形の中に足す() {
        let mut labels = Vec::new();
        let (quads, _) = tooltip((300.0, 100.0), 0.4, &mut labels);
        assert_eq!(quads.len(), 2, "縁と中");
        let rect = quads[0].rect;
        assert_eq!(rect.x, 300.0 + OFFSET);
        assert!(
            (rect.y + rect.h / 2.0 - 100.0).abs() <= 0.5,
            "縦はポインターの高さ"
        );
        assert_eq!(quads[1].rect, rect.inset(1.0));
        let label = labels.last().expect("量の文字");
        assert_eq!(&*label.text, "+12f");
        assert_eq!((label.clip, label.cut), (rect, None));
        assert!(label.x >= rect.x + PAD_X - 0.5);
    }

    #[test]
    fn 右に入らなければ左に置き_縦は_timeline_の中に収める() {
        let mut labels = Vec::new();
        let (quads, _) = tooltip((790.0, 199.0 + TOOLBAR_H), -2.0, &mut labels);
        let rect = quads[0].rect;
        assert!(rect.right() <= 790.0 - OFFSET + 0.5, "{rect:?}");
        assert!(rect.bottom() <= 200.0 + TOOLBAR_H, "{rect:?}");
        assert_eq!(&*labels[0].text, "\u{2212}00:00:02:00");
    }

    #[test]
    fn 矩形に重なる文字は矩形の手前で切り_左端が矩形の中なら外す() {
        let label = |text: &str, x: f32| Label {
            text: text.into(),
            x,
            y: 94.0,
            size: 12.0,
            color: crate::palette::LABEL,
            cut: None,
            clip: Rect::new(x - 4.0, 90.0, 400.0, 20.0),
        };
        let mut labels = vec![
            label("long bar name", 250.0),
            label("inside", 330.0),
            label("far", 600.0),
        ];
        let (quads, measure) = tooltip((300.0, 100.0), 0.4, &mut labels);
        let rect = quads[0].rect;
        let texts: Vec<&str> = labels.iter().map(|l| &*l.text).collect();
        assert_eq!(texts, ["long bar name", "far", "+12f"]);
        assert_eq!(labels[0].clip.right(), rect.x, "矩形の手前で切る");
        assert_eq!(labels[1].clip.w, 400.0, "重ならない文字はそのまま");
        assert!(
            !measure.asked.iter().any(|t| t == "far"),
            "字を描いてよい矩形が重ならない文字は測らない"
        );
    }

    #[test]
    fn 文字の上に重ねる線はツールチップの矩形の上下で切り_重なる三角形は描かない() {
        let tooltip = [Quad::new(
            Rect::new(100.0, 40.0, 60.0, 20.0),
            crate::palette::TICK,
        )];
        let color = crate::palette::PLAYHEAD;
        let line = Quad::new(Rect::new(120.0, 0.0, 2.0, 200.0), color);
        let away = Quad::new(Rect::new(300.0, 0.0, 2.0, 200.0), color);
        let triangle = Quad::shaped((130.0, 50.0), (11.0, 7.0), QuadShape::TriDown, color);
        let out = clear_tooltip_area(vec![line, away, triangle], &tooltip);
        let rects: Vec<Rect> = out.iter().map(|q| q.rect).collect();
        assert_eq!(
            rects,
            [
                Rect::new(120.0, 0.0, 2.0, 40.0),
                Rect::new(120.0, 60.0, 2.0, 140.0),
                away.rect
            ]
        );
        assert!(out.iter().all(|q| q.color == color));
        assert_eq!(
            clear_tooltip_area(vec![line], &[]),
            [line],
            "ツールチップが無ければそのまま"
        );
    }

    #[test]
    fn ドラッグしていなければ何も出さない() {
        let m = model();
        let f = TestViewport::new(&m, (800.0, 200.0));
        let mut labels = Vec::new();
        let font = UiFont::default();
        let mut measure = FakeMeasure::default();
        let quads = drag_tooltip(&m, &f.vp(), &state(0.0), &font, &mut measure, &mut labels);
        assert!(quads.is_empty() && labels.is_empty());
    }
}
