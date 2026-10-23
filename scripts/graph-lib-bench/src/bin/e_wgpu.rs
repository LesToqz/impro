//! Candidate E: wgpu, offscreen (no surface). Viability probe: lists the
//! adapters of every backend, then draws the same instanced scene (edges and
//! circle-SDF nodes, no labels) into an RGBA texture and waits for the GPU.
//! Flags: `--backend vulkan|gl|all` (default all, first adapter wins).

use bytemuck::{Pod, Zeroable};
use graph_lib_bench::fixture::{self, Fixture, BG, EDGE_COLOR, NODE_COLORS};
use graph_lib_bench::harness::{header, sweep, Cam, LabelDraw, Opts, Renderer};
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct U { cam: vec4<f32>, view: vec2<f32>, pad: vec2<f32>, edge: vec4<f32>, colors: array<vec4<f32>, 3> };
@group(0) @binding(0) var<uniform> u: U;
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) a: f32, @location(1) b: f32, @location(2) c: vec3<f32>, @location(3) p: vec2<f32> };
fn to_clip(p: vec2<f32>) -> vec4<f32> { return vec4<f32>(p / u.view * 2.0 - 1.0, 0.0, 1.0); }
@vertex fn vs_edge(@builtin(vertex_index) vi: u32, @location(0) e: vec4<f32>) -> VOut {
  var corners = array<vec2<f32>, 4>(vec2(0.0,-1.0), vec2(0.0,1.0), vec2(1.0,-1.0), vec2(1.0,1.0));
  let c = corners[vi];
  let p0 = (e.xy * u.cam.z + u.cam.xy) * u.cam.w;
  let p1 = (e.zw * u.cam.z + u.cam.xy) * u.cam.w;
  var dir = p1 - p0; let len = max(length(dir), 1e-4); dir = dir / len;
  let nrm = vec2(-dir.y, dir.x);
  let p = mix(p0, p1, c.x) + dir * (c.x * 2.0 - 1.0) * 0.5 + nrm * c.y;
  var o: VOut; o.pos = to_clip(p); o.a = c.y; o.b = 0.0; o.c = vec3(0.0); o.p = vec2(0.0); return o;
}
@fragment fn fs_edge(i: VOut) -> @location(0) vec4<f32> {
  return vec4(u.edge.rgb, u.edge.a * clamp(1.0 - abs(i.a), 0.0, 1.0));
}
@vertex fn vs_node(@builtin(vertex_index) vi: u32, @location(0) n: vec4<f32>) -> VOut {
  var corners = array<vec2<f32>, 4>(vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(-1.0,1.0), vec2(1.0,1.0));
  let c = (n.xy * u.cam.z + u.cam.xy) * u.cam.w;
  let r = max(n.z * sqrt(u.cam.z), 0.8) * u.cam.w;
  let q = corners[vi] * (r + 1.0);
  var o: VOut; o.pos = to_clip(c + q); o.a = 0.0; o.b = r; o.c = u.colors[u32(n.w)].rgb; o.p = q; return o;
}
@fragment fn fs_node(i: VOut) -> @location(0) vec4<f32> {
  return vec4(i.c, clamp(i.b - length(i.p) + 0.5, 0.0, 1.0));
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    cam: [f32; 4],
    view: [f32; 2],
    pad: [f32; 2],
    edge: [f32; 4],
    colors: [[f32; 4]; 3],
}

struct Wg {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture_view: wgpu::TextureView,
    _texture: wgpu::Texture,
    ubuf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    edge_pipe: wgpu::RenderPipeline,
    node_pipe: wgpu::RenderPipeline,
    edges: wgpu::Buffer,
    nodes: wgpu::Buffer,
    edge_count: u32,
    node_count: u32,
    scale: f32,
    dw: u32,
    dh: u32,
    info: String,
}

impl Renderer for Wg {
    fn frame(&mut self, _fx: &Fixture, cam: &Cam, _labels: &[LabelDraw]) {
        let u = Uniforms {
            cam: [cam.x as f32, cam.y as f32, cam.zoom as f32, self.scale],
            view: [self.dw as f32, self.dh as f32],
            pad: [0.0; 2],
            edge: EDGE_COLOR,
            colors: [
                [NODE_COLORS[0][0], NODE_COLORS[0][1], NODE_COLORS[0][2], 1.0],
                [NODE_COLORS[1][0], NODE_COLORS[1][1], NODE_COLORS[1][2], 1.0],
                [NODE_COLORS[2][0], NODE_COLORS[2][1], NODE_COLORS[2][2], 1.0],
            ],
        };
        self.queue.write_buffer(&self.ubuf, 0, bytemuck::bytes_of(&u));
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: BG[0] as f64,
                            g: BG[1] as f64,
                            b: BG[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_pipeline(&self.edge_pipe);
            pass.set_vertex_buffer(0, self.edges.slice(..));
            pass.draw(0..4, 0..self.edge_count);
            pass.set_pipeline(&self.node_pipe);
            pass.set_vertex_buffer(0, self.nodes.slice(..));
            pass.draw(0..4, 0..self.node_count);
        }
        self.queue.submit([enc.finish()]);
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
    }
    fn note(&self) -> String {
        self.info.clone()
    }
}

fn pipeline(device: &wgpu::Device, module: &wgpu::ShaderModule, layout: &wgpu::PipelineLayout, vs: &str, fs: &str) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 16,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn main() {
    let o = Opts::parse();
    let fx = fixture::by_name(&o.fixture);
    let backends = match o.value("--backend") {
        Some("vulkan") => wgpu::Backends::VULKAN,
        Some("gl") => wgpu::Backends::GL,
        _ => wgpu::Backends::all(),
    };
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    for a in pollster::block_on(instance.enumerate_adapters(backends)) {
        let i = a.get_info();
        println!("adapter: {} / backend {:?} / type {:?} / driver `{}` {}", i.name, i.backend, i.device_type, i.driver, i.driver_info);
    }
    header("E wgpu offscreen", &o);
    sweep(&fx, &o, |w, h, s| {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        let ai = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
        let (dw, dh) = (w * s, h * s);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: dw, height: dh, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&Default::default());
        let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let edge_data: Vec<f32> = fx
            .edges
            .iter()
            .flat_map(|&(a, b)| [fx.x[a as usize], fx.y[a as usize], fx.x[b as usize], fx.y[b as usize]])
            .collect();
        let node_data: Vec<f32> = (0..fx.x.len()).flat_map(|i| [fx.x[i], fx.y[i], fx.radius[i], fx.class[i] as f32]).collect();
        let edges = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&edge_data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let nodes = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&node_data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Some(Box::new(Wg {
            edge_pipe: pipeline(&device, &module, &layout, "vs_edge", "fs_edge"),
            node_pipe: pipeline(&device, &module, &layout, "vs_node", "fs_node"),
            device,
            queue,
            texture_view,
            _texture: texture,
            ubuf,
            bind,
            edges,
            nodes,
            edge_count: fx.edges.len() as u32,
            node_count: fx.x.len() as u32,
            scale: s as f32,
            dw,
            dh,
            info: format!("{} / {:?}", ai.name, ai.backend),
        }))
    });
}
