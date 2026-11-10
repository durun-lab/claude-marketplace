//! 文字の描画の画素。1 字ずつ並べた再生位置のタイムコードが、文字列ごとに描いたときと同じ画素になるかを、
//! 表示の倍率ごとに確かめる（字ごとの置き場所が物理ピクセルにそろい、ほかの文字よりぼやけない）。

use std::sync::Arc;

use mm_gpu::{GpuContext, GpuRequest};
use mm_ui_layout::{
    place_timecode, Align, Color, Label, Rect, Scale, TextMeasure, TextStyle, UiFont,
};

use super::TextLayer;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// 描くテクスチャの大きさ（物理ピクセル）。
const SIZE: (u32, u32) = (240, 48);

/// 共有の GPU のコンテキスト。無ければスキップする（`MM_REQUIRE_GPU` があれば失敗にする）。
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

/// `labels` を文字列ごとに、`timecodes` を 1 字ずつ並べて描き、RGBA8 の画素（行は詰めてある）を返す。
fn draw(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    layer: &mut TextLayer,
    (labels, timecodes): (&[Label], &[Label]),
    scale: Scale,
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE.0,
            height: SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    layer.prepare(device, queue, (labels, timecodes), scale, SIZE);
    let view = texture.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        layer.draw(&mut pass);
    }
    queue.submit([enc.finish()]);
    read_back(device, queue, &texture)
}

/// テクスチャの画素をリードバックする（行は詰めてある）。
fn read_back(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let (w, h) = (texture.width(), texture.height());
    let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    let layout = wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(row),
        rows_per_image: Some(h),
    };
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    let dst = wgpu::TexelCopyBufferInfo {
        buffer: &buffer,
        layout,
    };
    enc.copy_texture_to_buffer(texture.as_image_copy(), dst, size);
    queue.submit([enc.finish()]);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
    let mapped = buffer.slice(..).get_mapped_range().expect("range");
    (0..h as usize)
        .flat_map(|y| &mapped[y * row as usize..][..(w * 4) as usize])
        .copied()
        .collect()
}

/// テーマのフォントの数字の送り幅が全部同じか。
fn digits_same_width(layer: &mut TextLayer, size: f32) -> bool {
    layer.measure().timecode_stops("0123456789", size);
    let cache = layer.measure().cache();
    ('0'..='9').all(|d| {
        let glyph = cache.get(d.encode_utf8(&mut [0; 4]), size);
        glyph.map(|g| g.advance()) == cache.timecode_advance(d, size)
    })
}

#[test]
fn 数字の幅がそろったフォントなら_1字ずつ並べても_切り詰めても_文字列ごとに描いたのと同じ画素になる(
) {
    let Some((device, queue)) = gpu() else { return };
    let mut layer = TextLayer::new(&device, &queue, FORMAT, &UiFont::default().family);
    if !digits_same_width(&mut layer, 12.0) {
        eprintln!("テーマのフォントの数字の幅がそろっていないのでスキップする");
        return;
    }
    let text: Arc<str> = "12:34:56:78".into();
    let style = TextStyle {
        size: 12.0,
        color: Color([1.0; 4]),
    };
    // 幅 150 は全体が入り、幅 50 は切り詰めて「…」を付ける。
    for (factor, w) in [1.0, 1.25, 1.5]
        .into_iter()
        .flat_map(|f| [(f, 150.0), (f, 50.0)])
    {
        let rect = Rect::new(7.3, 4.6, w, 25.0);
        let label = place_timecode(&text, rect, 0.0, Align::Left, style, layer.measure());
        let label = label.expect("1 字と「…」は入る");
        assert_eq!(label.cut.is_some(), w < 100.0, "幅 {w}");
        layer.measure().stops(&text, style.size);
        let one = std::slice::from_ref(&label);
        let scale = Scale::new(factor);
        let whole = draw((&device, &queue), &mut layer, (one, &[]), scale);
        let each = draw((&device, &queue), &mut layer, (&[], one), scale);
        assert!(
            whole.iter().any(|&v| v > 128),
            "倍率 {factor}、幅 {w}: 字を描く"
        );
        assert!(
            whole == each,
            "倍率 {factor}、幅 {w}: 1 字ずつ並べた画素が違う"
        );
    }
}
