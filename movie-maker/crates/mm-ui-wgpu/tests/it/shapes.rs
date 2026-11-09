//! `Quad` の形（ひし形と上下左右の三角形）をオフスクリーンに描いて、形の内外とアンチエイリアスの縁を確かめる。
//! `MM_SHAPES_DUMP=<dir>` を付けると描いた画像を PNG で書き出す（目で確かめる用）。

use mm_ui_layout::{Color, Quad, Scale, Shape};
use mm_ui_wgpu::QuadRenderer;

use crate::support::{gpu, px_in, read_back, target, FORMAT};

/// テストパターンの大きさ（論理ピクセル）。
const PATTERN: (f32, f32) = (400.0, 60.0);
const BG: Color = Color::hex(0x202020);
const FG: Color = Color::hex(0xF0F0F0);
/// 形の外接する矩形の一辺（論理ピクセル）。
const SIZE: f32 = 24.0;
/// 鋭い角の外側のアンチエイリアスは描く範囲（外接する矩形の 1 物理ピクセル外まで）で切れるので、
/// 角からこの距離（物理ピクセル）の内は縁の値を比べない。
const CORNER: f32 = 2.0;
/// 画素の値（8 ビット）の許容差。
const TOL: f32 = 2.0;

/// 形ごとの外接する矩形（論理ピクセル）。縦長と横長のひし形と三角形、キーフレームのマーカーの大きさ（9）のひし形も。
/// 中心を画素の境からずらし、縁が画素の途中を通るようにする。
fn pattern() -> Vec<Quad> {
    let cell = |i: f32| (20.3 + i * 36.0, PATTERN.1 / 2.0 + 0.4);
    let sq = (SIZE, SIZE);
    vec![
        Quad::diamond(cell(0.0), sq, FG),
        Quad::shaped(cell(1.0), sq, Shape::TriUp, FG),
        Quad::shaped(cell(2.0), sq, Shape::TriDown, FG),
        Quad::shaped(cell(3.0), sq, Shape::TriLeft, FG),
        Quad::shaped(cell(4.0), sq, Shape::TriRight, FG),
        Quad::diamond(cell(5.0), (14.0, 28.0), FG),
        Quad::diamond(cell(6.0), (28.0, 14.0), FG),
        Quad::shaped(cell(7.0), (28.0, 12.0), Shape::TriDown, FG),
        Quad::shaped(cell(8.0), (12.0, 28.0), Shape::TriRight, FG),
        Quad::diamond(cell(9.0), (9.0, 9.0), FG),
        Quad::shaped(cell(10.0), sq, Shape::Rect, FG),
    ]
}

/// テストパターンを表示の倍率 `scale` で描いて、RGBA8 と大きさ、物理ピクセルに換えた並びを返す。
fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scale: f32,
) -> (Vec<u8>, (u32, u32), Vec<Quad>) {
    let s = Scale::new(scale);
    let size = ((PATTERN.0 * scale) as u32, (PATTERN.1 * scale) as u32);
    let quads: Vec<Quad> = pattern()
        .into_iter()
        .map(|q| Quad {
            rect: s.rect_to_physical(q.rect),
            ..q
        })
        .collect();
    let mut renderer = QuadRenderer::new(device, FORMAT);
    let n = renderer.prepare(device, queue, &quads, (size.0 as f32, size.1 as f32));
    let texture = target(device, size);
    let view = texture.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let [r, g, b, _] = BG.0.map(f64::from);
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        renderer.draw(&mut pass, 0..n);
    }
    queue.submit([enc.finish()]);
    (read_back(device, queue, &texture, 4), size, quads)
}

fn dump(img: &[u8], (w, h): (u32, u32), scale: f32) {
    let Some(dir) = std::env::var_os("MM_SHAPES_DUMP") else {
        return;
    };
    let path = std::path::Path::new(&dir).join(format!("shapes-x{scale}.png"));
    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(img))
        .expect("write png");
}

/// 赤の値（背景 0x20 から前景 0xF0 までの間）。
fn red(img: &[u8], w: u32, x: u32, y: u32) -> u8 {
    px_in(img, w, x, y)[0]
}

/// 前景の割合 `share` の画素の赤の値。
fn value(share: f32) -> f32 {
    let [bg, fg] = [BG, FG].map(|c| c.0[0] * 255.0);
    bg + (fg - bg) * share
}

/// 形の角（外接する矩形の中心が原点、y は下向き、画面で時計回り）。
fn corners(q: &Quad) -> Vec<(f32, f32)> {
    let (w, h) = (q.rect.w / 2.0, q.rect.h / 2.0);
    match q.shape {
        Shape::Rect => vec![(-w, -h), (w, -h), (w, h), (-w, h)],
        Shape::Diamond => vec![(0.0, -h), (w, 0.0), (0.0, h), (-w, 0.0)],
        Shape::TriUp => vec![(0.0, -h), (w, h), (-w, h)],
        Shape::TriDown => vec![(0.0, h), (-w, -h), (w, -h)],
        Shape::TriLeft => vec![(-w, 0.0), (w, -h), (w, h)],
        Shape::TriRight => vec![(w, 0.0), (-w, h), (-w, -h)],
    }
}

