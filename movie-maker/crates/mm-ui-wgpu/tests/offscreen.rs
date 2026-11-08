//! `Ui` をオフスクリーンのテクスチャに描いて、画素をリードバックする。GPU が無ければスキップする（`MM_REQUIRE_GPU` があれば失敗）。

use mm_gpu::{GpuContext, GpuRequest};
use mm_types::{Rgba, RgbaImage, Size};
use mm_ui_layout::{
    fit_rect, frame_point, palette, DrawingShape, DrawingShapeKind, Item, ItemKind, TimelineModel,
};
use mm_ui_wgpu::{Region, Ui};

const W: u32 = 640;
const H: u32 = 400;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// 既定のバックエンドの共有のコンテキストで描く。共有のコンテキストはプロセスの終わりまで破棄しないので、
/// `WGPU_BACKEND=vulkan` で回しても、NVIDIA の Vulkan ドライバーの破棄とプロセスの終了が重ならない。
fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    match GpuContext::shared(&GpuRequest::default()) {
        Ok(ctx) => Some((ctx.device.clone(), ctx.queue.clone())),
        Err(e) if std::env::var_os("MM_REQUIRE_GPU").is_some() => {
            panic!("MM_REQUIRE_GPU が有るのに GPU を作れない: {e}")
        }
        Err(e) => {
            eprintln!("GPU が無いのでスキップする: {e}");
            None
        }
    }
}

fn model() -> TimelineModel {
    TimelineModel {
        duration: 10.0,
        fps: 30.0,
        items: vec![Item::layer(0.0, 10.0, ItemKind::Footage, true)],
    }
}

/// 描いた結果を RGBA8 で返す（行は詰めてある）。
fn render(device: &wgpu::Device, queue: &wgpu::Queue, ui: &mut Ui) -> Vec<u8> {
    render_sized(device, queue, ui, (W, H))
}

/// `w`x`h` のテクスチャに描く。`ui` は先に同じ大きさへ `resize` しておく。
fn render_sized(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    ui: &mut Ui,
    (w, h): (u32, u32),
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    ui.render(device, queue, &texture.create_view(&Default::default()));
    read_back(device, queue, &texture, 4)
}

/// テクスチャの画素を読み戻す（行は詰めてある）。`bpp` は 1 画素のバイト数。
fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    bpp: u32,
) -> Vec<u8> {
    let (w, h) = (texture.width(), texture.height());
    let row = (w * bpp).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([enc.finish()]);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
    let mapped = buffer.slice(..).get_mapped_range().expect("range");
    let mut out = Vec::with_capacity((w * h * bpp) as usize);
    for y in 0..h {
        let start = (y * row) as usize;
        out.extend_from_slice(&mapped[start..start + (w * bpp) as usize]);
    }
    out
}

fn px(img: &[u8], x: u32, y: u32) -> [u8; 3] {
    px_in(img, W, x, y)
}

/// 幅 `w` の画像の画素。
fn px_in(img: &[u8], w: u32, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * w + x) * 4) as usize;
    [img[i], img[i + 1], img[i + 2]]
}

fn hex(c: mm_ui_layout::Color) -> [u8; 3] {
    [0, 1, 2].map(|i| (c.0[i] * 255.0).round() as u8)
}

fn close(a: [u8; 3], b: [u8; 3], tol: i32) -> bool {
    a.iter()
        .zip(&b)
        .all(|(x, y)| (i32::from(*x) - i32::from(*y)).abs() <= tol)
}

#[test]
fn program_は線形の画像を_rec1886_の表示の値で出す() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    // 線形 0.5 は Rec.1886 で 0.5^(1/2.4) = 0.749 → 191。
    let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.5, 0.5, 0.5, 1.0));
    ui.set_frame(&device, &queue, &img);
    let out = render(&device, &queue, &mut ui);
    let p = ui.layout().program;
    let centre = px(&out, (p.x + p.w / 2.0) as u32, (p.y + p.h / 2.0) as u32);
    assert!(close(centre, [191, 191, 191], 2), "{centre:?}");
}

