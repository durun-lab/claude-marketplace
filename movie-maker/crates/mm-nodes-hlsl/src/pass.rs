//! 変換した WGSL のフラグメントシェーダーを、全画面の三角形で 1 枚に描くパス。
//!
//! 束ね方は `shader.wgsl`（mm-nodes-basic の `gpu_pass`）と同じ: binding 0 が uniform、1.. が入力、その次がサンプラー。
//! 頂点シェーダーは自前の WGSL のモジュールで、変換した WGSL は `mm_fs` だけを持つ別のモジュールにする。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use mm_gpu::{GpuContext, WgpuGpu, FRAME_FORMAT};
use mm_types::{MmError, Result, Size};
use wgpu::util::DeviceExt;

/// 頂点シェーダー。`uv` は左上が (0, 0) で、HLSL の `TEXCOORD0`（location 0）に渡る。
const VERTEX_WGSL: &str = r"
struct MmVsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn mm_vs(@builtin(vertex_index) i: u32) -> MmVsOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: MmVsOut;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}
";

/// キャッシュするパイプラインの数の上限。
const MAX_PIPELINES: usize = 32;

/// 1 パスの入力。
pub(crate) struct Pass<'a> {
    /// `mm_fs` を持つ WGSL。
    pub fragment: &'a str,
    /// uniform の中身。大きさは 16 の倍数。
    pub uniform: &'a [u8],
    /// 入力の 2D テクスチャの番号。binding 1 から順に。
    pub inputs: &'a [u64],
    /// 出力の大きさ。
    pub size: Size,
}

struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

struct Entries {
    ctx: Weak<GpuContext>,
    map: HashMap<(usize, String), std::result::Result<Arc<Pipeline>, String>>,
}

/// パイプラインのキャッシュ。WGSL の全文と入力の数で引き、失敗も覚える。
#[derive(Default)]
pub(crate) struct PassCache {
    entries: Mutex<Option<Entries>>,
}

impl std::fmt::Debug for PassCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PassCache")
    }
}

fn layout_entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty,
        count: None,
    }
}

fn layout_entries(inputs: usize) -> Vec<wgpu::BindGroupLayoutEntry> {
    let mut entries = vec![layout_entry(
        0,
        wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
    )];
    for i in 0..inputs as u32 {
        entries.push(layout_entry(
            i + 1,
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
        ));
    }
    entries.push(layout_entry(
        inputs as u32 + 1,
        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
    ));
    entries
}

fn module(device: &wgpu::Device, label: &str, source: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

fn color_target() -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format: FRAME_FORMAT,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    }
}

/// 検証つきでパイプラインを作る。失敗は検証のメッセージ。
fn build(ctx: &GpuContext, fragment: &str, inputs: usize) -> std::result::Result<Pipeline, String> {
    let device = &ctx.device;
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let vs = module(device, "mm-nodes-hlsl vertex", VERTEX_WGSL);
    let fs = module(device, "mm-nodes-hlsl fragment", fragment);
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mm-nodes-hlsl layout"),
        entries: &layout_entries(inputs),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mm-nodes-hlsl pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mm-nodes-hlsl"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vs,
            entry_point: Some("mm_vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &fs,
            entry_point: Some("mm_fs"),
            compilation_options: Default::default(),
            targets: &[Some(color_target())],
        }),
        multiview_mask: None,
        cache: None,
    });
    if let Some(error) = pollster::block_on(scope.pop()) {
        return Err(error.to_string());
    }
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("mm-nodes-hlsl sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });
    Ok(Pipeline {
        pipeline,
        layout,
        sampler,
    })
}

impl PassCache {
    fn pipeline(&self, ctx: &Arc<GpuContext>, pass: &Pass<'_>) -> Result<Arc<Pipeline>> {
        let mut guard = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let current = Arc::downgrade(ctx);
        let entries = guard.get_or_insert_with(|| Entries {
            ctx: current.clone(),
            map: HashMap::new(),
        });
        if !Weak::ptr_eq(&entries.ctx, &current) || entries.map.len() >= MAX_PIPELINES {
            entries.ctx = current;
            entries.map.clear();
        }
        let key = (pass.inputs.len(), pass.fragment.to_string());
        let built = entries
            .map
            .entry(key)
            .or_insert_with(|| build(ctx, pass.fragment, pass.inputs.len()).map(Arc::new));
        built.clone().map_err(|message| {
            MmError::InvalidArgument(format!("変換した WGSL のパイプラインを作れない: {message}"))
        })
    }

    /// パスを描いて提出し、出力のテクスチャの番号を返す。
    pub(crate) fn run(&self, gpu: &mut WgpuGpu, pass: &Pass<'_>) -> Result<u64> {
        let ctx = gpu.context().clone();
        let pipeline = self.pipeline(&ctx, pass)?;
        let out = gpu.create(pass.size)?;
        let result = encode(&ctx, gpu, &pipeline, pass, out);
        if result.is_err() {
            gpu.release_id(out);
        }
        result.map(|()| out)
    }
}

fn bind_group(
    ctx: &GpuContext,
    gpu: &WgpuGpu,
    pipeline: &Pipeline,
    pass: &Pass<'_>,
) -> Result<wgpu::BindGroup> {
    let device = &ctx.device;
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("mm-nodes-hlsl uniform"),
        contents: pass.uniform,
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let views = pass
        .inputs
        .iter()
        .map(|id| {
            gpu.texture(*id)
                .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
                .ok_or_else(|| MmError::NotFound(format!("入力のテクスチャ {id}")))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut entries = vec![wgpu::BindGroupEntry {
        binding: 0,
        resource: uniform.as_entire_binding(),
    }];
    for (i, view) in views.iter().enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: i as u32 + 1,
            resource: wgpu::BindingResource::TextureView(view),
        });
    }
    entries.push(wgpu::BindGroupEntry {
        binding: views.len() as u32 + 1,
        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
    });
    Ok(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mm-nodes-hlsl"),
        layout: &pipeline.layout,
        entries: &entries,
    }))
}

fn encode(
    ctx: &GpuContext,
    gpu: &WgpuGpu,
    pipeline: &Pipeline,
    pass: &Pass<'_>,
    out: u64,
) -> Result<()> {
    let group = bind_group(ctx, gpu, pipeline, pass)?;
    let target = gpu
        .texture(out)
        .ok_or_else(|| MmError::Internal("作ったばかりの出力が無い".into()))?
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mm-nodes-hlsl"),
        });
    {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mm-nodes-hlsl"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            timestamp_writes: gpu.timer().pass_writes(),
            ..Default::default()
        });
        rp.set_pipeline(&pipeline.pipeline);
        rp.set_bind_group(0, &group, &[]);
        rp.draw(0..3, 0..1);
    }
    // 借用しているデコーダーのフレームが入力なら、その完了を待たせてから提出する。
    gpu.before_submit();
    ctx.queue.submit([encoder.finish()]);
    Ok(())
}
