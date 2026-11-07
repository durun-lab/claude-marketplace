//! CPU の画像を画布に収める。種類の契約（CPU の画像の大きさは画布と同じ）に合わせる。
//! `footage` の `contain` と同じ置き方。実装のクレートはほかの実装に依存しないので、`mm-nodes-basic` の同名の関数を持つ。

use mm_types::{Rgba, RgbaImage, Size};

/// `src` を縦横比を保って `target` に収め、中央に置く（余白は透明）。大きさが同じならそのまま返す。
pub(crate) fn fit_contain(src: RgbaImage, target: Size) -> RgbaImage {
    if src.size() == target || src.width() == 0 || src.height() == 0 {
        return src;
    }
    let ratio =
        (target.width as f32 / src.width() as f32).min(target.height as f32 / src.height() as f32);
    let w = ((src.width() as f32 * ratio).round() as u32).clamp(1, target.width);
    let h = ((src.height() as f32 * ratio).round() as u32).clamp(1, target.height);
    let (ox, oy) = ((target.width - w) / 2, (target.height - h) / 2);
    let mut out = RgbaImage::transparent(target);
    for y in 0..h {
        for x in 0..w {
            let fx = (x as f32 + 0.5) / ratio - 0.5;
            let fy = (y as f32 + 0.5) / ratio - 0.5;
            out.set_pixel(ox + x, oy + y, sample_bilinear(&src, fx, fy));
        }
    }
    out
}

fn lerp(a: Rgba, b: Rgba, t: f32) -> Rgba {
    Rgba::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn sample_bilinear(img: &RgbaImage, x: f32, y: f32) -> Rgba {
    let x0 = x.floor().max(0.0);
    let y0 = y.floor().max(0.0);
    let tx = (x - x0).clamp(0.0, 1.0);
    let ty = (y - y0).clamp(0.0, 1.0);
    let at = |dx: u32, dy: u32| {
        let px = (x0 as u32 + dx).min(img.width() - 1);
        let py = (y0 as u32 + dy).min(img.height() - 1);
        img.pixel(px, py)
    };
    let top = lerp(at(0, 0), at(1, 0), tx);
    let bottom = lerp(at(0, 1), at(1, 1), tx);
    lerp(top, bottom, ty)
}