/// HDR の素材の上限を渡すと、1 を超えるハイライトを白に潰さず SDR の表示に収める。上限 1 なら今までどおり。
#[test]
fn program_は_hdr_の上限を渡すとハイライトを_bt2390_で収める() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let shade = |ui: &mut Ui, v: f32| {
        let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(v, v, v, 1.0));
        ui.set_frame(&device, &queue, &img);
        let out = render(&device, &queue, ui);
        let p = ui.layout().program;
        px(&out, (p.x + p.w / 2.0) as u32, (p.y + p.h / 2.0) as u32)[0]
    };
    assert_eq!(shade(&mut ui, 2.0), 255, "上限 1 では 1 を超える値は白");
    assert!(
        close([shade(&mut ui, 0.5); 3], [191; 3], 2),
        "上限 1 の SDR は恒等"
    );
    ui.set_hdr_peak(1000.0 / 203.0);
    let (mid, high, peak) = (
        shade(&mut ui, 1.0),
        shade(&mut ui, 3.0),
        shade(&mut ui, 4.9),
    );
    assert!(
        mid < high && high < peak,
        "ハイライトの階調が残る: {mid} {high} {peak}"
    );
    assert!(peak >= 250, "素材の上限は白の付近: {peak}");
    assert!(
        close(
            [shade(&mut ui, 0.05); 3],
            [mm_types::transfer::linear_to_code8(0.05); 3],
            2
        ),
        "暗部は変えない"
    );
}

/// 1 色で塗った Rgba16Float のテクスチャ（レンダラーの出力の形）。
fn filled_texture(device: &wgpu::Device, queue: &wgpu::Queue, value: f64) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 16,
            height: 9,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: value,
                    g: value,
                    b: value,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    queue.submit([enc.finish()]);
    texture
}

#[test]
fn program_は同じデバイスのテクスチャをそのまま出す() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let texture = filled_texture(&device, &queue, 0.5);
    ui.set_frame_texture(&device, &texture);
    let out = render(&device, &queue, &mut ui);
    let p = ui.layout().program;
    let centre = px(&out, (p.x + p.w / 2.0) as u32, (p.y + p.h / 2.0) as u32);
    assert!(close(centre, [191, 191, 191], 2), "{centre:?}");
    // 借用したテクスチャの後に CPU の画像が来たら、自前のテクスチャに上げ直す（借用したものには書かない）。
    let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(1.0, 1.0, 1.0, 1.0));
    ui.set_frame(&device, &queue, &img);
    let out = render(&device, &queue, &mut ui);
    let centre = px(&out, (p.x + p.w / 2.0) as u32, (p.y + p.h / 2.0) as u32);
    assert!(close(centre, [255, 255, 255], 2), "{centre:?}");
}

#[test]
fn source_のフレームが来ると_program_の左に_source_の領域が現れ_閉じると消える() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    assert_eq!(ui.layout().source, None);
    let program = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.5, 0.5, 0.5, 1.0));
    ui.set_frame(&device, &queue, &program);
    let full = ui.layout().program;

    let source = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.0, 0.5, 0.0, 1.0));
    ui.set_source_frame(&device, &queue, &source);
    let l = ui.layout();
    let area = l.source.expect("Source の領域");
    assert!(area.right() <= l.program.x && l.program.w < full.w);
    assert_eq!(ui.region_at(area.x + 1.0, area.y + 1.0), Region::Source);
    assert_eq!(ui.region_at(l.program.x + 1.0, 1.0), Region::Program);
    assert_eq!(ui.region_at(1.0, l.timeline.y + 1.0), Region::Timeline);

    let out = render(&device, &queue, &mut ui);
    let at = |r: mm_ui_layout::Rect| px(&out, (r.x + r.w / 2.0) as u32, (r.y + r.h / 2.0) as u32);
    // 緑 0.5 は Rec.1886 で 191。Program の灰色 0.5 は 191 が 3 成分。
    assert!(close(at(area), [0, 191, 0], 2), "Source {:?}", at(area));
    assert!(
        close(at(l.program), [191, 191, 191], 2),
        "Program {:?}",
        at(l.program)
    );

    ui.close_source();
    assert_eq!(ui.layout().source, None);
    assert_eq!(ui.layout().program, full, "Program が元の広さに戻る");
}

#[test]
fn 画像の縦横比を保ち_余白は背景のまま() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(1.0, 1.0, 1.0, 1.0));
    ui.set_frame(&device, &queue, &img);
    let out = render(&device, &queue, &mut ui);
    let p = ui.layout().program;
    // Program の領域の左端は 16:9 の画像より横長なので余白。
    let edge = px(&out, 2, (p.y + p.h / 2.0) as u32);
    assert!(close(edge, hex(palette::PROGRAM_BACKDROP), 2), "{edge:?}");
}

#[test]
fn 再生位置の線が時刻の位置に立つ() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    ui.set_playhead(5.0);
    let out = render(&device, &queue, &mut ui);
    let t = ui.layout().timeline;
    // 全体表示は 10 秒が余白 16 px を除いた幅に収まる。5 秒はその中央。
    let x = (t.x + 16.0 + (t.w - 32.0) / 2.0) as u32;
    let y = (t.y + t.h - 3.0) as u32;
    let found =
        (x.saturating_sub(2)..=x + 2).any(|xx| close(px(&out, xx, y), hex(palette::PLAYHEAD), 3));
    assert!(found, "x={x} の近くに再生位置の線");
}

