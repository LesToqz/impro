//! The instanced OpenGL scene shared by the offscreen benchmark and the
//! GTK `GLArea` probe: edges as instanced thick-line quads, nodes as instanced
//! quads with a circle SDF fragment shader, labels as instanced quads from
//! one atlas texture. Drawing targets whatever framebuffer is bound.

use std::sync::Arc;

use glow::HasContext;

use crate::fixture::{Fixture, BG, EDGE_COLOR, NODE_COLORS};
use crate::harness::{Cam, LabelDraw};
use crate::labels::LabelCache;

const EDGE_VS: &str = r#"#version 330 core
uniform vec4 u_cam; uniform vec2 u_view;
layout(location=0) in vec2 a_corner; layout(location=1) in vec4 a_edge;
out float v_d;
void main(){
  vec2 p0 = (a_edge.xy*u_cam.z + u_cam.xy)*u_cam.w;
  vec2 p1 = (a_edge.zw*u_cam.z + u_cam.xy)*u_cam.w;
  vec2 dir = p1 - p0; float len = max(length(dir), 1e-4); dir /= len;
  vec2 nrm = vec2(-dir.y, dir.x);
  vec2 p = mix(p0, p1, a_corner.x) + dir*(a_corner.x*2.0-1.0)*0.5 + nrm*a_corner.y;
  v_d = a_corner.y;
  gl_Position = vec4(p/u_view*2.0-1.0, 0.0, 1.0);
}"#;
const EDGE_FS: &str = r#"#version 330 core
uniform vec4 u_color; in float v_d; out vec4 o;
void main(){ o = vec4(u_color.rgb, u_color.a*clamp(1.0-abs(v_d), 0.0, 1.0)); }"#;
const NODE_VS: &str = r#"#version 330 core
uniform vec4 u_cam; uniform vec2 u_view; uniform vec3 u_colors[3];
layout(location=0) in vec2 a_corner; layout(location=1) in vec4 a_node;
out vec2 v_p; out float v_r; out vec3 v_col;
void main(){
  vec2 c = (a_node.xy*u_cam.z + u_cam.xy)*u_cam.w;
  float r = max(a_node.z*sqrt(u_cam.z), 0.8)*u_cam.w;
  v_p = a_corner*(r+1.0); v_r = r; v_col = u_colors[int(a_node.w)];
  gl_Position = vec4((c + v_p)/u_view*2.0-1.0, 0.0, 1.0);
}"#;
const NODE_FS: &str = r#"#version 330 core
in vec2 v_p; in float v_r; in vec3 v_col; out vec4 o;
void main(){ o = vec4(v_col, clamp(v_r - length(v_p) + 0.5, 0.0, 1.0)); }"#;
const LABEL_VS: &str = r#"#version 330 core
uniform vec2 u_view;
layout(location=0) in vec2 a_corner; layout(location=1) in vec4 a_rect; layout(location=2) in vec4 a_uv;
out vec2 v_uv;
void main(){
  vec2 c = a_corner*0.5+0.5;
  v_uv = mix(a_uv.xy, a_uv.zw, c);
  gl_Position = vec4((a_rect.xy + c*a_rect.zw)/u_view*2.0-1.0, 0.0, 1.0);
}"#;
const LABEL_FS: &str = r#"#version 330 core
uniform sampler2D u_atlas; in vec2 v_uv; out vec4 o;
void main(){ o = vec4(0.9, 0.93, 0.96, 0.9*texture(u_atlas, v_uv).a); }"#;

const ATLAS: i32 = 2048;

pub fn bytes<T>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

unsafe fn program(gl: &glow::Context, vs: &str, fs: &str) -> glow::Program {
    let p = gl.create_program().expect("program");
    for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
        let s = gl.create_shader(kind).expect("shader");
        gl.shader_source(s, src);
        gl.compile_shader(s);
        assert!(gl.get_shader_compile_status(s), "{}", gl.get_shader_info_log(s));
        gl.attach_shader(p, s);
    }
    gl.link_program(p);
    assert!(gl.get_program_link_status(p), "{}", gl.get_program_info_log(p));
    p
}

