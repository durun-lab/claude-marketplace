//! トラックマット（`Layer.track_matte`）。After Effects の Track Matte と同じく、切り抜くレイヤーとマットのレイヤーを
//! それぞれ出力の大きさのテクスチャに描き（変形、切り抜き、マスク、効果、不透明度を掛けた後）、画素ごとに
//! マットの被覆率を掛けて 1 枚にする。その 1 枚を、切り抜くレイヤーの合成モードで重ねる。
//!
//! - 被覆率はマットのアルファか輝度。輝度は作業色空間（線形の光、Rec.709 の原色）の上で premultiplied のまま取るので、
//!   straight の輝度にアルファを掛けた値になる（CSS の `mask-mode: luminance`、geneva の `mask.asset` と同じ式）。
//! - マットに使われるレイヤーは、そのコンポジションの合成から外す（After Effects の既定）。
//! - マットのレイヤーが時刻の外なら空のマット（全部を隠す。反転なら全部を見せる）。

use std::collections::HashSet;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use mm_doc::{Composition, Layer, MatteKind, TrackMatte};
use mm_gpu::FRAME_FORMAT;
use mm_types::{MmError, Rect, Result, Size};

use crate::blend::BlendPass;
use crate::clip::Clip;
use crate::layers::{CompCx, Prepared, MAX_DEPTH};
use crate::renderer::WgpuRenderer;
use crate::transform::{uv_to_ndc, LayerTransform};

/// 作業色空間（Rec.709 の原色）の輝度の係数。シェーダーの同名の定数へは文字列で埋める。
pub(crate) const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
/// シェーダーの `flags` のビット。
const FLAG_LUMA: u32 = 1;
const FLAG_INVERTED: u32 = 2;

/// シェーダーの `Matte` と同じ並び（16 バイト）。
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MatteUniform {
    flags: u32,
    _pad: [u32; 3],
}

/// シェーダーに渡すビット。
pub(crate) fn flags(matte: &TrackMatte) -> u32 {
    let luma = if matte.kind == MatteKind::Luma {
        FLAG_LUMA
    } else {
        0
    };
    luma | if matte.inverted { FLAG_INVERTED } else { 0 }
}

/// コンポジションの中でトラックマットに使われるレイヤーの ID。合成から外す。
pub(crate) fn matte_layers(comp: &Composition) -> HashSet<&str> {
    comp.layers
        .iter()
        .filter_map(|l| l.track_matte.as_ref())
        .map(|m| m.layer.as_str())
        .collect()
}

/// マットを掛ける全画面のパス。
pub(crate) struct MattePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl MattePass {
    /// パイプラインを作る。
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let layout = bind_group_layout(device);
        let pipeline = create_pipeline(device, &layout);
        Self { pipeline, layout }
    }

    /// `out` に `layer` × マットの被覆率を描くパスを `encoder` に積む。3 枚とも同じ大きさ。
    pub(crate) fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (layer, matte, out): (&wgpu::Texture, &wgpu::Texture, &wgpu::Texture),
        flags: u32,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let group = self.bind_group(device, flags, (layer, matte));
        let out_view = out.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render.matte"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            timestamp_writes,
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    fn bind_group(
        &self,
        device: &wgpu::Device,
        flags: u32,
        (layer, matte): (&wgpu::Texture, &wgpu::Texture),
    ) -> wgpu::BindGroup {
        let uniform = MatteUniform {
            flags,
            _pad: [0; 3],
        };
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mm-render matte uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let (layer_view, matte_view) = (view(layer), view(matte));
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mm-render matte"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&layer_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&matte_view),
                },
            ],
        })
    }
}

/// 0 が `Matte`、1 が切り抜くレイヤー、2 がマット。
fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let texture = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mm-render matte layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            texture(1),
            texture(2),
        ],
    })
}

fn create_pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
    let luma = LUMA.map(|c| format!("{c:?}")).join(", ");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mm-render matte"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("matte.wgsl").replace("{{LUMA}}", &luma).into(),
        ),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mm-render matte pipeline layout"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mm-render matte"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FRAME_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl WgpuRenderer {
    /// レイヤーを合成に渡す 1 枚にする。トラックマットがあれば掛ける。`chain` はマットを辿った段数。
    pub(crate) fn matted_item(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        cx: &CompCx<'_>,
        layer: &Layer,
        temps: &mut Vec<u64>,
        chain: u32,
    ) -> Result<Option<Prepared>> {
        let Some(item) = self.layer_item(encoder, cx, layer, temps)? else {
            return Ok(None);
        };
        let Some(m) = &layer.track_matte else {
            return Ok(Some(item));
        };
        if chain >= MAX_DEPTH {
            return Err(MmError::InvalidArgument(format!(
                "トラックマットが {MAX_DEPTH} 段を超えた（循環の疑い）"
            )));
        }
        let matte_layer = cx
            .comp
            .layers
            .iter()
            .find(|l| l.id == m.layer)
            .ok_or_else(|| MmError::NotFound(format!("トラックマットのレイヤー {}", m.layer)))?;
        let matte = if matte_layer.range()?.contains(cx.t) {
            self.matted_item(encoder, cx, matte_layer, temps, chain + 1)?
        } else {
            None
        };
        let Some(matte) = matte else {
            return Ok(m.inverted.then_some(item));
        };
        self.combine(encoder, cx, (item, matte), flags(m), temps)
            .map(Some)
    }

    /// 切り抜くレイヤーとマットを別のテクスチャに描き、掛け合わせた 1 枚を返す。
    fn combine(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        cx: &CompCx<'_>,
        (item, matte): (Prepared, Prepared),
        flags: u32,
        temps: &mut Vec<u64>,
    ) -> Result<Prepared> {
        let passes = item.passes;
        let layer_tex = self.flatten(encoder, cx.size, item, temps)?;
        let matte_tex = self.flatten(encoder, cx.size, matte, temps)?;
        let out = self.gpu.create(cx.size)?;
        temps.push(out);
        let Self {
            gpu,
            matte: pass,
            timer,
            ..
        } = self;
        let device = &gpu.context().device;
        let pass = pass.get_or_insert_with(|| MattePass::new(device));
        let texture = |id| gpu.texture(id).expect("作ったばかり");
        pass.encode(
            device,
            encoder,
            (texture(layer_tex), texture(matte_tex), texture(out)),
            flags,
            timer.pass_writes(),
        );
        Ok(full_frame(cx, out, passes))
    }

    /// 1 枚を、出力の大きさの透明なテクスチャに source-over で描く。
    fn flatten(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        size: Size,
        item: Prepared,
        temps: &mut Vec<u64>,
    ) -> Result<u64> {
        let id = self.gpu.create(size)?;
        temps.push(id);
        let item = Prepared {
            passes: &[BlendPass::Over],
            ..item
        };
        self.composite(encoder, id, &[item]);
        Ok(id)
    }
}

/// 出力の大きさのテクスチャを、画面いっぱいに等倍で重ねる 1 枚。
fn full_frame(cx: &CompCx<'_>, texture: u64, passes: &'static [BlendPass]) -> Prepared {
    let (w, h) = cx.comp_size();
    let place = Rect::new(0.0, 0.0, w, h);
    let out = (cx.size.width as f32, cx.size.height as f32);
    Prepared {
        texture,
        matrix: uv_to_ndc(
            &LayerTransform::default(),
            place,
            (w, h),
            cx.comp_to_out(),
            out,
        ),
        opacity: 1.0,
        taps: [1.0, 1.0],
        clip: Clip::none(place),
        passes,
    }
}

#[cfg(test)]
mod tests;
