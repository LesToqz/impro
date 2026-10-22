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

fn main() {
    bench(300, 800);
    bench(3000, 8000);
    bench_hot(3000, 8000);
    bench_hover(3000, 8000);
}