/// 縁までの符号つき距離（外が正、物理ピクセル）。辺を延ばした直線までの距離の最大で、角の近くの外側では実際より短い。
fn distance(corners: &[(f32, f32)], (x, y): (f32, f32)) -> f32 {
    let next = corners.iter().cycle().skip(1);
    corners
        .iter()
        .zip(next)
        .map(|(&(ax, ay), &(bx, by))| {
            let (ex, ey) = (bx - ax, by - ay);
            ((x - ax) * ey - (y - ay) * ex) / ex.hypot(ey)
        })
        .fold(f32::MIN, f32::max)
}

/// 外接する矩形の中心から見た画素の中心 `p` の、前景の割合。縁は 1 物理ピクセルの幅で、縁までの距離に比例して背景へ移る。
/// 矩形（縁を画素の中心の判定で切る）と形の角の近くは、縁から 1 物理ピクセルより離れた画素だけを見る（None は見ない）。
fn expected(shape: Shape, corners: &[(f32, f32)], p: (f32, f32)) -> Option<f32> {
    let d = distance(corners, p);
    let near_corner = corners
        .iter()
        .any(|&(x, y)| (p.0 - x).hypot(p.1 - y) < CORNER);
    if shape != Shape::Rect && !near_corner {
        Some((0.5 - d).clamp(0.0, 1.0))
    } else if d.abs() > 1.0 {
        Some(if d < 0.0 { 1.0 } else { 0.0 })
    } else {
        None
    }
}

/// 形 1 つの外接する矩形の周りの画素を `expected` と比べ、縁の途中（背景と前景の間）の画素の数を返す。
fn check_quad(img: &[u8], width: u32, q: &Quad, scale: f32) -> usize {
    let corners = corners(q);
    let (cx, cy) = (q.rect.x + q.rect.w / 2.0, q.rect.y + q.rect.h / 2.0);
    let mut edge = 0;
    for y in (q.rect.y as u32 - 3)..=(q.rect.bottom() as u32 + 3) {
        for x in (q.rect.x as u32 - 3)..=(q.rect.right() as u32 + 3) {
            let p = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let Some(share) = expected(q.shape, &corners, p) else {
                continue;
            };
            let (v, want) = (f32::from(red(img, width, x, y)), value(share));
            assert!(
                (v - want).abs() <= TOL,
                "{:?} 表示の倍率 {scale} ({x}, {y}): {v}（期待 {want}）",
                q.shape
            );
            edge += usize::from(share > 0.0 && share < 1.0);
        }
    }
    edge
}

/// テストパターンの全部の形で、画素が縁までの距離から決まる値になっているか確かめる。
fn check_shapes(scale: f32) {
    let Some((device, queue)) = gpu() else { return };
    let (img, size, quads) = render(&device, &queue, scale);
    dump(&img, size, scale);
    for q in &quads {
        let edge = check_quad(&img, size.0, q, scale);
        let soft = q.shape != Shape::Rect;
        assert!(
            !soft || edge > 4,
            "{:?} 表示の倍率 {scale}: 縁の途中の画素が {edge} しかない",
            q.shape
        );
    }
}

#[test]
fn 形の内側は前景_外側は背景_縁は縁までの距離に応じた値_表示の倍率_100() {
    check_shapes(1.0);
}

#[test]
fn 形の内側は前景_外側は背景_縁は縁までの距離に応じた値_表示の倍率_125() {
    check_shapes(1.25);
}

#[test]
fn 形の内側は前景_外側は背景_縁は縁までの距離に応じた値_表示の倍率_150() {
    check_shapes(1.5);
}

#[test]
fn 形の内側は前景_外側は背景_縁は縁までの距離に応じた値_表示の倍率_200() {
    check_shapes(2.0);
}

#[test]
fn 三角形の頂点の向きに応じて重心が寄る() {
    let Some((device, queue)) = gpu() else { return };
    let (img, size, quads) = render(&device, &queue, 2.0);
    // 塗りの面積の重心は、頂点の側と逆（底辺の側）へ外接矩形の中心から 1/6 辺だけずれる。
    let centroid = |q: &Quad| {
        let (mut sx, mut sy, mut sum) = (0.0f64, 0.0f64, 0.0f64);
        for y in (q.rect.y as u32 - 2)..=(q.rect.bottom() as u32 + 2) {
            for x in (q.rect.x as u32 - 2)..=(q.rect.right() as u32 + 2) {
                let a = f64::from(red(&img, size.0, x, y) - 0x20) / f64::from(0xF0 - 0x20);
                sx += a * (f64::from(x) + 0.5);
                sy += a * (f64::from(y) + 0.5);
                sum += a;
            }
        }
        (
            sx / sum - f64::from(q.rect.x + q.rect.w / 2.0),
            sy / sum - f64::from(q.rect.y + q.rect.h / 2.0),
        )
    };
    let shift = f64::from(SIZE * 2.0) / 6.0;
    let near = |a: f64, b: f64| (a - b).abs() < 0.5;
    let sign = |s: Shape| {
        quads
            .iter()
            .find(|q| q.shape == s)
            .map(centroid)
            .expect("shape")
    };
    let up = sign(Shape::TriUp);
    let down = sign(Shape::TriDown);
    let left = sign(Shape::TriLeft);
    let right = sign(Shape::TriRight);
    assert!(near(up.0, 0.0) && near(up.1, shift), "up {up:?}");
    assert!(near(down.0, 0.0) && near(down.1, -shift), "down {down:?}");
    assert!(near(left.0, shift) && near(left.1, 0.0), "left {left:?}");
    assert!(
        near(right.0, -shift) && near(right.1, 0.0),
        "right {right:?}"
    );
}
