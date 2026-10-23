//! Candidate D: raw OpenGL (glow) on a surfaceless EGL context, drawing the
//! instanced scene of `glscene` into an offscreen renderbuffer. Flags:
//! `--readback` (glReadPixels into a cairo surface after drawing),
//! `--nofinish` (time submission only), `--gl 3.3` (context version).

use glow::HasContext;
use graph_lib_bench::egl::{self, Egl};
use graph_lib_bench::fixture::{self, Fixture};
use graph_lib_bench::glscene::GlScene;
use graph_lib_bench::harness::{header, sweep, Cam, LabelDraw, Opts, Renderer};

struct Gl {
    egl: Egl,
    scene: GlScene,
    fbo: glow::Framebuffer,
    finish: bool,
    readback: Option<cairo::ImageSurface>,
    dw: i32,
    dh: i32,
}

impl Renderer for Gl {
    fn frame(&mut self, fx: &Fixture, cam: &Cam, labels: &[LabelDraw]) {
        let gl = self.egl.gl.clone();
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            self.scene.draw(fx, cam, labels, self.dw, self.dh);
            if let Some(surface) = &mut self.readback {
                {
                    let mut data = surface.data().expect("pixels");
                    gl.read_pixels(
                        0,
                        0,
                        self.dw,
                        self.dh,
                        glow::BGRA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelPackData::Slice(Some(&mut data)),
                    );
                }
                surface.mark_dirty();
            } else if self.finish {
                gl.finish();
            }
        }
    }
    fn note(&self) -> String {
        format!("label renders {} ({:.0} ms)", self.scene.labels.renders, self.scene.labels.render_ms)
    }
}

fn main() {
    let o = Opts::parse();
    let fx = fixture::by_name(&o.fixture);
    let (maj, min) = o
        .value("--gl")
        .and_then(|v| v.split_once('.'))
        .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
        .unwrap_or((3, 3));
    let mut printed = false;
    header(
        &format!("D raw GL/glow instanced (readback={}, finish={})", o.has("--readback"), !o.has("--nofinish")),
        &o,
    );
    sweep(&fx, &o, |w, h, s| {
        let egl = egl::open(maj, min).map_err(|e| eprintln!("EGL: {e}")).ok()?;
        if !printed {
            eprintln!("{}", egl.info);
            printed = true;
        }
        let gl = egl.gl.clone();
        let (dw, dh) = ((w * s) as i32, (h * s) as i32);
        let fbo = unsafe {
            let fbo = gl.create_framebuffer().ok()?;
            let rbo = gl.create_renderbuffer().ok()?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rbo));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, dw, dh);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(rbo));
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                eprintln!("framebuffer {dw}x{dh} incomplete");
                return None;
            }
            fbo
        };
        let scene = GlScene::new(gl, &fx, s);
        let readback = o
            .has("--readback")
            .then(|| cairo::ImageSurface::create(cairo::Format::ARgb32, dw, dh).expect("surface"));
        Some(Box::new(Gl {
            egl,
            scene,
            fbo,
            finish: !o.has("--nofinish"),
            readback,
            dw,
            dh,
        }))
    });
}