struct Pipe {
    prog: glow::Program,
    vao: glow::VertexArray,
}

/// Vertex array with the shared corner buffer on attribute 0 and a static
/// per-instance buffer on attribute 1 (and 2 for the label buffer).
unsafe fn pipe(
    gl: &glow::Context,
    vs: &str,
    fs: &str,
    corners: &[f32; 8],
    inst: Option<(&[f32], i32)>,
) -> (Pipe, Option<glow::Buffer>) {
    let prog = program(gl, vs, fs);
    let vao = gl.create_vertex_array().expect("vao");
    gl.bind_vertex_array(Some(vao));
    let vb = gl.create_buffer().expect("vbo");
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vb));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(corners), glow::STATIC_DRAW);
    gl.enable_vertex_attrib_array(0);
    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
    let mut buf = None;
    if let Some((data, _)) = inst {
        let b = gl.create_buffer().expect("instance buffer");
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(b));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(data), glow::STATIC_DRAW);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, 16, 0);
        gl.vertex_attrib_divisor(1, 1);
        buf = Some(b);
    }
    (Pipe { prog, vao }, buf)
}


pub struct GlScene {
    gl: Arc<glow::Context>,
    edge: Pipe,
    node: Pipe,
    label: Pipe,
    label_inst: glow::Buffer,
    atlas: glow::Texture,
    edge_count: i32,
    node_count: i32,
    scale: f32,
    pub labels: LabelCache,
    placed: Vec<Option<[f32; 4]>>,
    shelf: (i32, i32, i32),
    inst: Vec<f32>,
}

