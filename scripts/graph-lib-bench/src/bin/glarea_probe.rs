//! Can a GTK 3.24 `GLArea` be created and render the instanced scene? Loads
//! GL entry points from libepoxy (the library GTK itself uses), draws the
//! fixture while a scripted pan + zoom-out drives `queue_render`, and prints
//! real frame intervals (frame clock) plus the time spent in the render
//! handler. Run under Xvfb (`GDK_BACKEND=x11`) or the private Wayland
//! compositor (`GDK_BACKEND=wayland`). Env: `PROBE_FRAMES`, `PROBE_SIZE=WxH`,
//! `PROBE_FINISH=1` (glFinish inside the handler), `PROBE_LABELS=1`.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use glow::HasContext;
use graph_lib_bench::fixture::{self, Fixture};
use graph_lib_bench::glscene::GlScene;
use graph_lib_bench::harness::{pan_cam, percentile, pick_labels, zoom_cam, Cam, LabelDraw, FRAMES};
use gtk::prelude::*;

struct State {
    fx: Fixture,
    scene: Option<GlScene>,
    cam: Cam,
    labels: Vec<LabelDraw>,
    with_labels: bool,
    finish: bool,
    handler_ms: Vec<f64>,
    intervals_ms: Vec<f64>,
    last_frame_us: Option<i64>,
    frame: usize,
    total: usize,
}

fn percentiles(name: &str, v: &mut [f64]) {
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
    gtk::init().expect("gtk init (needs a display: Xvfb or the private compositor)");
    println!(
        "GDK backend: {}",
        gdk::Display::default().map_or("none".into(), |d| d.type_().name().to_string())
    );
    let state = Rc::new(RefCell::new(State {
        fx: fixture::by_name("app"),
        scene: None,
        cam: pan_cam(0, w as f64, h as f64),
        labels: Vec::new(),
        with_labels: std::env::var_os("PROBE_LABELS").is_some(),
        finish: std::env::var_os("PROBE_FINISH").is_some(),
        handler_ms: Vec::new(),
        intervals_ms: Vec::new(),
        last_frame_us: None,
        frame: 0,
        total: frames,
    }));
    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_default_size(w, h);
    let area = gtk::GLArea::new();
    area.set_required_version(3, 3);
    area.set_has_depth_buffer(false);
    area.set_auto_render(false);
    win.add(&area);
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
    {
        let state = state.clone();
        let lib = lib.clone();
        area.connect_render(move |a, _ctx| {
            let t0 = Instant::now();
            let mut st = state.borrow_mut();
            let scale = a.scale_factor();
            let (dw, dh) = (a.allocated_width() * scale, a.allocated_height() * scale);
            if st.scene.is_none() {
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
                let gl = Arc::new(gl);
                st.scene = Some(GlScene::new(gl, &st.fx, scale as u32));
            }
            let State { fx, scene, cam, labels, finish, .. } = &mut *st;
            let scene = scene.as_mut().expect("scene");
            scene.draw(fx, cam, labels, dw, dh);
            if *finish {
                scene.finish();
            }
            st.handler_ms.push(t0.elapsed().as_secs_f64() * 1e3);
            gtk::Inhibit(true)
        });
    }
    {
        let state = state.clone();
        area.add_tick_callback(move |a, clock| {
            let mut st = state.borrow_mut();
            let us = clock.frame_time();
            if let Some(prev) = st.last_frame_us.replace(us) {
                st.intervals_ms.push((us - prev) as f64 / 1e3);
            }
            let (lw, lh) = (a.allocated_width() as f64, a.allocated_height() as f64);
            let k = st.frame;
            st.cam = if k < FRAMES { pan_cam(k, lw, lh) } else { zoom_cam((k - FRAMES).min(FRAMES - 1), lw, lh) };
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
                if handler.len() > 1 {
                    handler.remove(0); // cold frame (shader/atlas setup) reported separately
                }
                println!("frames rendered: {}", handler.len() + 1);
                percentiles("render handler", &mut handler);
                percentiles("frame-clock interval", &mut intervals);
                gtk::main_quit();
                return glib::ControlFlow::Break;
            }
            a.queue_render();
            glib::ControlFlow::Continue
        });
    }
    win.connect_delete_event(|_, _| {
        gtk::main_quit();
        gtk::Inhibit(false)
    });
    win.show_all();
    gtk::main();
}
