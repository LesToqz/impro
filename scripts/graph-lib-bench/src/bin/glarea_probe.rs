//! On-screen probe under a real GTK 3.24 toplevel: can a `GLArea` be created
//! and render the instanced scene, and what do frames cost to present
//! compared with a `DrawingArea` painting a cairo surface? Run under Xvfb
//! (`GDK_BACKEND=x11`) or the private Wayland compositor
//! (`GDK_BACKEND=wayland`), never the user's session. Prints the frame-clock
//! interval of each frame and the time spent in the draw/render handler.
//!
//! Env: `PROBE_MODE=gl|blit|cairo|current` (default `gl`; `blit` repaints one
//! pre-rendered surface, `cairo` renders candidate B every frame, `current`
//! the real Painter), `PROBE_FRAMES`, `PROBE_SIZE=WxH`, `PROBE_FINISH=1`
//! (glFinish inside the GL handler), `PROBE_LABELS=1`, `PROBE_MOTION=1`
//! (moving level of detail of the real Painter).

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;


use glow::HasContext;
use graph_lib_bench::cairo_scene::Plain;
use graph_lib_bench::cur_scene::build_scene;
use graph_lib_bench::fixture::{self, Fixture};
use graph_lib_bench::glscene::GlScene;
use graph_lib_bench::harness::{pan_cam, pan_fit_cam, percentile, pick_labels, zoom_cam, Cam, LabelDraw, Renderer, FRAMES};
use gtk::prelude::*;
use impro_graph::geometry::Camera;
use impro_graph::view::draw::{PaintOptions, Painter};
use impro_graph::view::scene::Scene;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Gl,
    Blit,
    Cairo,
    Current,
}

struct State {
    fx: Fixture,
    cam: Cam,
    labels: Vec<LabelDraw>,
    with_labels: bool,
    finish: bool,
    gl: Option<GlScene>,
    plain: Option<Plain>,
    blit: Option<cairo::ImageSurface>,
    cur: Option<(Scene, Painter)>,
    handler_ms: Vec<f64>,
    intervals_ms: Vec<f64>,
    last_frame_us: Option<i64>,
    frame: usize,
    total: usize,
}

