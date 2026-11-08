//! HDR の素材の入力の側のトーンマップ。作業の明るさが SDR のプロジェクトでは、HDR の素材のレイヤーの画像を、
//! 効果と合成の前に BT.2390 の EETF（`TONEMAP_WGSL`）で SDR の白（線形 1.0）に収める。SDR の素材、文字、画像の
//! レイヤーは通らないので白は 1.0 のまま。Premiere Pro の「メディアを自動でトーンマップ」と同じ位置。
//!
//! フレームのキャッシュが覚えるレイヤーの画像はトーンマップの前（デコードしたままの線形）で、キーに作業の明るさを
//! 入れない。入れ子の画像は後なので、そのキーにだけ作業の明るさが入る（`WgpuRenderer::key_color`）。

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use mm_gpu::FRAME_FORMAT;
use mm_node_api::NodeKind;
use mm_render_api::{input_tone_map_peak, input_tone_mapped};
use mm_types::tonemap::TONEMAP_WGSL;
use mm_types::transfer::TRANSFER_WGSL;
use mm_types::{MmError, Params, Result};

use crate::frame_cache::asset_of;
use crate::layers::{CompCx, LayerImage};
use crate::renderer::WgpuRenderer;

/// 入れ子の画像のキーの設定の指紋に足す文字列。入力の側のトーンマップが掛かるプロジェクトだけに付ける。
const KEY_MARK: &str = "+input_tonemap:bt2390";

/// premultiplied のまま EETF を掛けると半透明の縁で比が崩れるので、straight に戻して掛けてから掛け直す。
/// `TRANSFER_WGSL` と `TONEMAP_WGSL` を前に連結する。
const INPUT_TONE_WGSL: &str = r"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var<uniform> tone: vec4<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(src, vec2<u32>(pos.xy), 0);
    if (c.a <= 0.0) { return c; }
    return vec4<f32>(mm_tonemap_sdr(c.rgb / c.a, tone.x) * c.a, c.a);
}
";

/// シェーダーの `tone` と同じ並び（16 バイト。x が素材の上限の明るさ）。
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ToneUniform {
    peak: f32,
    _pad: [f32; 3],
}

/// 入力の側のトーンマップの全画面のパス。
pub(crate) struct InputTonePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl InputTonePass {
    fn new(device: &wgpu::Device) -> Self {
        let layout = bind_group_layout(device);
        let pipeline = create_pipeline(device, &layout);
        Self { pipeline, layout }
    }

    /// `src` を `peak` でトーンマップして `dst`（同じ大きさ）に書くパスを積む。
    fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (src, dst): (&wgpu::Texture, &wgpu::Texture),
        peak: f32,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let uniform = ToneUniform {
            peak,
            _pad: [0.0; 3],
        };
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mm-render input tone uniform"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let src_view = src.create_view(&wgpu::TextureViewDescriptor::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mm-render input tone"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        let dst_view = dst.create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render.input_tone"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &dst_view,
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
}

/// 0 が入力の画像、1 が `ToneUniform`。
fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mm-render input tone layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

fn create_pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mm-render input tone"),
        source: wgpu::ShaderSource::Wgsl(
            format!("{TRANSFER_WGSL}{TONEMAP_WGSL}{INPUT_TONE_WGSL}").into(),
        ),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mm-render input tone pipeline layout"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mm-render input tone"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
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
    /// 入れ子の画像のキーに入れる設定の指紋（[`Self::config_fingerprint`]）に、入力の側のトーンマップの有無を足す。
    /// 入れ子の画像はトーンマップの後の画素を覚えるため。掛からないプロジェクトの指紋は変えない。
    pub(crate) fn key_color(&self, project: &mm_doc::Project) -> String {
        let color = self.config_fingerprint();
        if input_tone_mapped(project) {
            format!("{color}{KEY_MARK}")
        } else {
            color
        }
    }

    /// 素材を読む種類のレイヤーの画像に、素材の色空間と作業の明るさで決まる入力の側のトーンマップを掛ける。
    /// 掛からなければ `image` をそのまま返す。作ったテクスチャは `temps` に積む。
    pub(crate) fn tone_map_input(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        cx: &CompCx<'_>,
        (kind, params): (&dyn NodeKind, &Params),
        image: LayerImage,
        temps: &mut Vec<u64>,
    ) -> Result<LayerImage> {
        let peak = if kind.decodes_asset() {
            asset_of(params)
                .and_then(|id| cx.project.asset(id).ok())
                .map_or(1.0, |a| input_tone_map_peak(cx.project, &a.color_space))
        } else {
            1.0
        };
        if peak <= 1.0 {
            return Ok(image);
        }
        let out = self.gpu.create(image.size)?;
        temps.push(out);
        let Self {
            gpu,
            input_tone,
            timer,
            ..
        } = self;
        let device = &gpu.context().device;
        let pass = input_tone.get_or_insert_with(|| InputTonePass::new(device));
        let src = gpu
            .texture(image.texture)
            .ok_or_else(|| MmError::Internal(format!("テクスチャ {} が無い", image.texture)))?;
        let dst = gpu.texture(out).expect("作ったばかり");
        pass.encode(device, encoder, (src, dst), peak, timer.pass_writes());
        Ok(LayerImage {
            texture: out,
            cached: false,
            ..image
        })
    }
}
