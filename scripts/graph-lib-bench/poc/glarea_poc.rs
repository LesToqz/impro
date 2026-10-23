//! Proof of concept: the fixture in a GTK 3.24 `GLArea` drawn by the
//! instanced GL scene, with mouse pan (drag) and zoom (wheel about the
//! pointer). Runs only against a private display — Xvfb (`GDK_BACKEND=x11`)
//! or the private headless Wayland compositor — never the user's session.
//!
//! `glarea_poc`         interactive; frame intervals are printed on close
//! `glarea_poc --auto`  scripted pan + zoom-out, prints the intervals, quits
//! `--size WxH` window size (default 1600x900), `--fixture app|packed|dense`
//!
//! Frames are requested only while the camera moves (no idle redraws).

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use glow::HasContext;
use graph_lib_bench::fixture::{self, Fixture};
use graph_lib_bench::glscene::GlScene;
use graph_lib_bench::harness::{fit_cam, pan_fit_cam, percentile, zoom_cam, Cam, FRAMES};
use gtk::prelude::*;

struct State {
    fx: Fixture,
    gl: Option<GlScene>,
    cam: Cam,
    drag: Option<(f64, f64)>,
    auto: bool,
    frame: usize,
    last_us: Option<i64>,
    intervals: Vec<f64>,
    handler: Vec<f64>,
}

fn report(name: &str, v: &mut [f64]) {
    if v.len() < 2 {
        println!("{name}: not enough samples ({})", v.len());
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
    let args: Vec<String> = std::env::args().collect();
    let auto = args.iter().any(|a| a == "--auto");
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let (w, h): (i32, i32) = value("--size")
        .and_then(|v| v.split_once('x').map(|(a, b)| (a.parse().ok(), b.parse().ok())))
        .and_then(|(a, b)| Some((a?, b?)))
        .unwrap_or((1600, 900));
    gtk::init().expect("gtk init (needs a private display)");
    if std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_none() {
        eprintln!("no display: run under xvfb-run or the private compositor");
        std::process::exit(2);
    }
    let fx = fixture::by_name(&value("--fixture").unwrap_or_else(|| "app".into()));
    let cam = fit_cam(&fx, w as f64, h as f64);
    let state = Rc::new(RefCell::new(State {
        fx,
        gl: None,
        cam,
        drag: None,
        auto,
        frame: 0,
        last_us: None,
        intervals: Vec::new(),
        handler: Vec::new(),
    }));
    let win = gtk::Window::new(gtk::WindowType::Toplevel);
    win.set_title("impro graph GL proof of concept");
    win.set_default_size(w, h);
    let area = gtk::GLArea::new();
    area.set_required_version(3, 3);
    area.set_has_depth_buffer(false);
    area.set_auto_render(false);
    area.add_events(
        gdk::EventMask::BUTTON_PRESS_MASK
            | gdk::EventMask::BUTTON_RELEASE_MASK
            | gdk::EventMask::POINTER_MOTION_MASK
            | gdk::EventMask::SCROLL_MASK,
    );
    let lib = Arc::new(unsafe { libloading::Library::new("libepoxy.so.0") }.expect("libepoxy.so.0"));
    area.connect_realize(|a| {
        a.make_current();
        if let Some(e) = a.error() {
            eprintln!("GLArea cannot create a GL 3.3 context: {e}");
        }
    });
    {
        let state = state.clone();
        area.connect_render(move |a, _| {
            let t0 = Instant::now();
            let mut st = state.borrow_mut();
            let scale = a.scale_factor();
            let (dw, dh) = (a.allocated_width() * scale, a.allocated_height() * scale);
            if st.gl.is_none() {
                let gl = unsafe {
                    glow::Context::from_loader_function(|name| {
                        let sym = lib.get::<*const *const c_void>(format!("epoxy_{name}\0").as_bytes());
                        sym.map_or(std::ptr::null(), |s| **s)
                    })
                };
                eprintln!("GL_RENDERER `{}`", unsafe { gl.get_parameter_string(glow::RENDERER) });
                let scene = GlScene::new(Arc::new(gl), &st.fx, scale as u32);
                st.gl = Some(scene);
            }
            let State { fx, gl, cam, .. } = &mut *st;
            gl.as_mut().expect("scene").draw(fx, cam, &[], dw, dh);
            st.handler.push(t0.elapsed().as_secs_f64() * 1e3);
            glib::Propagation::Stop
        });
    }
    {
        let state = state.clone();
        area.connect_button_press_event(move |_, e| {
            state.borrow_mut().drag = Some(e.position());
            glib::Propagation::Stop
        });
    }
    {
        let state = state.clone();
        area.connect_button_release_event(move |_, _| {
            state.borrow_mut().drag = None;
            glib::Propagation::Stop
        });
    }
    {
        let state = state.clone();
        area.connect_motion_notify_event(move |a, e| {
            let mut st = state.borrow_mut();
            if let Some((px, py)) = st.drag {
                let (x, y) = e.position();
                st.cam.x += x - px;
                st.cam.y += y - py;
                st.drag = Some((x, y));
                drop(st);
                a.queue_render();
            }
            glib::Propagation::Stop
        });
    }
    {
        let state = state.clone();
        area.connect_scroll_event(move |a, e| {
            let mut st = state.borrow_mut();
            let factor = match e.direction() {
                gdk::ScrollDirection::Up => 1.1,
                gdk::ScrollDirection::Down => 1.0 / 1.1,
                _ => 1.0 + (-e.delta().1) * 0.1,
            };
            let (px, py) = e.position();
            let zoom = (st.cam.zoom * factor).clamp(0.04, 6.0);
            let k = zoom / st.cam.zoom;
            // keep the world point under the pointer fixed
            st.cam.x = px - (px - st.cam.x) * k;
            st.cam.y = py - (py - st.cam.y) * k;
            st.cam.zoom = zoom;
            drop(st);
            a.queue_render();
            glib::Propagation::Stop
        });
    }
    {
        let state = state.clone();
        area.add_tick_callback(move |a, clock| {
            let mut st = state.borrow_mut();
            let us = clock.frame_time();
            if let Some(prev) = st.last_us.replace(us) {
                st.intervals.push((us - prev) as f64 / 1e3);
            }
            if !st.auto {
                // idle: stop measuring gaps between interactions
                if st.drag.is_none() {
                    st.last_us = None;
                }
                return glib::ControlFlow::Continue;
            }
            let (lw, lh) = (a.allocated_width() as f64, a.allocated_height() as f64);
            let k = st.frame;
            st.cam = if k < FRAMES { pan_fit_cam(&st.fx, k, lw, lh) } else { zoom_cam((k - FRAMES).min(FRAMES - 1), lw, lh) };
            st.frame += 1;
            if st.frame > 2 * FRAMES + 2 {
                gtk::main_quit();
                return glib::ControlFlow::Break;
            }
            a.queue_render();
            glib::ControlFlow::Continue
        });
    }
    win.add(&area);
    win.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    win.show_all();
    gtk::main();
    let mut st = state.borrow_mut();
    let mut handler = std::mem::take(&mut st.handler);
    let mut intervals = std::mem::take(&mut st.intervals);
    if handler.len() > 1 {
        handler.remove(0);
    }
    report("render handler (CPU submit)", &mut handler);
    report("frame-clock interval", &mut intervals);
}