impl GlScene {
    /// Uploads the static instance data. `scale`: device pixels per logical pixel.
    pub fn new(gl: Arc<glow::Context>, fx: &Fixture, scale: u32) -> GlScene {
        let edge_inst: Vec<f32> = fx
            .edges
            .iter()
            .flat_map(|&(a, b)| {
                let (a, b) = (a as usize, b as usize);
                [fx.x[a], fx.y[a], fx.x[b], fx.y[b]]
            })
            .collect();
        let node_inst: Vec<f32> = (0..fx.x.len())
            .flat_map(|i| [fx.x[i], fx.y[i], fx.radius[i], fx.class[i] as f32])
            .collect();
        let edge_corners: [f32; 8] = [0., -1., 0., 1., 1., -1., 1., 1.];
        let quad: [f32; 8] = [-1., -1., 1., -1., -1., 1., 1., 1.];
        unsafe {
            let (edge, _) = pipe(&gl, EDGE_VS, EDGE_FS, &edge_corners, Some((&edge_inst, 0)));
            let (node, _) = pipe(&gl, NODE_VS, NODE_FS, &quad, Some((&node_inst, 0)));
            let (label, buf) = pipe(&gl, LABEL_VS, LABEL_FS, &quad, Some((&[0.0; 4], 0)));
            // label instances: rect (attribute 1) + uv (attribute 2), 8 floats each
            let label_inst = buf.expect("label buffer");
            gl.bind_vertex_array(Some(label.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(label_inst));
            gl.buffer_data_size(glow::ARRAY_BUFFER, 300 * 32, glow::DYNAMIC_DRAW);
            gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, 32, 0);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, 32, 16);
            gl.vertex_attrib_divisor(2, 1);
            let atlas = gl.create_texture().expect("atlas");
            gl.bind_texture(glow::TEXTURE_2D, Some(atlas));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                ATLAS,
                ATLAS,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
            gl.bind_vertex_array(None);
            GlScene {
                gl,
                edge,
                node,
                label,
                label_inst,
                atlas,
                edge_count: fx.edges.len() as i32,
                node_count: fx.x.len() as i32,
                scale: scale as f32,
                labels: LabelCache::new(scale as f64),
                placed: Vec::new(),
                shelf: (0, 0, 0),
                inst: Vec::new(),
            }
        }
    }

    /// Atlas rectangle `[x, y, w, h]` in texels of label `id`, uploading it
    /// on first use (shelf packing).
    unsafe fn place(&mut self, id: usize) -> [f32; 4] {
        if self.placed.len() <= id {
            self.placed.resize(id + 1, None);
        }
        if let Some(r) = self.placed[id] {
            return r;
        }
        let gl = &self.gl;
        let b = &self.labels.items[id];
        let (x, y, row_h) = &mut self.shelf;
        if *x + b.width > ATLAS {
            *y += *row_h;
            *x = 0;
            *row_h = 0;
        }
        gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            0,
            *x,
            *y,
            b.width,
            b.height,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&b.data)),
        );
        let r = [*x as f32, *y as f32, b.width as f32, b.height as f32];
        *x += b.width;
        *row_h = (*row_h).max(b.height);
        self.placed[id] = Some(r);
        r
    }

    /// Waits until the GPU finished everything submitted so far.
    pub fn finish(&self) {
        unsafe { self.gl.finish() };
    }

    /// Clears the bound `dw × dh` framebuffer and draws edges, nodes and labels.
    pub fn draw(&mut self, fx: &Fixture, cam: &Cam, labels: &[LabelDraw], dw: i32, dh: i32) {
        let gl = self.gl.clone();
        let (dwf, dhf) = (dw as f32, dh as f32);
        let cam4 = [cam.x as f32, cam.y as f32, cam.zoom as f32, self.scale];
        unsafe {
            gl.viewport(0, 0, dw, dh);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA, glow::ZERO, glow::ONE);
            gl.clear_color(BG[0], BG[1], BG[2], 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            let set = |p: glow::Program| {
                gl.use_program(Some(p));
                gl.uniform_4_f32_slice(gl.get_uniform_location(p, "u_cam").as_ref(), &cam4);
                gl.uniform_2_f32(gl.get_uniform_location(p, "u_view").as_ref(), dwf, dhf);
            };
            set(self.edge.prog);
            gl.uniform_4_f32_slice(gl.get_uniform_location(self.edge.prog, "u_color").as_ref(), &EDGE_COLOR);
            gl.bind_vertex_array(Some(self.edge.vao));
            gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, self.edge_count);
            set(self.node.prog);
            let colors: Vec<f32> = NODE_COLORS.iter().flatten().copied().collect();
            gl.uniform_3_f32_slice(gl.get_uniform_location(self.node.prog, "u_colors").as_ref(), &colors);
            gl.bind_vertex_array(Some(self.node.vao));
            gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, self.node_count);
            if !labels.is_empty() {
                self.inst.clear();
                for l in labels {
                    let id = self.labels.get(&fx.names[l.node as usize], l.size);
                    let r = self.place(id);
                    let s = self.scale;
                    let x = (l.x * s - r[2] / 2.0).round();
                    let y = (l.y * s).round();
                    let a = ATLAS as f32;
                    self.inst.extend_from_slice(&[x, y, r[2], r[3], r[0] / a, r[1] / a, (r[0] + r[2]) / a, (r[1] + r[3]) / a]);
                }
                gl.use_program(Some(self.label.prog));
                gl.uniform_2_f32(gl.get_uniform_location(self.label.prog, "u_view").as_ref(), dwf, dhf);
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas));
                gl.uniform_1_i32(gl.get_uniform_location(self.label.prog, "u_atlas").as_ref(), 0);
                gl.bind_vertex_array(Some(self.label.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.label_inst));
                gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, bytes(&self.inst));
                gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, labels.len() as i32);
            }
            gl.bind_vertex_array(None);
        }
    }
}
