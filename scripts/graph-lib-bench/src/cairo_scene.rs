//! Candidate B's scene: plain cairo vector. One path per style batch (all
//! edges in one `move_to/line_to` path + one stroke, nodes as batched arcs
//! per colour), Pango labels via a cached bitmap per string used as a mask.

use cairo::{Antialias, Context, Format, ImageSurface};
use crate::fixture::{Fixture, BG, EDGE_COLOR, NODE_COLORS};
use crate::harness::{Cam, LabelDraw, Renderer};
use crate::labels::LabelCache;

pub struct Plain {
    pub surface: ImageSurface,
    scale: f64,
    w: f64,
    h: f64,
    aa: Antialias,
    labels: LabelCache,
    label_surfaces: Vec<Option<ImageSurface>>,
    nodes: usize,
    edges: usize,
    sx: Vec<f32>,
    sy: Vec<f32>,
    sr: Vec<f32>,
}

impl Plain {
    /// A surface of `w × h` logical pixels at device scale `s`.
    pub fn new(w: u32, h: u32, s: u32, aa: Antialias) -> Option<Plain> {
        Some(Plain {
            surface: ImageSurface::create(Format::ARgb32, (w * s) as i32, (h * s) as i32).ok()?,
            scale: s as f64,
            w: w as f64,
            h: h as f64,
            aa,
            labels: LabelCache::new(s as f64),
            label_surfaces: Vec::new(),
            nodes: 0,
            edges: 0,
            sx: Vec::new(),
            sy: Vec::new(),
            sr: Vec::new(),
        })
    }
}

impl Renderer for Plain {
    fn frame(&mut self, fx: &Fixture, cam: &Cam, labels: &[LabelDraw]) {
        let n = fx.x.len();
        self.sx.resize(n, 0.0);
        self.sy.resize(n, 0.0);
        self.sr.resize(n, 0.0);
        for i in 0..n {
            let (x, y) = cam.to_screen(fx.x[i], fx.y[i]);
            self.sx[i] = x;
            self.sy[i] = y;
            self.sr[i] = cam.screen_radius(fx.radius[i]).max(0.8);
        }
        let (w, h) = (self.w as f32, self.h as f32);
        let cr = Context::new(&self.surface).expect("context");
        cr.scale(self.scale, self.scale);
        cr.set_source_rgb(BG[0] as f64, BG[1] as f64, BG[2] as f64);
        cr.set_operator(cairo::Operator::Source);
        let _ = cr.paint();
        cr.set_operator(cairo::Operator::Over);
        // edges: one path, one stroke, one device pixel wide
        cr.set_antialias(Antialias::Default);
        cr.set_line_width(1.0 / self.scale);
        cr.set_source_rgba(
            EDGE_COLOR[0] as f64,
            EDGE_COLOR[1] as f64,
            EDGE_COLOR[2] as f64,
            EDGE_COLOR[3] as f64,
        );
        let mut edges = 0;
        for &(a, b) in &fx.edges {
            let (a, b) = (a as usize, b as usize);
            let (x0, y0, x1, y1) = (self.sx[a], self.sy[a], self.sx[b], self.sy[b]);
            if (x0 < 0.0 && x1 < 0.0)
                || (y0 < 0.0 && y1 < 0.0)
                || (x0 > w && x1 > w)
                || (y0 > h && y1 > h)
            {
                continue;
            }
            cr.move_to(x0 as f64, y0 as f64);
            cr.line_to(x1 as f64, y1 as f64);
            edges += 1;
        }
        let _ = cr.stroke();
        self.edges = edges;
        // nodes: one path per colour, arcs
        cr.set_antialias(self.aa);
        let mut nodes = 0;
        for (class, c) in NODE_COLORS.iter().enumerate() {
            cr.set_source_rgb(c[0] as f64, c[1] as f64, c[2] as f64);
            for i in 0..n {
                if fx.class[i] as usize != class {
                    continue;
                }
                let r = self.sr[i];
                if self.sx[i] + r < 0.0 || self.sx[i] - r > w || self.sy[i] + r < 0.0 || self.sy[i] - r > h {
                    continue;
                }
                cr.new_sub_path();
                cr.arc(self.sx[i] as f64, self.sy[i] as f64, r as f64, 0.0, std::f64::consts::TAU);
                nodes += 1;
            }
            let _ = cr.fill();
        }
        self.nodes = nodes;
        // labels: cached bitmaps used as a mask, one colour
        cr.set_antialias(Antialias::Default);
        cr.set_source_rgba(0.9, 0.93, 0.96, 0.9);
        for l in labels {
            let id = self.labels.get(&fx.names[l.node as usize], l.size);
            if self.label_surfaces.len() <= id {
                self.label_surfaces.resize_with(id + 1, || None);
            }
            if self.label_surfaces[id].is_none() {
                let b = &self.labels.items[id];
                let s = ImageSurface::create_for_data(b.data.clone(), Format::ARgb32, b.width, b.height, b.stride)
                    .expect("label surface");
                self.label_surfaces[id] = Some(s);
            }
            let s = self.label_surfaces[id].as_ref().expect("surface");
            let lw = s.width() as f64 / self.scale;
            let x = ((l.x as f64 - lw / 2.0) * self.scale).round() / self.scale;
            let y = (l.y as f64 * self.scale).round() / self.scale;
            let _ = cr.mask_surface(s, x * 1.0, y * 1.0);
        }
        drop(cr);
        self.surface.flush();
    }
    fn note(&self) -> String {
        format!("nodes {} edges {} label renders {} ({:.0} ms)", self.nodes, self.edges, self.labels.renders, self.labels.render_ms)
    }
}

