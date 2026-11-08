//! コメントの描き込みを Program の上に重ねる計算。0〜1 に正規化した描き込みの座標を、
//! Program に映っている映像の矩形（レターボックスを除く）のサーフェスの座標へ写し、太さつきの線分に落とす。

use crate::geom::{fit_rect, Rect};

/// 線の太さ（物理ピクセル）。映像の拡大に依らず画面の上で一定にする。
pub const MARK_LINE_PX: f32 = 3.0;
/// 点の印の直径（物理ピクセル）。線より太くし、1 点でも見つけられるようにする。
pub const MARK_POINT_PX: f32 = 12.0;

/// 描き込みの種類。`Stroke` は点の並び、`Rect` は対角の 2 点、`Point` は 1 点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Stroke,
    Rect,
    Point,
}

/// 描き込み 1 つ。座標はフレームの左上を (0, 0)、右下を (1, 1) に正規化する。
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub kind: MarkKind,
    pub points: Vec<[f32; 2]>,
}

/// 描く線分 1 本（サーフェスの座標）。端は丸め、長さ 0 は直径 `width` の点になる。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkSegment {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub width: f32,
}

/// 正規化した座標を映像の矩形の点へ写す。外れた値は映像の縁に寄せる。
pub fn frame_point(frame: Rect, p: [f32; 2]) -> [f32; 2] {
    let [u, v] = p.map(|c| c.clamp(0.0, 1.0));
    [frame.x + u * frame.w, frame.y + v * frame.h]
}

/// 描き込みを線分に落とす。`image` は Program の画像の大きさ、`area` は Program の領域で、
/// 映像は `area` の中に縦横比を保って収まる（[`fit_rect`]）。
pub fn mark_segments(marks: &[Mark], image: (f32, f32), area: Rect) -> Vec<MarkSegment> {
    let frame = fit_rect(image.0, image.1, area);
    if frame.w <= 0.0 || frame.h <= 0.0 {
        return Vec::new();
    }
    marks.iter().flat_map(|m| segments_of(m, frame)).collect()
}

fn segments_of(mark: &Mark, frame: Rect) -> Vec<MarkSegment> {
    let points: Vec<[f32; 2]> = mark
        .points
        .iter()
        .filter(|p| p.iter().all(|c| c.is_finite()))
        .map(|p| frame_point(frame, *p))
        .collect();
    let line = |a: [f32; 2], b: [f32; 2]| MarkSegment {
        a,
        b,
        width: MARK_LINE_PX,
    };
    match (mark.kind, points.as_slice()) {
        (_, []) => Vec::new(),
        (MarkKind::Point, [p, ..]) => vec![MarkSegment {
            a: *p,
            b: *p,
            width: MARK_POINT_PX,
        }],
        (MarkKind::Rect, [a, b, ..]) => {
            let corners = [*a, [b[0], a[1]], *b, [a[0], b[1]]];
            (0..4)
                .map(|i| line(corners[i], corners[(i + 1) % 4]))
                .collect()
        }
        (_, [p]) => vec![line(*p, *p)],
        (MarkKind::Stroke, _) => points.windows(2).map(|w| line(w[0], w[1])).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(kind: MarkKind, points: &[[f32; 2]]) -> Mark {
        Mark {
            kind,
            points: points.to_vec(),
        }
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn 横長の映像は上下の余白を除いた矩形に写す() {
        // 1000x1000 の領域に 16:9 は 1000x562.5 で、上下に 218.75 ずつ余白。
        let area = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let s = mark_segments(
            &[mark(MarkKind::Stroke, &[[0.0, 0.0], [1.0, 1.0]])],
            (1920.0, 1080.0),
            area,
        );
        assert_eq!(s.len(), 1);
        assert!(close(s[0].a, [0.0, 218.75]), "{:?}", s[0].a);
        assert!(close(s[0].b, [1000.0, 781.25]), "{:?}", s[0].b);
    }

    #[test]
    fn 縦長の映像は左右の余白を除き_領域の位置もずらす() {
        // (100, 50) から 1600x900 の領域に 9:16 は 506.25x900 で、左右に 546.875 ずつ余白。
        let area = Rect::new(100.0, 50.0, 1600.0, 900.0);
        let s = mark_segments(
            &[mark(MarkKind::Point, &[[0.5, 0.25]])],
            (1080.0, 1920.0),
            area,
        );
        let x = 100.0 + 546.875 + 0.5 * 506.25;
        let y = 50.0 + 0.25 * 900.0;
        assert_eq!(s.len(), 1);
        assert!(close(s[0].a, [x, y]) && s[0].a == s[0].b, "{:?}", s[0]);
        assert_eq!(s[0].width, MARK_POINT_PX);
    }

    #[test]
    fn 線の太さは映像の大きさに依らず一定() {
        let m = [mark(MarkKind::Stroke, &[[0.1, 0.1], [0.9, 0.9]])];
        let small = mark_segments(&m, (640.0, 360.0), Rect::new(0.0, 0.0, 320.0, 180.0));
        let large = mark_segments(&m, (3840.0, 2160.0), Rect::new(0.0, 0.0, 2560.0, 1440.0));
        assert_eq!(small[0].width, MARK_LINE_PX);
        assert_eq!(large[0].width, MARK_LINE_PX);
    }

    #[test]
    fn 矩形は対角の_2_点から_4_辺を作る() {
        let s = mark_segments(
            &[mark(MarkKind::Rect, &[[0.25, 0.25], [0.75, 0.5]])],
            (100.0, 100.0),
            Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        let ends: Vec<_> = s.iter().map(|g| (g.a, g.b)).collect();
        assert_eq!(
            ends,
            [
                ([25.0, 25.0], [75.0, 25.0]),
                ([75.0, 25.0], [75.0, 50.0]),
                ([75.0, 50.0], [25.0, 50.0]),
                ([25.0, 50.0], [25.0, 25.0]),
            ]
        );
    }

    #[test]
    fn 線は隣の点どうしをつなぎ_1_点だけなら点にする() {
        let area = Rect::new(0.0, 0.0, 10.0, 10.0);
        let s = mark_segments(
            &[mark(
                MarkKind::Stroke,
                &[[0.0, 0.0], [0.5, 0.0], [0.5, 0.5]],
            )],
            (10.0, 10.0),
            area,
        );
        assert_eq!(s.len(), 2);
        let dot = mark_segments(&[mark(MarkKind::Stroke, &[[0.5, 0.5]])], (10.0, 10.0), area);
        assert_eq!(dot[0].a, dot[0].b);
        assert_eq!(dot[0].width, MARK_LINE_PX);
    }

    #[test]
    fn 範囲の外の座標は縁に寄せ_数でない座標と画像の無いときは描かない() {
        let area = Rect::new(0.0, 0.0, 10.0, 10.0);
        let s = mark_segments(
            &[mark(
                MarkKind::Stroke,
                &[[-1.0, 2.0], [f32::NAN, 0.5], [0.5, 0.5]],
            )],
            (10.0, 10.0),
            area,
        );
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].a, s[0].b), ([0.0, 10.0], [5.0, 5.0]));
        let m = [mark(MarkKind::Point, &[[0.5, 0.5]])];
        assert!(mark_segments(&m, (0.0, 0.0), area).is_empty());
    }
}