fn point_shape(u: f32, v: f32) -> DrawingShape {
    DrawingShape {
        kind: DrawingShapeKind::Point,
        points: vec![[u, v]],
    }
}

/// 描き込みはサーフェスにだけ重ね、レンダラーから借りたフレームのテクスチャ（キャッシュと書き出しの元）には書かない。
#[test]
fn 描き込みは映像の矩形に重ね_フレームのテクスチャには書かない() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let texture = filled_texture(&device, &queue, 0.0);
    let before = read_back(&device, &queue, &texture, 8);
    ui.set_frame_texture(&device, &texture);
    ui.set_drawing(vec![point_shape(0.25, 0.5)]);
    let out = render(&device, &queue, &mut ui);

    let frame = fit_rect(16.0, 9.0, ui.layout().program);
    let [x, y] = frame_point(frame, [0.25, 0.5]);
    let at = px(&out, x as u32, y as u32);
    assert!(close(at, hex(palette::DRAWING), 3), "描き込みの点 {at:?}");
    let [fx, fy] = frame_point(frame, [0.75, 0.5]);
    let outside = px(&out, fx as u32, fy as u32);
    assert!(
        close(outside, [0, 0, 0], 2),
        "点の外は映像のまま {outside:?}"
    );
    assert_eq!(
        read_back(&device, &queue, &texture, 8),
        before,
        "フレームのテクスチャは変わらない"
    );

    ui.set_drawing(Vec::new());
    let cleared = render(&device, &queue, &mut ui);
    let after = px(&cleared, x as u32, y as u32);
    assert!(close(after, [0, 0, 0], 2), "空で消える {after:?}");
}

#[test]
fn 描き込みはレターボックスの余白でなく映像の縁に合わせる() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    // 縦長の映像は Program の領域の左右に余白ができる。
    let img = RgbaImage::filled(Size::new(9, 16), Rgba::new(0.0, 0.0, 0.0, 1.0));
    ui.set_frame(&device, &queue, &img);
    ui.set_drawing(vec![point_shape(0.0, 0.5)]);
    let out = render(&device, &queue, &mut ui);
    let area = ui.layout().program;
    let frame = fit_rect(9.0, 16.0, area);
    assert!(frame.x > area.x + 20.0, "左に余白がある");
    let y = (frame.y + frame.h / 2.0) as u32;
    let edge = px(&out, (frame.x + 1.0) as u32, y);
    assert!(close(edge, hex(palette::DRAWING), 3), "映像の左端 {edge:?}");
    let margin = px(&out, (area.x + 2.0) as u32, y);
    assert!(
        close(margin, hex(palette::PROGRAM_BACKDROP), 2),
        "余白 {margin:?}"
    );
}

/// 頂点は条件が同じ間は作り直さないので、表示の倍率とウィンドウの大きさを変えたら描き直されることを確かめる。
#[test]
fn 描き込みは表示の倍率とウィンドウの大きさが変わると描き直す() {
    let Some((device, queue)) = gpu() else { return };
    let mut ui = Ui::new(&device, FORMAT);
    ui.set_model(model());
    ui.resize(W, H);
    let img = RgbaImage::filled(Size::new(16, 9), Rgba::new(0.0, 0.0, 0.0, 1.0));
    ui.set_frame(&device, &queue, &img);
    ui.set_drawing(vec![point_shape(0.5, 0.5)]);
    let first = render(&device, &queue, &mut ui);
    let [x, y] = frame_point(fit_rect(16.0, 9.0, ui.layout().program), [0.5, 0.5]);
    // 点の中心から 8 画素の所は、倍率 1（直径 12）では点の外、倍率 2（直径 24）では点の中。
    let (ox, oy) = (x as u32 + 8, y as u32);
    let base = px(&first, ox, oy);
    assert!(close(base, [0, 0, 0], 2), "倍率 1 では点の外 {base:?}");
    ui.set_scale_factor(2.0);
    let hidpi = render(&device, &queue, &mut ui);
    let wide = px(&hidpi, ox, oy);
    assert!(
        close(wide, hex(palette::DRAWING), 3),
        "倍率 2 では点の中 {wide:?}"
    );

    let small = (W / 2, H / 2);
    ui.resize(small.0, small.1);
    let out = render_sized(&device, &queue, &mut ui, small);
    let [sx, sy] = frame_point(fit_rect(16.0, 9.0, ui.layout().program), [0.5, 0.5]);
    let at = px_in(&out, small.0, sx as u32, sy as u32);
    assert!(
        close(at, hex(palette::DRAWING), 3),
        "小さくしても映像の中央 {at:?}"
    );
}