fn report(name: &str, v: &mut [f64]) {
    if v.is_empty() {
        println!("{name}: no samples");
        return;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    println!(
        "{name}: n={} p50 {:.2} p95 {:.2} p99 {:.2} max {:.2} ms",
        v.len(),
        percentile(v, 0.5),
        percentile(v, 0.95),
        percentile(v, 0.99),
        v[v.len() - 1]
    );
}

fn main() {
    let frames: usize = std::env::var("PROBE_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(2 * FRAMES);
    let (w, h): (i32, i32) = std::env::var("PROBE_SIZE")
        .ok()
        .and_then(|v| v.split_once('x').map(|(a, b)| (a.parse().ok(), b.parse().ok())))
        .and_then(|(a, b)| Some((a?, b?)))
        .unwrap_or((1600, 900));
    let mode = match std::env::var("PROBE_MODE").as_deref() {
        Ok("blit") => Mode::Blit,
        Ok("cairo") => Mode::Cairo,
        Ok("current") => Mode::Current,
        _ => Mode::Gl,
    };
    gtk::init().expect("gtk init (needs a display: Xvfb or the private compositor)");
    println!(
        "GDK backend: {}; mode {}",
        gdk::Display::default().map_or("none".into(), |d| d.type_().name().to_string()),
        match mode {
            Mode::Gl => "gl",
            Mode::Blit => "blit",
            Mode::Cairo => "cairo",
            Mode::Current => "current",
        }
    );
    let fx = fixture::by_name("app");
    let state = Rc::new(RefCell::new(State {
        cur: (mode == Mode::Current).then(|| (build_scene(&fx), Painter::new())),
        fx,
        cam: pan_cam(0, w as f64, h as f64),
        labels: Vec::new(),
        with_labels: std::env::var_os("PROBE_LABELS").is_some(),
        finish: std::env::var_os("PROBE_FINISH").is_some(),
        gl: None,
        plain: None,
        blit: None,
        handler_ms: Vec::new(),
        intervals_ms: Vec::new(),
        last_frame_us: None,
        frame: 0,
        total: frames,
    }));
    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_default_size(w, h);
    let widget: gtk::Widget = if mode == Mode::Gl {
        let area = gtk::GLArea::new();
        area.set_required_version(3, 3);
        area.set_has_depth_buffer(false);
        area.set_auto_render(false);
        let lib = Arc::new(unsafe { libloading::Library::new("libepoxy.so.0") }.expect("libepoxy.so.0"));
        area.connect_realize(|a| {
            a.make_current();
            match a.error() {
                Some(e) => println!("GLArea realize error: {e}"),
                None => {
                    let ctx = a.context().expect("context");
                    let (maj, min) = ctx.version();
                    println!("GLArea context: GL {maj}.{min}, legacy {}, uses_es {}", ctx.is_legacy(), ctx.uses_es());
                }
            }
        });
        let state = state.clone();
        area.connect_render(move |a, _ctx| {
            let t0 = Instant::now();
            let mut st = state.borrow_mut();
            let scale = a.scale_factor();
            let (dw, dh) = (a.allocated_width() * scale, a.allocated_height() * scale);
            if st.gl.is_none() {
                let gl = unsafe {
                    glow::Context::from_loader_function(|name| {
                        let sym = unsafe { lib.get::<*const *const c_void>(format!("epoxy_{name}\0").as_bytes()) };
                        sym.map_or(std::ptr::null(), |s| unsafe { **s })
                    })
                };
                unsafe {
                    println!(
                        "GL_RENDERER `{}`; GL_VERSION `{}`; GLSL `{}`; device scale {scale}; framebuffer {dw}x{dh}",
                        gl.get_parameter_string(glow::RENDERER),
                        gl.get_parameter_string(glow::VERSION),
                        gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION)
                    );
                }
                let scene = GlScene::new(Arc::new(gl), &st.fx, scale as u32);
                st.gl = Some(scene);
            }
            let State { fx, gl, cam, labels, finish, .. } = &mut *st;
            let scene = gl.as_mut().expect("scene");
            scene.draw(fx, cam, labels, dw, dh);
            if *finish {
                scene.finish();
            }
            st.handler_ms.push(t0.elapsed().as_secs_f64() * 1e3);
            glib::Propagation::Stop
        });
        area.upcast()
    } else {
        let da = gtk::DrawingArea::new();
        let state = state.clone();
        da.connect_draw(move |a, cr| {
            let t0 = Instant::now();
            let mut st = state.borrow_mut();
            let (lw, lh) = (a.allocated_width() as u32, a.allocated_height() as u32);
            let scale = a.scale_factor() as u32;
            let State { fx, cam, labels, plain, blit, cur, .. } = &mut *st;
            match mode {
                Mode::Blit => {
                    let s = blit.get_or_insert_with(|| {
                        let mut p = Plain::new(lw, lh, scale, cairo::Antialias::Default).expect("surface");
                        p.frame(fx, cam, &[]);
                        p.surface
                    });
                    cr.save().ok();
                    cr.scale(1.0 / scale as f64, 1.0 / scale as f64);
                    cr.set_source_surface(&*s, 0.0, 0.0).ok();
                    cr.paint().ok();
                    cr.restore().ok();
                }
                Mode::Cairo => {
                    let p = plain.get_or_insert_with(|| Plain::new(lw, lh, scale, cairo::Antialias::Default).expect("surface"));
                    p.frame(fx, cam, labels);
                    cr.save().ok();
                    cr.scale(1.0 / scale as f64, 1.0 / scale as f64);
                    cr.set_source_surface(&p.surface, 0.0, 0.0).ok();
                    cr.paint().ok();
                    cr.restore().ok();
                }
                Mode::Current => {
                    if let Some((scene, painter)) = cur {
                        scene.set_size(lw as f64, lh as f64);
                        scene.cam.cam = Camera { pan_x: cam.x, pan_y: cam.y, zoom: cam.zoom };
                        scene.cam.target_zoom = cam.zoom;
                        painter.motion = std::env::var_os("PROBE_MOTION").is_some();
                        painter.paint(cr, scene, lw as f64, lh as f64, PaintOptions { hide_labels: false, scale: scale as f64 });
                    }
                }
                Mode::Gl => {}
            }
            st.handler_ms.push(t0.elapsed().as_secs_f64() * 1e3);
            glib::Propagation::Stop
        });
        da.upcast()
    };
    win.add(&widget);
    {
        let state = state.clone();
        widget.add_tick_callback(move |a, clock| {
            let mut st = state.borrow_mut();
            let us = clock.frame_time();
            if let Some(prev) = st.last_frame_us.replace(us) {
                st.intervals_ms.push((us - prev) as f64 / 1e3);
            }
            let (lw, lh) = (a.allocated_width() as f64, a.allocated_height() as f64);
            let k = st.frame;
            st.cam = if k < FRAMES { pan_fit_cam(&st.fx, k, lw, lh) } else { zoom_cam((k - FRAMES).min(FRAMES - 1), lw, lh) };
            if st.with_labels {
                let cam = st.cam;
                let mut labels = std::mem::take(&mut st.labels);
                pick_labels(&st.fx, &cam, lw, lh, &mut labels);
                st.labels = labels;
            }
            st.frame += 1;
            if st.frame >= st.total {
                let mut handler = std::mem::take(&mut st.handler_ms);
                let mut intervals = std::mem::take(&mut st.intervals_ms);
                println!("frames drawn: {}; first (cold) handler call {:.2} ms", handler.len(), handler.first().copied().unwrap_or(0.0));
                if handler.len() > 1 {
                    handler.remove(0);
                }
                report("draw/render handler (after the cold frame)", &mut handler);
                report("frame-clock interval", &mut intervals);
                gtk::main_quit();
                return glib::ControlFlow::Break;
            }
            if let Some(gla) = a.downcast_ref::<gtk::GLArea>() {
                gla.queue_render();
            } else {
                a.queue_draw();
            }
            glib::ControlFlow::Continue
        });
    }
    win.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    win.show_all();
    gtk::main();
}
