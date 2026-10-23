//! Candidate D2: femtovg (vector library on OpenGL) on a surfaceless EGL
//! context, rendering into an offscreen image target. Paths are tessellated
//! on the CPU every frame and drawn by the library; labels use its own text
//! engine (glyph atlas).

use femtovg::renderer::OpenGl;
use femtovg::{Align, Baseline, Canvas, Color, FontId, ImageFlags, ImageId, Paint, Path, PixelFormat, RenderTarget};
use glow::HasContext;
use graph_lib_bench::egl::{self, Egl};
use graph_lib_bench::fixture::{self, Fixture, BG, EDGE_COLOR, NODE_COLORS};
use graph_lib_bench::harness::{header, sweep, Cam, LabelDraw, Opts, Renderer};

struct Vg {
    egl: Egl,
    canvas: Canvas<OpenGl>,
    target: ImageId,
    font: FontId,
    scale: f32,
    dw: u32,
    dh: u32,
}

impl Renderer for Vg {
    fn frame(&mut self, fx: &Fixture, cam: &Cam, labels: &[LabelDraw]) {
        let s = self.scale;
        let (w, h) = (self.dw as f32, self.dh as f32);
        let c = &mut self.canvas;
        c.set_render_target(RenderTarget::Image(self.target));
        c.clear_rect(0, 0, self.dw, self.dh, Color::rgbf(BG[0], BG[1], BG[2]));
        let n = fx.x.len();
        let mut sx = Vec::with_capacity(n);
        let mut sy = Vec::with_capacity(n);
        for i in 0..n {
            let (x, y) = cam.to_screen(fx.x[i], fx.y[i]);
            sx.push(x * s);
            sy.push(y * s);
        }
        let mut path = Path::new();
        for &(a, b) in &fx.edges {
            let (a, b) = (a as usize, b as usize);
            let (x0, y0, x1, y1) = (sx[a], sy[a], sx[b], sy[b]);
            if (x0 < 0.0 && x1 < 0.0) || (y0 < 0.0 && y1 < 0.0) || (x0 > w && x1 > w) || (y0 > h && y1 > h) {
                continue;
            }
            path.move_to(x0, y0);
            path.line_to(x1, y1);
        }
        let mut edge_paint = Paint::color(Color::rgbaf(EDGE_COLOR[0], EDGE_COLOR[1], EDGE_COLOR[2], EDGE_COLOR[3]));
        edge_paint.set_line_width(1.0);
        c.stroke_path(&path, &edge_paint);
        for (class, col) in NODE_COLORS.iter().enumerate() {
            let mut path = Path::new();
            for i in 0..n {
                if fx.class[i] as usize != class {
                    continue;
                }
                let r = cam.screen_radius(fx.radius[i]).max(0.8) * s;
                if sx[i] + r < 0.0 || sx[i] - r > w || sy[i] + r < 0.0 || sy[i] - r > h {
                    continue;
                }
                path.circle(sx[i], sy[i], r);
            }
            c.fill_path(&path, &Paint::color(Color::rgbf(col[0], col[1], col[2])));
        }
        let mut text = Paint::color(Color::rgbaf(0.9, 0.93, 0.96, 0.9));
        text.set_font(&[self.font]);
        text.set_text_align(Align::Center);
        text.set_text_baseline(Baseline::Top);
        for l in labels {
            text.set_font_size(l.size * s);
            let _ = c.fill_text(l.x * s, l.y * s, &fx.names[l.node as usize], &text);
        }
        c.flush();
        unsafe { self.egl.gl.finish() };
    }
}

fn main() {
    let o = Opts::parse();
    let fx = fixture::by_name(&o.fixture);
    let mut printed = false;
    header("D2 femtovg on surfaceless EGL", &o);
    sweep(&fx, &o, |w, h, s| {
        let egl = egl::open(3, 3).map_err(|e| eprintln!("EGL: {e}")).ok()?;
        if !printed {
            eprintln!("{}", egl.info);
            printed = true;
        }
        let renderer = unsafe {
            OpenGl::new_from_function(|name| {
                egl.instance
                    .get_proc_address(name)
                    .map_or(std::ptr::null(), |p| p as *const _)
            })
        }
        .map_err(|e| eprintln!("femtovg renderer: {e:?}"))
        .ok()?;
        let mut canvas = Canvas::new(renderer).map_err(|e| eprintln!("canvas: {e:?}")).ok()?;
        let (dw, dh) = (w * s, h * s);
        canvas.set_size(dw, dh, 1.0);
        let target = canvas
            .create_image_empty(dw as usize, dh as usize, PixelFormat::Rgba8, ImageFlags::FLIP_Y)
            .map_err(|e| eprintln!("target image: {e:?}"))
            .ok()?;
        let font = canvas
            .add_font("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")
            .map_err(|e| eprintln!("font: {e:?}"))
            .ok()?;
        let _ = unsafe { egl.gl.get_error() };
        Some(Box::new(Vg {
            egl,
            canvas,
            target,
            font,
            scale: s as f32,
            dw,
            dh,
        }))
    });
}
