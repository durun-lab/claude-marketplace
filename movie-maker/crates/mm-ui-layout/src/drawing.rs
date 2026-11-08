//! コメントの描き込みを Program の上に重ねる計算。0〜1 に正規化した描き込みの座標を、
//! Program に映っている映像の矩形（レターボックスを除く）のサーフェスの座標へ変換し、太さつきの線分に分ける。

use crate::geom::{fit_rect, Rect};

/// 線の太さ（論理ピクセル）。モバイル版の描き込みの線（CSS の 3px）と同じ見た目の太さにする。
pub const DRAWING_LINE_PX: f32 = 3.0;
/// 点の直径（論理ピクセル）。線より太くし、1 点でも見つけられるようにする。
pub const DRAWING_POINT_PX: f32 = 12.0;

/// 描き込みの種類。`Stroke` は点の並び、`Rect` は対角の 2 点、`Point` は 1 点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawingShapeKind {
    Stroke,
    Rect,
    Point,
}

/// 描き込み 1 つ。座標はフレームの左上を (0, 0)、右下を (1, 1) に正規化する。
#[derive(Debug, Clone, PartialEq)]
pub struct DrawingShape {
    pub kind: DrawingShapeKind,
    pub points: Vec<[f32; 2]>,
}

/// 描く線分 1 本（サーフェスの座標）。端は丸め、長さ 0 は直径 `width` の点になる。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawingSegment {
    pub a: [f32; 2],
    pub b: [f32; 2],
    /// 物理ピクセル。
    pub width: f32,
}

/// 正規化した座標を映像の矩形の点へ変換する。外れた値は映像の縁に寄せる。
pub fn frame_point(frame: Rect, p: [f32; 2]) -> [f32; 2] {
    let [u, v] = p.map(|c| c.clamp(0.0, 1.0));
    [frame.x + u * frame.w, frame.y + v * frame.h]
}

/// 描き込みを線分に分ける。`image` は Program の画像の大きさ、`area` は Program の領域で、
/// 映像は `area` の中に縦横比を保って収まる（[`fit_rect`]）。`scale` は表示の倍率（DPI / 96）で、太さに掛ける。
pub fn drawing_segments(
    shapes: &[DrawingShape],
    image: (f32, f32),
    area: Rect,
    scale: f32,
) -> Vec<DrawingSegment> {
    let frame = fit_rect(image.0, image.1, area);
    let mut out = Vec::new();
    if frame.w <= 0.0 || frame.h <= 0.0 {
        return out;
    }
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    for shape in shapes {
        push_segments(&mut out, shape, frame, scale);
    }
    out
}

