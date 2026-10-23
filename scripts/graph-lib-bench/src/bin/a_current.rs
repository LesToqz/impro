//! Candidate A: the real `impro-graph` painter driven like
//! `crates/impro-graph/examples/graph_bench.rs`.
//! Flags: `--motion` (moving level of detail), `--threads N`.

use std::cell::RefCell;
use std::rc::Rc;

use graph_lib_bench::fixture::{self, Fixture};
use graph_lib_bench::harness::{header, sweep, Cam, LabelDraw, Opts, Renderer};
use impro_graph::geometry::Camera;
use impro_graph::view::draw::{PaintOptions, Painter};
use impro_graph::view::scene::{Built, Scene};
use impro_graph::Graph;

struct Cur {
    scene: Rc<RefCell<Scene>>,
    painter: Painter,
    scale: f64,
    labels: bool,
    w: f64,
    h: f64,
}

impl Renderer for Cur {
    fn frame(&mut self, _fx: &Fixture, cam: &Cam, _labels: &[LabelDraw]) {
        let mut scene = self.scene.borrow_mut();
        scene.cam.cam = Camera {
            pan_x: cam.x,
            pan_y: cam.y,
            zoom: cam.zoom,
        };
        scene.cam.target_zoom = cam.zoom;
        let opts = PaintOptions {
            hide_labels: !self.labels,
            scale: self.scale,
        };
        let surface = self.painter.render(&scene, self.w, self.h, opts);
        if let Some(s) = surface {
            s.flush();
        }
    }
    fn note(&self) -> String {
        let s = self.painter.stats;
        format!(
            "nodes {} lines {} arrows {} labels {}",
            s.nodes, s.lines, s.arrows, s.labels
        )
    }
}

fn build_scene(fx: &Fixture) -> Scene {
    let pairs: Vec<(u32, u32)> = fx.edges.clone();
    let mut scene = Scene::new();
    scene.set_size(1920.0, 1080.0);
    scene.apply(Built::global(Graph::from_pairs(fx.x.len(), &pairs)));
    scene.layout.settle(600);
    for i in 0..fx.x.len() {
        scene.layout.set_position(i, fx.x[i], fx.y[i]);
    }
    for _ in 0..600 {
        if !scene.advance(1.0 / 60.0) {
            break;
        }
    }
    scene
}

fn main() {
    let o = Opts::parse();
    let fx = fixture::by_name(&o.fixture);
    let scene = Rc::new(RefCell::new(build_scene(&fx)));
    let motion = o.has("--motion");
    let threads: Option<usize> = o.value("--threads").and_then(|v| v.parse().ok());
    header(
        &format!(
            "A current Painter (motion={motion}, threads={})",
            threads.map_or("default(≤4)".into(), |t| t.to_string())
        ),
        &o,
    );
    sweep(&fx, &o, |w, h, s| {
        scene.borrow_mut().set_size(w as f64, h as f64);
        let mut painter = Painter::new();
        painter.motion = motion;
        if let Some(t) = threads {
            painter.threads = t;
        }
        Some(Box::new(Cur {
            scene: scene.clone(),
            painter,
            scale: s as f64,
            labels: o.labels,
            w: w as f64,
            h: h as f64,
        }))
    });
}
