//! `Quad` の形（ひし形と上下左右の三角）をオフスクリーンに描いて、形の内外と縁のなめらかさを確かめる。
//! `MM_SHAPES_DUMP=<dir>` を付けると描いた画像を PNG で書き出す（期待画像を目で見る用）。

use mm_ui_layout::{Color, Quad, Rect, Scale, Shape};
use mm_ui_wgpu::QuadRenderer;

use crate::support::{gpu, px_in, read_back, FORMAT};

/// 論理ピクセルのシートの大きさ。
const SHEET: (f32, f32) = (300.0, 60.0);
const BG: Color = Color::hex(0x202020);
const FG: Color = Color::hex(0xF0F0F0);
/// 形の外接する矩形の一辺（論理ピクセル）。縁から 2 画素以上離れた画素を取れる大きさ。
const SIZE: f32 = 24.0;
/// 内外の判定で縁から離す距離（物理ピクセル）。縁の画素は半端な値になるので見ない。
const MARGIN: f32 = 2.0;

/// 形ごとの外接する矩形（論理ピクセル）。ひし形は縦長と横長も。
fn sheet() -> Vec<Quad> {
    let cell = |i: f32| (20.0 + i * 36.0, SHEET.1 / 2.0);
    let sq = (SIZE, SIZE);
    vec![
        Quad::diamond(cell(0.0), sq, FG),
        Quad::triangle(cell(1.0), sq, Shape::TriUp, FG),
        Quad::triangle(cell(2.0), sq, Shape::TriDown, FG),
        Quad::triangle(cell(3.0), sq, Shape::TriLeft, FG),
        Quad::triangle(cell(4.0), sq, Shape::TriRight, FG),
        Quad::diamond(cell(5.0), (14.0, 28.0), FG),
        Quad::diamond(cell(6.0), (28.0, 14.0), FG),
        Quad::new(Rect::new(cell(7.0).0 - 12.0, 18.0, 24.0, 24.0), FG),
    ]
}

/// シートを倍率 `scale` で描いて RGBA8 と大きさ、物理ピクセルの図形の並びを返す。
fn render(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scale: f32,
) -> (Vec<u8>, (u32, u32), Vec<Quad>) {
    let s = Scale::new(scale);
    let size = ((SHEET.0 * scale) as u32, (SHEET.1 * scale) as u32);
    let quads: Vec<Quad> = sheet()
        .into_iter()
        .map(|q| Quad {
            rect: s.rect_to_physical(q.rect),
            ..q
        })
        .collect();
    let mut r = QuadRenderer::new(device, FORMAT);
    let n = r.prepare(device, queue, &quads, (size.0 as f32, size.1 as f32));
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let [r8, g8, b8, _] = BG.0;
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(r8),
                        g: f64::from(g8),
                        b: f64::from(b8),
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        r.draw(&mut pass, 0..n);
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

/// 縁から `MARGIN` 画素以上内側の画素は前景、外側の画素は背景。縁の画素は両方の間に入る。
fn check_shapes(scale: f32) {
    let Some((device, queue)) = gpu() else { return };
    let (img, size, quads) = render(&device, &queue, scale);
    dump(&img, size, scale);
    let bg = 0x20u8;
    assert_eq!(red(&img, size.0, 0, 0), bg, "背景の画素が背景の色でない");
    let mut partial = 0;
    for q in &quads {
        let (hw, hh) = (q.rect.w / 2.0, q.rect.h / 2.0);
        let (cx, cy) = (q.rect.x + hw, q.rect.y + hh);
        let pad = MARGIN.ceil() as i32 + 1;
        for y in (q.rect.y as i32 - pad)..=(q.rect.bottom() as i32 + pad) {
            for x in (q.rect.x as i32 - pad)..=(q.rect.right() as i32 + pad) {
                let (lx, ly) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let v = red(&img, size.0, x as u32, y as u32);
                let inner = q.shape.contains(lx, ly, hw - MARGIN, hh - MARGIN);
                let outer = q.shape.contains(lx, ly, hw + MARGIN, hh + MARGIN);
                if inner {
                    assert!(v >= 0xF0, "{:?} ({x},{y}) は内側なのに {v}", q.shape);
                } else if !outer {
                    assert_eq!(v, bg, "{:?} ({x},{y}) は外側なのに {v}", q.shape);
                } else if v > bg && v < 0xF0 {
                    partial += 1;
                }
            }
        }
    }
    assert!(partial > 20, "縁になめらかな中間の画素が無い: {partial}");
}

#[test]
fn 形の内側は塗り外側は背景で縁は中間の値_倍率_1() {
    check_shapes(1.0);
}

#[test]
fn 形の内側は塗り外側は背景で縁は中間の値_倍率_1_5() {
    check_shapes(1.5);
}

#[test]
fn 形の内側は塗り外側は背景で縁は中間の値_倍率_2() {
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
