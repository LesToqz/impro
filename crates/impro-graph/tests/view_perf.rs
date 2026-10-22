//! Timings (run with `--ignored --nocapture`): draw cost at 300 and 3 000
//! nodes, tick plus draw, open to first frame, idle CPU.

mod gtk_common;

use gtk::prelude::*;
use std::time::{Duration, Instant};

use gtk_common::*;
use impro_graph::geometry::Camera;
use impro_graph::view::draw::{PaintOptions, Painter};
use impro_graph::view::scene::{Built, Scene};
use impro_graph::view::testing as t;
use impro_graph::Graph;
use serde_json::json;

fn cpu_seconds() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let fields: Vec<&str> = stat
        .rsplit(')')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    let ticks = |i: usize| {
        fields
            .get(i)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (ticks(11) + ticks(12)) / 100.0
}

fn pairs(n: usize, edges: usize) -> Vec<(u32, u32)> {
    let mut rng = impro_graph::layout::Rng::new(7);
    let mut out = Vec::new();
    let mut ends = vec![0u32];
    for i in 1..n as u32 {
        let target = ends[(rng.unit() * ends.len() as f32) as usize % ends.len()];
        out.push((i, target));
        ends.push(target);
        ends.push(i);
    }
    while out.len() < edges {
        let a = (rng.unit() * n as f32) as u32 % n as u32;
        let b = ends[(rng.unit() * ends.len() as f32) as usize % ends.len()];
        if a != b {
            out.push((a, b));
        }
    }
    out
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f64 * p) as usize]
}

#[test]
#[ignore = "prints timings"]
fn draw_and_tick_timings() {
    run(|| {
        for (n, e) in [(300, 800), (3000, 8000)] {
            let mut scene = Scene::new();
            scene.set_size(1280.0, 800.0);
            scene.apply(Built::global(Graph::from_pairs(n, &pairs(n, e))));
            while scene.advance(1.0 / 60.0) {}
            let mut painter = Painter::new();
            let bounds = scene.layout.bounds().expect("bounds");
            let mut fit = Camera::default();
            fit.fit(bounds, 1280.0, 800.0, 40.0);
            scene.cam.cam = fit;
            scene.cam.target_zoom = fit.zoom;
            for _ in 0..3 {
                painter.render(&scene, 1280.0, 800.0, PaintOptions::default());
            }
            let mut draw = Vec::new();
            for _ in 0..60 {
                let t0 = Instant::now();
                painter.render(&scene, 1280.0, 800.0, PaintOptions::default());
                draw.push(t0.elapsed().as_secs_f64() * 1e3);
            }
            // tick + draw while the layout is hot
            scene.layout.restart();
            let mut frame = Vec::new();
            for _ in 0..120 {
                let t0 = Instant::now();
                scene.advance(1.0 / 60.0);
                painter.render(&scene, 1280.0, 800.0, PaintOptions::default());
                frame.push(t0.elapsed().as_secs_f64() * 1e3);
            }
            println!(
                "[{n} nodes / {e} edges] draw p50 {:.2} ms p95 {:.2} ms | tick+draw p50 {:.2} ms p95 {:.2} ms",
                percentile(&mut draw.clone(), 0.5),
                percentile(&mut draw, 0.95),
                percentile(&mut frame.clone(), 0.5),
                percentile(&mut frame, 0.95)
            );
        }
    });
}

#[test]
#[ignore = "prints timings"]
fn idle_cpu_and_open_to_first_frame() {
    run(|| {
        let project = tempfile::tempdir().expect("dir");
        for i in 0..600 {
            write(
                project.path(),
                &format!("n/{i}.md"),
                &format!("[[{}]] [[{}]]", (i * 7) % 600, (i * 13 + 5) % 600),
            );
        }
        impro_ui::apply_style(None).expect("style");
        let first = std::rc::Rc::new(std::cell::Cell::new(None));
        let populated = std::rc::Rc::new(std::cell::Cell::new(None));
        let t0 = Instant::now();
        let view = impro_graph::GraphView::with_options(
            project.path(),
            impro_graph::ViewOptions {
                store: std::sync::Arc::new(impro_graph::view::MemoryStore::new(json!({}))),
                scan: true,
            },
        );
        let (frame, data) = (first.clone(), populated.clone());
        let weak = view.downgrade();
        view.canvas().connect_draw(move |_, _| {
            frame.set(frame.get().or_else(|| Some(t0.elapsed())));
            if weak.upgrade().is_some_and(|view| view.summary().nodes > 0) {
                data.set(data.get().or_else(|| Some(t0.elapsed())));
            }
            glib::Propagation::Proceed
        });
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(900, 600);
        window.add(&*view);
        window.show_all();
        assert!(t::wait_for(10_000, || populated.get().is_some()));
        println!(
            "open-first-frame-ms={:.3} open-first-populated-frame-ms={:.3}",
            first.get().unwrap().as_secs_f64() * 1e3,
            populated.get().unwrap().as_secs_f64() * 1e3
        );
        t::settle_now(&view);
        assert!(t::settle(&view, 15_000));
        t::pump(3000);
        let main_loop = glib::MainLoop::new(None, false);
        let stop = main_loop.clone();
        glib::timeout_add_local_once(Duration::from_secs(10), move || stop.quit());
        let (cpu, wall) = (cpu_seconds(), Instant::now());
        main_loop.run();
        let elapsed = wall.elapsed().as_secs_f64();
        let used = cpu_seconds() - cpu;
        println!(
            "idle-cpu-s={used:.3} wall-s={elapsed:.3} idle-percent={:.3}",
            used / elapsed * 100.0
        );
        unsafe {
            window.destroy();
        }
    });
}
