//! 1 フレームのパスの設定。描く数と、サーフェス全体を Program の背景色で塗るパス。

use mm_ui_layout::palette;

/// 1 フレームのパスで描く数。
pub(super) struct Counts {
    /// 文字の下に描く矩形の数。残りの矩形（`under..quads`）は文字の上に重ねる。
    pub(super) under: u32,
    /// 帯までの矩形の数。サムネイルはその後ろに描く。
    pub(super) bars: u32,
    /// 描くサムネイルの数。
    pub(super) thumbs: u32,
    pub(super) quads: u32,
    /// 描き込みの線分の数。
    pub(super) drawing: u32,
}

/// サーフェス全体を Program の背景色で塗るパスを始める。
pub(super) fn begin_pass<'a>(
    enc: &'a mut wgpu::CommandEncoder,
    target: &'a wgpu::TextureView,
) -> wgpu::RenderPass<'a> {
    let c = palette::PROGRAM_BACKDROP.0;
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("ui"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: f64::from(c[0]),
                    g: f64::from(c[1]),
                    b: f64::from(c[2]),
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
