//! Draw and tick timings of the graph view model on synthetic graphs.
//!
//! `cargo run --release -p impro-graph --example graph_bench` prints the
//! median and 95th-percentile cost of a frame for 300 and 3 000 nodes.

use std::time::Instant;

use impro_graph::geometry::Camera;
use impro_graph::layout::Rng;
use impro_graph::view::draw::{PaintOptions, Painter};
use impro_graph::view::scene::{Built, Scene};
use impro_graph::Graph;

const W: i32 = 1280;
const H: i32 = 800;

/// Preferential-attachment pairs: a few hubs, chains, some random cross links.
fn pairs(n: usize, edges: usize) -> Vec<(u32, u32)> {
    let mut rng = Rng::new(7);
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut ends: Vec<u32> = vec![0];
    for i in 1..n as u32 {
        let t = ends[(rng.unit() * ends.len() as f32) as usize % ends.len()];
        out.push((i, t));
        ends.push(t);
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

fn bench(n: usize, edges: usize) {
    let mut scene = Scene::new();
    scene.set_size(W as f64, H as f64);
    let t = Instant::now();
    scene.apply(Built::global(Graph::from_pairs(n, &pairs(n, edges))));
    println!(
        "[{n} nodes / {edges} edges] apply {:.2} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
    // settle the layout, timing the ticks
    let mut ticks: Vec<f64> = Vec::new();
    while !scene.layout.settled() && ticks.len() < 400 {
        let t = Instant::now();
        scene.layout.step();
        ticks.push(t.elapsed().as_secs_f64() * 1e3);
    }
    println!(
        "  layout tick p50 {:.2} ms  p95 {:.2} ms ({} ticks)",
        percentile(&mut ticks.clone(), 0.5),
        percentile(&mut ticks, 0.95),
        scene.layout.ticks()
    );
    while scene.advance(1.0 / 60.0) {}
    let mut painter = Painter::new();
    if let Some(t) = std::env::args().nth(1).and_then(|a| a.parse().ok()) {
        painter.threads = t;
    }
    let bounds = scene.layout.bounds().expect("bounds");
    let mut cases: Vec<(&str, Camera)> = Vec::new();
    let mut fit = Camera::default();
    fit.fit(bounds, W as f64, H as f64, 40.0);
    cases.push(("fit", fit));
    cases.push((
        "zoom 1",
        Camera {
            pan_x: W as f64 / 2.0,
            pan_y: H as f64 / 2.0,
            zoom: 1.0,
        },
    ));
    let mut near = fit;
    near.zoom = (fit.zoom * 3.0).min(6.0);
    near.pan_x = W as f64 / 2.0;
    near.pan_y = H as f64 / 2.0;
    cases.push(("zoomed in", near));
    for (name, cam) in cases {
        scene.cam.cam = cam;
        scene.cam.target_zoom = cam.zoom;
        for hover in [None, Some(0)] {
            scene.hover = hover;
            while scene.advance(1.0 / 60.0) {}
            // warm label bitmaps
            for _ in 0..3 {
                painter.render(&scene, W as f64, H as f64, PaintOptions::default());
            }
            let mut ms: Vec<f64> = Vec::new();
            for _ in 0..40 {
                let t = Instant::now();
                painter.render(&scene, W as f64, H as f64, PaintOptions::default());
                ms.push(t.elapsed().as_secs_f64() * 1e3);
            }
            if n == 3000 && hover.is_none() {
                if let Some(dir) = std::env::var_os("IMPRO_GRAPH_BENCH_IMAGES") {
                    let dir = std::path::PathBuf::from(dir);
                    std::fs::create_dir_all(&dir).expect("image directory");
                    let mut file =
                        std::fs::File::create(dir.join(format!("{}.png", name.replace(' ', "-"))))
                            .expect("image file");
                    painter
                        .render(&scene, W as f64, H as f64, PaintOptions::default())
                        .expect("surface")
                        .write_to_png(&mut file)
                        .expect("PNG");
                }
            }
            let s = painter.stats;
            if n == 3000 && name == "zoomed in" {
                // Geometry changes every frame: a pan of a few pixels, drawn
                // at full quality and at the moving level of detail.
                for motion in [false, true] {
                    painter.motion = motion;
                    let mut changed = Vec::new();
                    let mut phases: [Vec<f64>; 5] = Default::default();
                    for frame in 0..60 {
                        scene.cam.cam.pan_x = cam.pan_x + (frame % 20) as f64 * 3.7;
                        let start = Instant::now();
                        painter.render(&scene, W as f64, H as f64, PaintOptions::default());
                        changed.push(start.elapsed().as_secs_f64() * 1e3);
                        for (slot, v) in phases.iter_mut().zip(painter.stats.phase_ms) {
                            slot.push(v);
                        }
                    }
                    let med = phases.map(|mut v| percentile(&mut v, 0.5));
                    println!(
                        "  changing geometry hover={} motion={motion} p50 {:.2} ms p95 {:.2} ms phases bg/lines/arrows/nodes/labels {:.1?} lines prep/raster/composite {:.1?} (arrows {} labels {})",
                        hover.is_some(),
                        percentile(&mut changed.clone(), 0.5),
                        percentile(&mut changed, 0.95),
                        med,
                        painter.stats.line_ms,
                        painter.stats.arrows,
                        painter.stats.labels
                    );
                    if motion && hover.is_none() {
                        if let Some(dir) = std::env::var_os("IMPRO_GRAPH_BENCH_IMAGES") {
                            let dir = std::path::PathBuf::from(dir);
                            let mut file = std::fs::File::create(dir.join("zoomed-in-moving.png"))
                                .expect("image file");
                            painter
                                .render(&scene, W as f64, H as f64, PaintOptions::default())
                                .expect("surface")
                                .write_to_png(&mut file)
                                .expect("PNG");
                        }
                    }
                }
                painter.motion = false;
                scene.cam.cam = cam;
            }
            println!(
                "  {name:<9} hover={:<5} draw p50 {:.2} ms  p95 {:.2} ms  (nodes {} lines {} [{:.0} px] arrows {} labels {}) phases bg/lines/arrows/nodes/labels {:.1?} lines prep/raster/composite {:.1?}",
                hover.is_some(),
                percentile(&mut ms.clone(), 0.5),
                percentile(&mut ms, 0.95),
                s.nodes, s.lines, s.line_px, s.arrows, s.labels, s.phase_ms, s.line_ms
            );
        }
    }
}

/// Frames while a hover change eases (geometry unchanged, appearance fades),
/// at three cameras: hover on, then off again.
fn bench_hover(n: usize, edges: usize) {
    let mut scene = Scene::new();
    scene.set_size(W as f64, H as f64);
    scene.apply(Built::global(Graph::from_pairs(n, &pairs(n, edges))));
    while scene.advance(1.0 / 60.0) {}
    let bounds = scene.layout.bounds().expect("bounds");
    let mut fit = Camera::default();
    fit.fit(bounds, W as f64, H as f64, 40.0);
    let mut near = fit;
    near.zoom = (fit.zoom * 3.0).min(6.0);
    near.pan_x = W as f64 / 2.0;
    near.pan_y = H as f64 / 2.0;
    for (name, cam) in [("fit", fit), ("zoomed in", near)] {
        scene.cam.cam = cam;
        scene.cam.target_zoom = cam.zoom;
        scene.hover = None;
        while scene.advance(1.0 / 60.0) {}
        let mut painter = Painter::new();
        for _ in 0..4 {
            painter.render(&scene, W as f64, H as f64, PaintOptions::default());
        }
        let mut ms = Vec::new();
        for target in [Some(0), None, Some(5), None] {
            scene.set_hover(target);
            loop {
                let busy = scene.advance(1.0 / 60.0);
                painter.motion = scene.moving();
                let t = Instant::now();
                painter.render(&scene, W as f64, H as f64, PaintOptions::default());
                ms.push(t.elapsed().as_secs_f64() * 1e3);
                if std::env::var_os("IMPRO_GRAPH_BENCH_TRACE").is_some() {
                    eprintln!(
                        "    {:.1} ms lines {} arrows {}",
                        ms[ms.len() - 1],
                        painter.stats.lines,
                        painter.stats.arrows
                    );
                }
                if !busy || ms.len() > 400 {
                    break;
                }
            }
        }
        println!(
            "  hover easing, {name:<9}: {} frames draw p50 {:.2} ms p95 {:.2} ms max {:.2} ms",
            ms.len(),
            percentile(&mut ms.clone(), 0.5),
            percentile(&mut ms.clone(), 0.95),
            percentile(&mut ms, 1.0)
        );
    }
}

/// Tick plus draw while the layout simulation runs (zoomed in on the middle).
fn bench_hot(n: usize, edges: usize) {
    let mut scene = Scene::new();
    scene.set_size(W as f64, H as f64);
    scene.apply(Built::global(Graph::from_pairs(n, &pairs(n, edges))));
    while scene.advance(1.0 / 60.0) {}
    let bounds = scene.layout.bounds().expect("bounds");
    let mut fit = Camera::default();
    fit.fit(bounds, W as f64, H as f64, 40.0);
    for (label, zoom_factor) in [("fit", 1.0), ("zoomed in", 3.0)] {
        for motion in [false, true] {
            let mut cam = fit;
            cam.zoom = (fit.zoom * zoom_factor).min(6.0);
            cam.pan_x = W as f64 / 2.0;
            cam.pan_y = H as f64 / 2.0;
            scene.cam.cam = cam;
            scene.cam.target_zoom = cam.zoom;
            scene.layout.restart();
            let mut painter = Painter::new();
            painter.motion = motion;
            for _ in 0..3 {
                painter.render(&scene, W as f64, H as f64, PaintOptions::default());
            }
            let (mut total, mut ticks, mut draws) = (Vec::new(), Vec::new(), Vec::new());
            for _ in 0..90 {
                let t0 = Instant::now();
                scene.advance(1.0 / 60.0);
                let t1 = Instant::now();
                painter.render(&scene, W as f64, H as f64, PaintOptions::default());
                let t2 = Instant::now();
                ticks.push((t1 - t0).as_secs_f64() * 1e3);
                draws.push((t2 - t1).as_secs_f64() * 1e3);
                total.push((t2 - t0).as_secs_f64() * 1e3);
            }
            println!(
                "  layout running, {label:<9} motion={motion:<5} tick p50 {:.2} draw p50 {:.2} p95 {:.2} | tick+draw p50 {:.2} p95 {:.2} ms",
                percentile(&mut ticks, 0.5),
                percentile(&mut draws.clone(), 0.5),
                percentile(&mut draws, 0.95),
                percentile(&mut total.clone(), 0.5),
                percentile(&mut total, 0.95)
            );
        }
    }
}

/// Frame timings of the real widget: `graph_bench frames [WIDTHxHEIGHT ...]`
/// (device pixels; run under Xvfb, `GDK_SCALE` picks the device scale).
mod frames {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use gtk::prelude::*;
    use impro_graph::view::testing::{self as t, BigProject};
    use impro_graph::view::{MemoryStore, ViewOptions};
    use impro_graph::GraphView;

    use super::percentile;

    #[derive(Default)]
    struct Log {
        draw: Vec<f64>,
        interval: Vec<f64>,
        last: Option<Instant>,
    }

    fn summary(v: &[f64]) -> String {
        if v.is_empty() {
            return "      -".into();
        }
        let mut s = v.to_vec();
        format!(
            "p50 {:>6.2}  p95 {:>6.2}  p99 {:>6.2}  max {:>6.2}",
            percentile(&mut s, 0.5),
            percentile(&mut s, 0.95),
            percentile(&mut s, 0.99),
            percentile(&mut s, 1.0)
        )
    }

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

    fn frame(view: &GraphView) {
        t::wait_frames(view, 1, 5_000);
    }

    struct Rig {
        view: GraphView,
        window: gtk::Window,
        log: Rc<RefCell<Log>>,
        t0: Instant,
        _project: tempfile::TempDir,
        spots: Vec<(f64, f64)>,
    }

    impl Rig {
        fn new(logical: (i32, i32)) -> Rig {
            let project = tempfile::tempdir().expect("tempdir");
            BigProject::default().write(project.path());
            let view = GraphView::with_options(
                project.path(),
                ViewOptions {
                    store: Arc::new(MemoryStore::new(serde_json::json!({}))),
                    scan: true,
                },
            );
            let window = gtk::Window::new(gtk::WindowType::Toplevel);
            window.set_default_size(logical.0, logical.1);
            window.add(&*view);
            window.show_all();
            window.move_(0, 0);
            assert!(t::wait_for(60_000, || view.summary().nodes > 0), "no graph");
            t::settle_now(&view);
            assert!(t::settle(&view, 60_000), "never settled");
            let log = Rc::new(RefCell::new(Log::default()));
            let sink = log.clone();
            t::set_probe(&view, move |shot| {
                let mut l = sink.borrow_mut();
                let now = Instant::now();
                if let Some(prev) = l.last {
                    l.interval.push((now - prev).as_secs_f64() * 1e3);
                }
                l.last = Some(now);
                l.draw.push(shot.last_draw_ms());
            });
            let mut rig = Rig {
                view,
                window,
                log,
                t0: Instant::now(),
                _project: project,
                spots: Vec::new(),
            };
            rig.find_spots();
            rig
        }

        /// Canvas points at least 8 px from any marker.
        fn find_spots(&mut self) {
            let (w, h) = t::info(&self.view).size;
            let nodes: Vec<(f64, f64, f64)> = t::node_ids(&self.view)
                .iter()
                .filter_map(|id| t::node_screen(&self.view, id))
                .collect();
            let mut y = 40.0;
            while y < h - 40.0 {
                let mut x = 40.0;
                while x < w - 40.0 {
                    if nodes
                        .iter()
                        .all(|(nx, ny, r)| (nx - x).hypot(ny - y) > r + 8.0)
                    {
                        self.spots.push((x, y));
                    }
                    x += 23.0;
                }
                y += 23.0;
            }
            if self.spots.is_empty() {
                self.spots.push((6.0, 6.0));
            }
        }

        fn ms(&self) -> u32 {
            1_000 + self.t0.elapsed().as_millis() as u32
        }

        fn reset_log(&self) {
            *self.log.borrow_mut() = Log::default();
        }

        fn report(&self, name: &str) {
            let l = self.log.borrow();
            println!(
                "    {name:<22} frames {:>4} | draw ms {} | interval ms {}",
                l.draw.len(),
                summary(&l.draw),
                summary(&l.interval)
            );
        }

        fn settle(&self) {
            t::settle(&self.view, 30_000);
            t::pump(100);
        }

        fn held_pan(&self, frames: u32) {
            self.settle();
            let none = gdk::ModifierType::empty();
            let (mut x, mut y) = self.spots[self.spots.len() / 2];
            t::motion_at(&self.view, x, y, self.ms());
            frame(&self.view);
            self.reset_log();
            t::press_at(&self.view, x, y, 1, none, self.ms());
            for k in 0..frames {
                frame(&self.view);
                let speed = 3.0 + 20.0 * (1.0 + (k as f64 / 15.0).sin());
                x += speed * 0.8;
                y += speed * 0.4;
                if x > 1_000.0 || y > 600.0 {
                    x -= 800.0;
                    y -= 400.0;
                    t::release_at(&self.view, x, y, 1, none, self.ms() + 400);
                    t::press_at(&self.view, x, y, 1, none, self.ms());
                }
                t::motion_at(&self.view, x, y, self.ms());
            }
            self.report("held pan");
            self.reset_log();
            // release with a fling and let it decay
            t::release_at(&self.view, x, y, 1, none, self.ms());
            t::wait_for(10_000, || !t::needs_frames(&self.view));
            self.report("fling decay");
        }

        fn zoom(&self) {
            self.settle();
            let (w, h) = t::info(&self.view).size;
            self.reset_log();
            for round in 0..4 {
                let dir = if round % 2 == 0 { 53.0 } else { -53.0 };
                for _ in 0..5 {
                    t::wheel(&self.view, w / 2.0 + 40.0, h / 2.0, dir, false);
                    frame(&self.view);
                }
                t::wait_for(10_000, || !t::needs_frames(&self.view));
            }
            self.report("wheel zoom");
        }

        fn hover(&self, frames: u32) {
            self.settle();
            let (w, h) = t::info(&self.view).size;
            self.reset_log();
            let (mut x, mut y) = (w * 0.3, h * 0.3);
            for k in 0..frames {
                frame(&self.view);
                x = w * 0.3 + (k as f64 * 9.0) % (w * 0.4);
                y = h * 0.3 + (k as f64 * 5.0) % (h * 0.4);
                t::motion_at(&self.view, x, y, self.ms());
            }
            let _ = (x, y);
            self.report("hover movement");
        }

        fn simulation(&self) {
            self.settle();
            self.reset_log();
            t::reheat(&self.view);
            t::wait_for(30_000, || !t::needs_frames(&self.view));
            self.report("layout settling");
        }

        fn idle(&self) {
            self.settle();
            let (c0, t0) = (cpu_seconds(), Instant::now());
            t::pump(2_000);
            let used = cpu_seconds() - c0;
            println!(
                "    idle: {:.0} ms CPU over {:.1} s settled",
                used * 1e3,
                t0.elapsed().as_secs_f64()
            );
        }
    }

    use std::sync::Arc;

    pub fn run(args: &[String]) {
        gtk::init().expect("an X display (run under xvfb-run)");
        impro_ui::init();
        impro_graph::init();
        let scale: i32 = std::env::var("GDK_SCALE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let sizes: Vec<(i32, i32)> = if args.is_empty() {
            vec![(1920, 1080), (2560, 1440), (3840, 2160)]
        } else {
            args.iter()
                .filter_map(|a| a.split_once('x'))
                .filter_map(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                .collect()
        };
        let cpu = std::fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("model name"))
                    .map(String::from)
            })
            .unwrap_or_default();
        println!(
            "{cpu} | threads {}",
            std::thread::available_parallelism().map_or(1, |n| n.get())
        );
        for (w, h) in sizes {
            println!("== {w}x{h} device pixels, scale {scale}, 3000 nodes");
            let rig = Rig::new((w / scale, h / scale));
            let info = t::info(&rig.view);
            println!(
                "    canvas {:?} logical, {} nodes, {} lines",
                info.size, info.nodes, info.lines
            );
            rig.held_pan(150);
            rig.zoom();
            rig.hover(150);
            rig.simulation();
            rig.idle();
            rig.window.close();
            rig.view.shutdown();
            t::pump(200);
        }
        let _ = Duration::from_secs(0);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("frames") {
        frames::run(&args[1..]);
        return;
    }
    bench(300, 800);
    bench(3000, 8000);
    bench_hot(3000, 8000);
    bench_hover(3000, 8000);
}
