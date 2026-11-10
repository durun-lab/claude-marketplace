//! アトラスのテクスチャと、サムネイルを 1 回の呼び出しで描くパイプライン。

use bytemuck::{Pod, Zeroable};

use super::{CELL_H, CELL_W, COLS, MIPS, ROWS};

const SHADER: &str = r#"
struct U { res: vec2<f32>, pad: vec2<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct Out {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) alpha: f32,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32,
      @location(0) rect: vec4<f32>,
      @location(1) uv: vec4<f32>,
      @location(2) alpha: f32) -> Out {
  var corners = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
  let c = corners[vi];
  let p = rect.xy + c * rect.zw;
  var o: Out;
  o.pos = vec4<f32>(p.x / u.res.x * 2.0 - 1.0, 1.0 - p.y / u.res.y * 2.0, 0.0, 1.0);
  o.uv = mix(uv.xy, uv.zw, c);
  o.alpha = alpha;
  return o;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
  let c = textureSample(atlas, samp, in.uv);
  return vec4<f32>(c.rgb, c.a * in.alpha);
}
"#;

/// 頂点バッファの 1 要素（置く矩形、アトラスの中の範囲の左上と右下、不透明度）。
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Instance {
    pub(super) rect: [f32; 4],
    pub(super) uv: [f32; 4],
    pub(super) alpha: f32,
}

const INSTANCE_SIZE: u64 = std::mem::size_of::<Instance>() as u64;
const INITIAL_CAPACITY: u64 = 256;

/// アトラスのテクスチャと描くための資源。最初のサムネイルが届くまで作らない（サムネイルの無いプロジェクトでは GPU のメモリを使わない）。
pub(super) struct Gpu {
    pub(super) texture: wgpu::Texture,
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) bind: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    pub(super) instances: wgpu::Buffer,
    capacity: u64,
}

impl Gpu {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let texture = atlas_texture(device);
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui thumbs uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = bind_layout(device);
        let bind = bind_group(device, &bgl, &uniform, &texture);
        Self {
            texture,
            pipeline: pipeline(device, format, &bgl),
            bind,
            uniform,
            instances: instance_buffer(device, INITIAL_CAPACITY),
            capacity: INITIAL_CAPACITY,
        }
    }

    pub(super) fn write(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[Instance],
        viewport: (f32, f32),
    ) {
        if instances.len() as u64 > self.capacity {
            self.capacity = (instances.len() as u64).next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::cast_slice(&[viewport.0, viewport.1, 0.0, 0.0]),
        );
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ui thumbs instances"),
        size: capacity * INSTANCE_SIZE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// スロットを `COLS` x `ROWS` に並べたアトラスのテクスチャ。
fn atlas_texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("ui thumbs atlas"),
        size: wgpu::Extent3d {
            width: COLS * CELL_W,
            height: ROWS * CELL_H,
            depth_or_array_layers: 1,
        },
        mip_level_count: MIPS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // JPEG の値は表示の値のまま（UI の色と同じく、sRGB の変換を掛けない）。
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    texture: &wgpu::Texture,
) -> wgpu::BindGroup {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let view = texture.create_view(&Default::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ui thumbs"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    })
}

fn bind_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ui thumbs"),
        entries: &[
            entry(
                0,
                wgpu::ShaderStages::VERTEX,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            entry(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
        ],
    })
}

fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("ui thumbs"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("ui thumbs"),
        bind_group_layouts: &[Some(bgl)],
        immediate_size: 0,
    });
    let attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ui thumbs"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: INSTANCE_SIZE,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &attrs,
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