fn push_segments(out: &mut Vec<DrawingSegment>, shape: &DrawingShape, frame: Rect, scale: f32) {
    let mut points = shape
        .points
        .iter()
        .filter(|p| p.iter().all(|c| c.is_finite()))
        .map(|p| frame_point(frame, *p));
    let line = |a: [f32; 2], b: [f32; 2]| DrawingSegment {
        a,
        b,
        width: DRAWING_LINE_PX * scale,
    };
    let Some(first) = points.next() else {
        return;
    };
    match (shape.kind, points.next()) {
        (DrawingShapeKind::Point, _) => out.push(DrawingSegment {
            a: first,
            b: first,
            width: DRAWING_POINT_PX * scale,
        }),
        (_, None) => out.push(line(first, first)),
        (DrawingShapeKind::Rect, Some(b)) => {
            let corners = [first, [b[0], first[1]], b, [first[0], b[1]]];
            out.extend((0..4).map(|i| line(corners[i], corners[(i + 1) % 4])));
        }
        (DrawingShapeKind::Stroke, Some(second)) => {
            out.push(line(first, second));
            let mut prev = second;
            for p in points {
                out.push(line(prev, p));
                prev = p;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(kind: DrawingShapeKind, points: &[[f32; 2]]) -> DrawingShape {
        DrawingShape {
            kind,
            points: points.to_vec(),
        }
    }

    fn segments(shapes: &[DrawingShape], image: (f32, f32), area: Rect) -> Vec<DrawingSegment> {
        drawing_segments(shapes, image, area, 1.0)
    }

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn 横長の映像は上下の余白を除いた矩形に変換する() {
        // 1000x1000 の領域に 16:9 は 1000x562.5 で、上下に 218.75 ずつ余白。
        let area = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let s = segments(
            &[shape(DrawingShapeKind::Stroke, &[[0.0, 0.0], [1.0, 1.0]])],
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
        let s = segments(
            &[shape(DrawingShapeKind::Point, &[[0.5, 0.25]])],
            (1080.0, 1920.0),
            area,
        );
        let x = 100.0 + 546.875 + 0.5 * 506.25;
        let y = 50.0 + 0.25 * 900.0;
        assert_eq!(s.len(), 1);
        assert!(close(s[0].a, [x, y]) && s[0].a == s[0].b, "{:?}", s[0]);
        assert_eq!(s[0].width, DRAWING_POINT_PX);
    }

    #[test]
    fn 線の太さは映像の大きさに依らず_表示の倍率だけを掛ける() {
        let m = [shape(DrawingShapeKind::Stroke, &[[0.1, 0.1], [0.9, 0.9]])];
        let small = segments(&m, (640.0, 360.0), Rect::new(0.0, 0.0, 320.0, 180.0));
        let large = segments(&m, (3840.0, 2160.0), Rect::new(0.0, 0.0, 2560.0, 1440.0));
        assert_eq!(small[0].width, DRAWING_LINE_PX);
        assert_eq!(large[0].width, DRAWING_LINE_PX);
        let area = Rect::new(0.0, 0.0, 2560.0, 1440.0);
        let hidpi = drawing_segments(&m, (3840.0, 2160.0), area, 2.0);
        assert_eq!(hidpi[0].width, DRAWING_LINE_PX * 2.0);
        let dot = [shape(DrawingShapeKind::Point, &[[0.5, 0.5]])];
        let hidpi_dot = drawing_segments(&dot, (16.0, 9.0), area, 1.5);
        assert_eq!(hidpi_dot[0].width, DRAWING_POINT_PX * 1.5);
        let broken = drawing_segments(&m, (16.0, 9.0), area, f32::NAN);
        assert_eq!(broken[0].width, DRAWING_LINE_PX, "倍率が数でなければ 1 倍");
    }

    #[test]
    fn 矩形は対角の_2_点から_4_辺を作る() {
        let s = segments(
            &[shape(DrawingShapeKind::Rect, &[[0.25, 0.25], [0.75, 0.5]])],
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
        let s = segments(
            &[shape(
                DrawingShapeKind::Stroke,
                &[[0.0, 0.0], [0.5, 0.0], [0.5, 0.5]],
            )],
            (10.0, 10.0),
            area,
        );
        let ends: Vec<_> = s.iter().map(|g| (g.a, g.b)).collect();
        assert_eq!(ends, [([0.0, 0.0], [5.0, 0.0]), ([5.0, 0.0], [5.0, 5.0])]);
        let dot = segments(
            &[shape(DrawingShapeKind::Stroke, &[[0.5, 0.5]])],
            (10.0, 10.0),
            area,
        );
        assert_eq!(dot[0].a, dot[0].b);
        assert_eq!(dot[0].width, DRAWING_LINE_PX);
    }

    #[test]
    fn 範囲の外の座標は縁に寄せ_数でない座標と画像の無いときは描かない() {
        let area = Rect::new(0.0, 0.0, 10.0, 10.0);
        let s = segments(
            &[shape(
                DrawingShapeKind::Stroke,
                &[[-1.0, 2.0], [f32::NAN, 0.5], [0.5, 0.5]],
            )],
            (10.0, 10.0),
            area,
        );
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].a, s[0].b), ([0.0, 10.0], [5.0, 5.0]));
        let m = [shape(DrawingShapeKind::Point, &[[0.5, 0.5]])];
        assert!(segments(&m, (0.0, 0.0), area).is_empty());
        let empty = [shape(DrawingShapeKind::Rect, &[])];
        assert!(segments(&empty, (10.0, 10.0), area).is_empty());
    }
}
