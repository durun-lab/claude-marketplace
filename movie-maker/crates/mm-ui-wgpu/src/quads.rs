//! 塗りつぶしの図形（矩形、ひし形、三角形）のバッチ描画。UI の部品は `mm-ui-layout` が `Quad` の並びにするので、描くのはこれだけ。

use bytemuck::{Pod, Zeroable};
use mm_ui_layout::{Quad, QuadShape};

const SHADER: &str = r#"
struct U { res: vec2<f32>, pad: vec2<f32> };
@group(0) @binding(0) var<uniform> u: U;

struct Out {
  @builtin(position) pos: vec4<f32>,
  @location(0) color: vec4<f32>,
  // 外接する矩形の中心を原点にした物理ピクセルの座標と、外接する矩形の幅と高さの半分。
  @location(1) local: vec2<f32>,
  @location(2) @interpolate(flat) half: vec2<f32>,
  @location(3) @interpolate(flat) shape: u32,
};

// 矩形のほかはアンチエイリアスで縁の外 0.5 物理ピクセルまで塗るので、1 物理ピクセル外へ広げて描く。
const EDGE_PAD: f32 = 1.0;

@vertex
fn vs(@builtin(vertex_index) vi: u32,
      @location(0) rect: vec4<f32>,
      @location(1) color: vec4<f32>,
      @location(2) shape: u32) -> Out {
  var corners = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
  var pad = EDGE_PAD;
  if (shape == 0u) { pad = 0.0; }
  let half = rect.zw * 0.5;
  let local = (corners[vi] * 2.0 - 1.0) * (half + pad);
  let p = rect.xy + half + local;
  var o: Out;
  o.pos = vec4<f32>(p.x / u.res.x * 2.0 - 1.0, 1.0 - p.y / u.res.y * 2.0, 0.0, 1.0);
  o.color = color;
  o.local = local;
  o.half = half;
  o.shape = shape;
  return o;
}

// 上向きの二等辺三角形の縁までの符号つき距離（外が正、物理ピクセル）。
fn tri_up(p: vec2<f32>, h: vec2<f32>) -> f32 {
  let n = normalize(vec2<f32>(2.0 * h.y, -h.x));
  let side = dot(vec2<f32>(abs(p.x), p.y + h.y), n);
  return max(side, p.y - h.y);
}

// 形の縁までの符号つき距離（外が正、物理ピクセル）。番号は `shape_id`。
fn edge_distance(shape: u32, p: vec2<f32>, h0: vec2<f32>) -> f32 {
  let h = max(h0, vec2<f32>(1e-3));
  switch shape {
    case 1u: {
      return (abs(p.x) / h.x + abs(p.y) / h.y - 1.0) / length(1.0 / h);
    }
    case 2u: { return tri_up(p, h); }
    case 3u: { return tri_up(vec2<f32>(p.x, -p.y), h); }
    case 4u: { return tri_up(vec2<f32>(p.y, p.x), h.yx); }
    case 5u: { return tri_up(vec2<f32>(p.y, -p.x), h.yx); }
    default: { return 0.0; }
  }
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
  if (in.shape == 0u) { return in.color; }
  let d = edge_distance(in.shape, in.local, in.half);
  let coverage = clamp(0.5 - d, 0.0, 1.0);
  if (coverage <= 0.0) { discard; }
  return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
"#;

/// 1 つ目の頂点バッファの 1 要素（外接する矩形、色、形の番号）。
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    rect: [f32; 4],
    color: [f32; 4],
    shape: u32,
}

/// `SHADER` の `edge_distance` の分岐と同じ番号。
const fn shape_id(shape: QuadShape) -> u32 {
    match shape {
        QuadShape::Rect => 0,
        QuadShape::Diamond => 1,
        QuadShape::TriUp => 2,
        QuadShape::TriDown => 3,
        QuadShape::TriLeft => 4,
        QuadShape::TriRight => 5,
    }
}

const INSTANCE_SIZE: u64 = std::mem::size_of::<Instance>() as u64;
/// 最初に確保するインスタンスの数。
const INITIAL_CAPACITY: u64 = 1024;

/// 矩形のバッチを描くパイプライン。
pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: u64,
}

impl QuadRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bgl = viewport_layout(device, "ui quads");
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui quads uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui quads"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        Self {
            pipeline: pipeline(device, format, &bgl),
            bind,
            uniform,
            instances: instance_buffer(device, INITIAL_CAPACITY),
            capacity: INITIAL_CAPACITY,
        }
    }

    /// 矩形を転送する。パスの外（`submit` の前）で呼ぶ。1 回の `submit` につき 1 回だけ。
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quads: &[Quad],
        viewport: (f32, f32),
    ) -> u32 {
        if quads.len() as u64 > self.capacity {
            self.capacity = (quads.len() as u64).next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
        let data: Vec<Instance> = quads
            .iter()
            .map(|q| Instance {
                rect: [q.rect.x, q.rect.y, q.rect.w, q.rect.h],
                color: q.color.0,
                shape: shape_id(q.shape),
            })
            .collect();
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&data));
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::cast_slice(&[viewport.0, viewport.1, 0.0, 0.0]),
        );
        data.len() as u32
    }

    /// `prepare` で渡した並びの `range` の矩形を描く。並びを分けて描くと、間に別のもの（文字）を挟める。
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, range: std::ops::Range<u32>) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, range);
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ui quads instances"),
        size: capacity * INSTANCE_SIZE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// 頂点のシェーダーがサーフェスの大きさだけを読むバインドグループの形。矩形と描き込みの線分で共有する。
pub(crate) fn viewport_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("ui quads"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("ui quads"),
        bind_group_layouts: &[Some(bgl)],
        immediate_size: 0,
    });
    let attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Uint32];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ui quads"),
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
