//! Candidate C: tiny-skia (CPU, SIMD). Same scene through paths, strokes and
//! pixmap blending. `--cached` renders into an oversized layer once and pans
//! by integer translation, re-rendering when the window leaves the layer or
//! the zoom changes.

use graph_lib_bench::fixture::{self, Fixture, BG, EDGE_COLOR, NODE_COLORS};
use graph_lib_bench::harness::{header, sweep, Cam, LabelDraw, Opts, Renderer};
use graph_lib_bench::labels::LabelCache;
use tiny_skia::{
    BlendMode, Color, FillRule, IntSize, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};

const MARGIN: f64 = 512.0;

struct Skia {
    target: Pixmap,
    layer: Option<(Pixmap, Cam)>,
    cached: bool,
    scale: f32,
    w: f64,
    h: f64,
    labels: LabelCache,
    label_pixmaps: Vec<Option<Pixmap>>,
    renders: usize,
    blits: usize,
}

fn paint(c: [f32; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba(c[0], c[1], c[2], c[3]).expect("colour"));
    p.anti_alias = true;
    p
}

/// Draws background, edges and nodes of `fx` into `pm` for a camera given in
/// logical pixels and a device scale.
fn draw_scene(pm: &mut Pixmap, fx: &Fixture, cam: &Cam, s: f32) {
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    pm.fill(Color::from_rgba(BG[0], BG[1], BG[2], 1.0).expect("bg"));
    let n = fx.x.len();
    let mut sx = Vec::with_capacity(n);
    let mut sy = Vec::with_capacity(n);
    for i in 0..n {
        let (x, y) = cam.to_screen(fx.x[i], fx.y[i]);
        sx.push(x * s);
        sy.push(y * s);
    }
    let mut pb = PathBuilder::new();
    for &(a, b) in &fx.edges {
        let (a, b) = (a as usize, b as usize);
        let (x0, y0, x1, y1) = (sx[a], sy[a], sx[b], sy[b]);
        if (x0 < 0.0 && x1 < 0.0) || (y0 < 0.0 && y1 < 0.0) || (x0 > w && x1 > w) || (y0 > h && y1 > h) {
            continue;
        }
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
    }
    if let Some(path) = pb.finish() {
        let stroke = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pm.stroke_path(&path, &paint(EDGE_COLOR), &stroke, Transform::identity(), None);
    }
    for (class, c) in NODE_COLORS.iter().enumerate() {
        let mut pb = PathBuilder::new();
        for i in 0..n {
            if fx.class[i] as usize != class {
                continue;
            }
            let r = cam.screen_radius(fx.radius[i]).max(0.8) * s;
            if sx[i] + r < 0.0 || sx[i] - r > w || sy[i] + r < 0.0 || sy[i] - r > h {
                continue;
            }
            pb.push_circle(sx[i], sy[i], r);
        }
        if let Some(path) = pb.finish() {
            pm.fill_path(&path, &paint([c[0], c[1], c[2], 1.0]), FillRule::Winding, Transform::identity(), None);
        }
    }
}

impl Renderer for Skia {
    fn frame(&mut self, fx: &Fixture, cam: &Cam, labels: &[LabelDraw]) {
        let s = self.scale;
        if !self.cached {
            draw_scene(&mut self.target, fx, cam, s);
            self.renders += 1;
        } else {
            // window offset into the layer, in logical pixels
            let hit = self.layer.as_ref().and_then(|(_, lc)| {
                if (lc.zoom - cam.zoom).abs() > 1e-12 {
                    return None;
                }
                let x0 = MARGIN + (lc.x - cam.x);
                let y0 = MARGIN + (lc.y - cam.y);
                let ok = (0.0..=2.0 * MARGIN).contains(&x0)
                    && (0.0..=2.0 * MARGIN).contains(&y0)
                    && (x0 * s as f64).fract() == 0.0
                    && (y0 * s as f64).fract() == 0.0;
                ok.then_some((x0, y0))
            });
            let (x0, y0) = match hit {
                Some(p) => {
                    self.blits += 1;
                    p
                }
                None => {
                    let lw = ((self.w + 2.0 * MARGIN) * s as f64) as u32;
                    let lh = ((self.h + 2.0 * MARGIN) * s as f64) as u32;
                    if self.layer.as_ref().map(|(p, _)| (p.width(), p.height())) != Some((lw, lh)) {
                        self.layer = Some((Pixmap::new(lw, lh).expect("layer"), *cam));
                    }
                    let (pm, lc) = self.layer.as_mut().expect("layer");
                    *lc = *cam;
                    let shifted = Cam {
                        x: cam.x + MARGIN,
                        y: cam.y + MARGIN,
                        zoom: cam.zoom,
                    };
                    draw_scene(pm, fx, &shifted, s);
                    self.renders += 1;
                    (MARGIN, MARGIN)
                }
            };
            let (layer, _) = self.layer.as_ref().expect("layer");
            let pp = PixmapPaint {
                blend_mode: BlendMode::Source,
                ..PixmapPaint::default()
            };
            self.target.draw_pixmap(
                (-x0 * s as f64) as i32,
                (-y0 * s as f64) as i32,
                layer.as_ref(),
                &pp,
                Transform::identity(),
                None,
            );
        }
        let pp = PixmapPaint {
            opacity: 0.9,
            ..PixmapPaint::default()
        };
        for l in labels {
            let id = self.labels.get(&fx.names[l.node as usize], l.size);
            if self.label_pixmaps.len() <= id {
                self.label_pixmaps.resize_with(id + 1, || None);
            }
            if self.label_pixmaps[id].is_none() {
                let b = &self.labels.items[id];
                debug_assert_eq!(b.stride, b.width * 4);
                self.label_pixmaps[id] = Pixmap::from_vec(
                    b.data.clone(),
                    IntSize::from_wh(b.width as u32, b.height as u32).expect("size"),
                );
            }
            if let Some(p) = &self.label_pixmaps[id] {
                let x = (l.x * s - p.width() as f32 / 2.0).round() as i32;
                let y = (l.y * s).round() as i32;
                self.target.draw_pixmap(x, y, p.as_ref(), &pp, Transform::identity(), None);
            }
        }
    }
    fn note(&self) -> String {
        format!(
            "scene renders {} blits {} label renders {} ({:.0} ms)",
            self.renders, self.blits, self.labels.renders, self.labels.render_ms
        )
    }
}

fn main() {
    let o = Opts::parse();
    let fx = fixture::by_name(&o.fixture);
    let cached = o.has("--cached");
    header(&format!("C tiny-skia (cached layer + integer translate: {cached})"), &o);
    sweep(&fx, &o, |w, h, s| {
        Some(Box::new(Skia {
            target: Pixmap::new(w * s, h * s)?,
            layer: None,
            cached,
            scale: s as f32,
            w: w as f64,
            h: h as f64,
            labels: LabelCache::new(s as f64),
            label_pixmaps: Vec::new(),
            renders: 0,
            blits: 0,
        }))
    });
}
