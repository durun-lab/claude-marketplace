//! 結合テストが共通に使うハーネス。共有の GPU のコンテキスト、オフスクリーンのテクスチャへの描画とリードバック、画素の比べ方。

use mm_gpu::{GpuContext, GpuRequest};
use mm_ui_layout::{Rect, Rows, TimelineLayout, TimelineModel, Viewport};
use mm_ui_wgpu::Ui;

pub(crate) const W: u32 = 640;
pub(crate) const H: u32 = 400;
pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// 既定のバックエンドの共有のコンテキストで描く。共有のコンテキストはプロセスの終わりまで破棄しないので、
/// `WGPU_BACKEND=vulkan` で回しても、NVIDIA の Vulkan ドライバーの破棄とプロセスの終了が重ならない。
pub(crate) fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
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

/// `w`x`h` のテクスチャに描いて RGBA8 で返す（行は詰めてある）。`ui` は先に同じ大きさへ `resize` しておく。
pub(crate) fn render_sized(
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

/// テクスチャの画素をリードバックする（行は詰めてある）。`bpp` は 1 画素のバイト数。
pub(crate) fn read_back(
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

/// `Ui` と同じ `mm-ui-layout` の関数で求めた、行 `row` の時刻 `[start, end)` の帯の矩形（物理ピクセル）。
/// ズームとスクロールと行の高さは `Ui::view_state` から読む（開いたときのスクロールを含む）。
pub(crate) fn bar_rect(
    ui: &Ui,
    model: &TimelineModel,
    scale: f32,
    row: usize,
    (start, end): (f64, f64),
) -> Rect {
    let t = ui.layout().timeline;
    let state = ui.view_state().expect("最初の表示を当てた後");
    let rows = Rows::of(&model.items, &state.heights);
    let layout = TimelineLayout::new((t.w / scale, t.h / scale), rows.content_height());
    let vp = Viewport {
        layout: &layout,
        rows: &rows,
        time: state.time,
        scroll_y: state.scroll_y,
    };
    let b = vp.bar(row, start, end);
    Rect::new(
        t.x + b.x * scale,
        t.y + b.y * scale,
        b.w * scale,
        b.h * scale,
    )
}

/// 幅 `w` の画像の画素。
pub(crate) fn px_in(img: &[u8], w: u32, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * w + x) * 4) as usize;
    [img[i], img[i + 1], img[i + 2]]
}

pub(crate) fn hex(c: mm_ui_layout::Color) -> [u8; 3] {
    [0, 1, 2].map(|i| (c.0[i] * 255.0).round() as u8)
}

pub(crate) fn close(a: [u8; 3], b: [u8; 3], tol: i32) -> bool {
    a.iter()
        .zip(&b)
        .all(|(x, y)| (i32::from(*x) - i32::from(*y)).abs() <= tol)
}
