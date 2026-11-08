//! コメントの描き込みの線分のバッチ描画。線分ごとに太さぶんの矩形を張り、フラグメントで線分からの距離を測って
//! 端と継ぎ目を丸める。太さはサーフェスの画素で決まるので、映像の拡大に依らず一定になる。

use bytemuck::{Pod, Zeroable};
use mm_ui_layout::{Color, MarkSegment};

const SHADER: &str = r#"
struct U { res: vec2<f32>, pad: vec2<f32> };
@group(0) @binding(0) var<uniform> u: U;

struct Out {
  @builtin(position) pos: vec4<f32>,
  @location(0) local: vec2<f32>,
  @location(1) @interpolate(flat) seg: vec4<f32>,
  @location(2) @interpolate(flat) radius: f32,
  @location(3) color: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32,
      @location(0) seg: vec4<f32>,
      @location(1) params: vec4<f32>,
      @location(2) color: vec4<f32>) -> Out {
  var corners = array<vec2<f32>, 6>(
    vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0));
  let a = seg.xy;
  let delta = seg.zw - a;
  let len = length(delta);
  var dir = vec2<f32>(1.0, 0.0);
  if (len > 0.0001) { dir = delta / len; }
  let n = vec2<f32>(-dir.y, dir.x);
  // 縁のぼかしの 1 画素ぶん広げた矩形。
  let r = params.x + 1.0;
  let c = corners[vi];
  let p = a - dir * r + dir * (len + 2.0 * r) * c.x + n * r * c.y;
  var o: Out;
  o.pos = vec4<f32>(p.x / u.res.x * 2.0 - 1.0, 1.0 - p.y / u.res.y * 2.0, 0.0, 1.0);
  o.local = p;
  o.seg = seg;
  o.radius = params.x;
  o.color = color;
  return o;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
  let a = in.seg.xy;
  let ab = in.seg.zw - a;
  let t = clamp(dot(in.local - a, ab) / max(dot(ab, ab), 0.000001), 0.0, 1.0);
  let d = length(in.local - (a + ab * t));
  let cover = clamp(in.radius + 0.5 - d, 0.0, 1.0);
  if (cover <= 0.0) { discard; }
  return vec4<f32>(in.color.rgb, in.color.a * cover);
}
"#;

/// 頂点バッファの 1 要素（線分の両端、半径、色）。
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    seg: [f32; 4],
    params: [f32; 4],
    color: [f32; 4],
}

const INSTANCE_SIZE: u64 = std::mem::size_of::<Instance>() as u64;
/// 最初に確保する線分の数。指でなぞった線 1 本はたいてい数十〜数百の点。
const INITIAL_CAPACITY: u64 = 256;

/// 描き込みの線分を描くパイプライン。
pub struct MarkRenderer {
    pipeline: wgpu::RenderPipeline,
    bind: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: u64,
}

impl MarkRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bgl = crate::quads::viewport_layout(device, "ui marks");
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui marks uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui marks"),
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

    /// 線分を転送する。パスの外（`submit` の前）で呼び、描く数を返す。
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        segments: &[MarkSegment],
        color: Color,
        viewport: (f32, f32),
    ) -> u32 {
        if segments.is_empty() {
            return 0;
        }
        if segments.len() as u64 > self.capacity {
            self.capacity = (segments.len() as u64).next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
        let data: Vec<Instance> = segments
            .iter()
            .map(|s| Instance {
                seg: [s.a[0], s.a[1], s.b[0], s.b[1]],
                params: [s.width / 2.0, 0.0, 0.0, 0.0],
                color: color.0,
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

    /// `prepare` が返した数だけ描く。
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, count: u32) {
        if count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, 0..count);
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ui marks instances"),
        size: capacity * INSTANCE_SIZE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("ui marks"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("ui marks"),
        bind_group_layouts: &[Some(bgl)],
        immediate_size: 0,
    });
    let attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ui marks"),
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
