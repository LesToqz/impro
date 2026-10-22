//! Cairo renderer of a [`Scene`]: batched paths per style, off-screen culling,
//! label levels of detail and cached pango layouts.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::raster::{Canvas, LineMask};
use super::scene::Scene;
use crate::geometry::{arrow_length, arrow_opacity, label_font_size, label_opacity, LABEL_WRAP};

/// Alpha buckets: line and arrow opacities are quantised to 1/40, markers to 1/50.
const LINE_LEVELS: f32 = 40.0;
const NODE_LEVELS: f32 = 50.0;
/// Labels smaller than this many pixels are not drawn.
const MIN_LABEL_PX: f64 = 3.0;
/// Cached label bitmaps within this relative size of the wanted one are reused.
const LABEL_SIZE_TOLERANCE: f64 = 0.1;
/// Bitmaps that may be re-rendered in one frame; stale ones are scaled instead.
const LABEL_RENDER_BUDGET: u32 = 120;
const LABEL_PAD: i32 = 2;
/// The cache is dropped when it grows past this many labels.
const LABEL_CACHE_LIMIT: usize = 6000;

/// What one frame drew, for tests and the benchmark.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawStats {
    pub nodes: usize,
    pub lines: usize,
    pub arrows: usize,
    pub labels: usize,
    pub label_renders: usize,
    /// The connection surface was reused without rasterising its geometry.
    pub connections_reused: bool,
    /// Total length of the drawn connections in device pixels.
    pub line_px: f64,
    /// Milliseconds spent on background, lines, arrows, markers and labels.
    pub phase_ms: [f64; 5],
    /// Connections: preparation, rasterising, compositing (milliseconds).
    pub line_ms: [f64; 3],
}

/// Per-frame switches.
#[derive(Clone, Copy, Debug)]
pub struct PaintOptions {
    /// Skip labels (low-quality motion).
    pub hide_labels: bool,
    /// Device pixels per logical pixel.
    pub scale: f64,
}

impl Default for PaintOptions {
    fn default() -> Self {
        PaintOptions {
            hide_labels: false,
            scale: 1.0,
        }
    }
}

struct Label {
    surface: cairo::ImageSurface,
    /// Font size in device pixels the bitmap was rendered at.
    px: f64,
    /// Logical size of the bitmap content in its own pixels.
    width: f64,
    height: f64,
}

/// Pango layouts rendered once per (text, size) into alpha masks.
pub struct LabelCache {
    entries: HashMap<Box<str>, Label>,
    layout: pango::Layout,
    /// Keeps the layout's context alive.
    _measure: cairo::Context,
    family: String,
    scale: f64,
    renders_this_frame: u32,
}

impl LabelCache {
    pub fn new() -> LabelCache {
        let surface = cairo::ImageSurface::create(cairo::Format::A8, 1, 1)
            .unwrap_or_else(|_| unreachable_surface());
        let cr = cairo::Context::new(&surface).unwrap_or_else(|_| unreachable_context());
        let context = pangocairo::functions::create_context(&cr);
        let options = cairo::FontOptions::new().ok();
        if let Some(mut o) = options {
            o.set_antialias(cairo::Antialias::Gray);
            o.set_hint_metrics(cairo::HintMetrics::Off);
            pangocairo::functions::context_set_font_options(&context, Some(&o));
        }
        let layout = pango::Layout::new(&context);
        layout.set_alignment(pango::Alignment::Center);
        layout.set_wrap(pango::WrapMode::WordChar);
        LabelCache {
            entries: HashMap::new(),
            layout,
            _measure: cr,
            family: "sans".to_string(),
            scale: 1.0,
            renders_this_frame: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Sets the font family; bitmaps made with another family are dropped.
    pub fn set_family(&mut self, family: &str) {
        if self.family != family {
            self.family = family.to_string();
            self.entries.clear();
        }
    }

    fn begin_frame(&mut self, scale: f64) {
        self.renders_this_frame = 0;
        if (self.scale - scale).abs() > 1e-9 {
            self.scale = scale;
            self.entries.clear();
        }
        if self.entries.len() > LABEL_CACHE_LIMIT {
            self.entries.clear();
        }
    }

    /// Renders `text` at `px` device pixels, wrapped at `wrap` device pixels.
    fn render(&mut self, text: &str, px: f64, wrap: f64) -> Option<Label> {
        let mut font = pango::FontDescription::new();
        font.set_family(&self.family);
        font.set_absolute_size(px * pango::SCALE as f64);
        self.layout.set_font_description(Some(&font));
        self.layout.set_width((wrap * pango::SCALE as f64) as i32);
        self.layout.set_text(text);
        let (_, logical) = self.layout.pixel_extents();
        let w = logical.width().max(1) + 2 * LABEL_PAD;
        let h = logical.height().max(1) + 2 * LABEL_PAD;
        let surface = cairo::ImageSurface::create(cairo::Format::A8, w, h).ok()?;
        {
            let cr = cairo::Context::new(&surface).ok()?;
            cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
            cr.move_to(
                (LABEL_PAD - logical.x()) as f64,
                (LABEL_PAD - logical.y()) as f64,
            );
            pangocairo::functions::update_layout(&cr, &self.layout);
            pangocairo::functions::show_layout(&cr, &self.layout);
        }
        Some(Label {
            surface,
            px,
            width: w as f64,
            height: h as f64,
        })
    }

    /// The bitmap for a label wanted at `px`, rendering or reusing as budget allows.
    fn get(
        &mut self,
        text: &str,
        px: f64,
        font_world: f64,
    ) -> Option<(cairo::ImageSurface, f64, f64, f64)> {
        let stale = match self.entries.get(text) {
            Some(l) => (px / l.px - 1.0).abs() > LABEL_SIZE_TOLERANCE,
            None => true,
        };
        if stale {
            let exists = self.entries.contains_key(text);
            let allowed = if exists {
                self.renders_this_frame < LABEL_RENDER_BUDGET
            } else {
                self.renders_this_frame < LABEL_RENDER_BUDGET * 2
            };
            if allowed {
                self.renders_this_frame += 1;
                let wrap = LABEL_WRAP as f64 * px / font_world;
                if let Some(label) = self.render(text, px, wrap) {
                    self.entries.insert(text.into(), label);
                }
            }
        }
        self.entries
            .get(text)
            .map(|l| (l.surface.clone(), l.px, l.width, l.height))
    }
}

impl Default for LabelCache {
    fn default() -> Self {
        Self::new()
    }
}

// Surface and context creation only fail on memory exhaustion of a 1x1 image.
fn unreachable_surface() -> cairo::ImageSurface {
    cairo::ImageSurface::create(cairo::Format::A8, 1, 1).expect("1x1 surface")
}

fn unreachable_context() -> cairo::Context {
    cairo::Context::new(unreachable_surface()).expect("context")
}

/// Reusable drawing state: scratch arrays, the back buffer and the label cache.
pub struct Painter {
    sx: Vec<f32>,
    sy: Vec<f32>,
    sr: Vec<f32>,
    keys: Vec<u64>,
    segments: Vec<[f32; 4]>,
    dense: bool,
    label_cells: Vec<bool>,
    connections: Option<cairo::ImageSurface>,
    connections_key: Option<u64>,
    connection_stats: DrawStats,
    /// Threads the connection raster may use.
    pub threads: usize,
    mask: LineMask,
    back: Option<cairo::ImageSurface>,
    back_size: (i32, i32),
    pub labels: LabelCache,
    pub stats: DrawStats,
}

impl Default for Painter {
    fn default() -> Self {
        Self::new()
    }
}

impl Painter {
    pub fn new() -> Painter {
        Painter {
            sx: Vec::new(),
            sy: Vec::new(),
            sr: Vec::new(),
            keys: Vec::new(),
            segments: Vec::new(),
            dense: false,
            label_cells: Vec::new(),
            connections: None,
            connections_key: None,
            connection_stats: DrawStats::default(),
            threads: std::thread::available_parallelism().map_or(1, |n| n.get().min(4)),
            mask: LineMask::default(),
            back: None,
            back_size: (0, 0),
            labels: LabelCache::new(),
            stats: DrawStats::default(),
        }
    }

    /// Paints the scene into a `w × h` logical-pixel target.
    pub fn paint(
        &mut self,
        cr: &cairo::Context,
        scene: &Scene,
        w: f64,
        h: f64,
        opts: PaintOptions,
    ) {
        if self.render(scene, w, h, opts).is_none() {
            return;
        }
        if let Some(back) = &self.back {
            let _ = cr.save();
            cr.set_operator(cairo::Operator::Source);
            let _ = cr.set_source_surface(back, 0.0, 0.0);
            let _ = cr.paint();
            let _ = cr.restore();
        }
    }

    /// Renders the scene into the back buffer and returns it.
    pub fn render(
        &mut self,
        scene: &Scene,
        w: f64,
        h: f64,
        opts: PaintOptions,
    ) -> Option<&cairo::ImageSurface> {
        self.stats = DrawStats::default();
        let dev = opts.scale.max(1.0);
        let (pw, ph) = (
            (w * dev).ceil().max(1.0) as i32,
            (h * dev).ceil().max(1.0) as i32,
        );
        if self.back.is_none() || self.back_size != (pw, ph) {
            let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, pw, ph).ok()?;
            surface.set_device_scale(dev, dev);
            self.back = Some(surface);
            self.back_size = (pw, ph);
        } else if let Some(b) = &self.back {
            b.set_device_scale(dev, dev);
        }
        let mut clock = std::time::Instant::now();
        let mut lap = |slot: usize, stats: &mut DrawStats| {
            stats.phase_ms[slot] = clock.elapsed().as_secs_f64() * 1e3;
            clock = std::time::Instant::now();
        };
        let mut back = self.back.take()?;
        let bg = scene.palette.background;
        if let Ok(cr) = cairo::Context::new(&back) {
            cr.set_operator(cairo::Operator::Source);
            cr.set_source_rgb(bg.r as f64, bg.g as f64, bg.b as f64);
            let _ = cr.paint();
        }
        lap(0, &mut self.stats);
        let drawing = !scene.is_empty() || !scene.ghosts.is_empty();
        if drawing {
            self.project(scene, dev as f32);
            let visible = self
                .sx
                .iter()
                .zip(&self.sy)
                .filter(|(x, y)| **x >= 0.0 && **x <= pw as f32 && **y >= 0.0 && **y <= ph as f32)
                .count();
            // Below this density keep the original rendering, including fit
            // views whose direction markers have already faded away.
            self.dense = visible as f64 / (w * h).max(1.0) > 0.0008
                && arrow_opacity(scene.cam.cam.zoom) > 0.0;
            let cacheable = !scene.needs_frames();
            let key = cacheable.then(|| self.connection_key(scene, pw, ph, dev));
            let reuse = cacheable && self.connections_key == key;
            if reuse {
                if let (Some(layer), Ok(cr)) = (&self.connections, cairo::Context::new(&back)) {
                    let _ = cr.set_source_surface(layer, 0.0, 0.0);
                    let _ = cr.paint();
                }
                self.stats.lines = self.connection_stats.lines;
                self.stats.line_px = self.connection_stats.line_px;
                self.stats.arrows = self.connection_stats.arrows;
                self.stats.connections_reused = true;
                lap(1, &mut self.stats);
            } else {
                if !scene.ghosts.is_empty() {
                    if let Some(mut canvas) = Self::canvas(&mut back, pw, ph) {
                        self.ghosts(&mut canvas, scene, dev);
                    }
                }
                if let Ok(cr) = cairo::Context::new(&back) {
                    self.lines(&cr, scene, pw as usize, ph as usize, dev as f32);
                }
                lap(1, &mut self.stats);
                if let Some(mut canvas) = Self::canvas(&mut back, pw, ph) {
                    self.arrows(&mut canvas, scene, dev as f32);
                }
                self.connections_key = None;
                if cacheable {
                    let layer = self
                        .connections
                        .take()
                        .filter(|s| s.width() == pw && s.height() == ph)
                        .or_else(|| {
                            cairo::ImageSurface::create(cairo::Format::ARgb32, pw, ph).ok()
                        });
                    if let Some(layer) = layer {
                        layer.set_device_scale(dev, dev);
                        if let Ok(cr) = cairo::Context::new(&layer) {
                            cr.set_operator(cairo::Operator::Source);
                            let _ = cr.set_source_surface(&back, 0.0, 0.0);
                            let _ = cr.paint();
                            self.connections_key = key;
                            self.connection_stats = self.stats;
                        }
                        self.connections = Some(layer);
                    }
                }
            }
            lap(2, &mut self.stats);
            if let Some(mut canvas) = Self::canvas(&mut back, pw, ph) {
                self.nodes(&mut canvas, scene, dev as f32);
                lap(3, &mut self.stats);
            }
        }
        if !scene.is_empty() {
            if let Ok(cr) = cairo::Context::new(&back) {
                if !opts.hide_labels {
                    self.draw_labels(&cr, scene, w, h, dev);
                } else if let Some(hl) = scene.highlight().filter(|h| *h < scene.len()) {
                    self.labels.begin_frame(dev);
                    self.label(&cr, scene, hl, w, h, dev, true);
                }
            }
        }
        lap(4, &mut self.stats);
        self.back = Some(back);
        self.back.as_ref()
    }

    /// Pixel access to the back buffer; the borrow ends with the returned value.
    fn canvas(back: &mut cairo::ImageSurface, w: i32, h: i32) -> Option<Canvas<'_>> {
        let stride = back.stride() as usize;
        let data = back.data().ok()?;
        Some(Canvas::new_guarded(data, w as usize, h as usize, stride))
    }

    /// Device-pixel centres and radii of every node.
    fn project(&mut self, scene: &Scene, dev: f32) {
        let n = scene.len();
        let cam = scene.cam.cam;
        let (s, rs) = (cam.zoom, cam.zoom.sqrt());
        let (xs, ys, rad) = (scene.layout.x(), scene.layout.y(), scene.layout.radius());
        self.sx.clear();
        self.sy.clear();
        self.sr.clear();
        let d = dev as f64;
        for i in 0..n {
            self.sx.push(((cam.pan_x + s * xs[i] as f64) * d) as f32);
            self.sy.push(((cam.pan_y + s * ys[i] as f64) * d) as f32);
            self.sr.push((rad[i] as f64 * rs * d) as f32);
        }
    }

    /// Fingerprint every input used by the connection layer. The projected
    /// geometry includes camera, layout and device scale; node colours and
    /// label offsets are deliberately independent of this layer.
    fn connection_key(&self, scene: &Scene, w: i32, h: i32, dev: f64) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (w, h, dev.to_bits(), scene.style.show_arrow).hash(&mut hash);
        scene.cam.cam.zoom.to_bits().hash(&mut hash);
        scene.disp.line.to_bits().hash(&mut hash);
        for values in [
            &self.sx,
            &self.sy,
            &self.sr,
            &scene.line_alpha,
            &scene.line_mix,
            &scene.arrow_alpha,
        ] {
            values.len().hash(&mut hash);
            for value in values {
                value.to_bits().hash(&mut hash);
            }
        }
        for line in &scene.lines {
            (line.a, line.b).hash(&mut hash);
        }
        scene.arrows.hash(&mut hash);
        for color in [
            scene.palette.background,
            scene.palette.line,
            scene.palette.line_highlight,
            scene.palette.arrow,
        ] {
            for value in [color.r, color.g, color.b, color.a] {
                value.to_bits().hash(&mut hash);
            }
        }
        hash.finish()
    }

    fn ghosts(&mut self, canvas: &mut Canvas<'_>, scene: &Scene, dev: f64) {
        let cam = scene.cam.cam;
        let rs = cam.zoom.sqrt();
        for g in &scene.ghosts {
            let gx = (cam.pan_x + cam.zoom * g.x as f64) * dev;
            let gy = (cam.pan_y + cam.zoom * g.y as f64) * dev;
            canvas.disc(
                gx as f32,
                gy as f32,
                (g.radius as f64 * rs * dev) as f32,
                g.color,
                g.alpha,
            );
        }
    }

    fn lines(&mut self, cr: &cairo::Context, scene: &Scene, cw: usize, ch: usize, dev: f32) {
        let (wf, hf) = (cw as f32, ch as f32);
        let line_alpha = scene.palette.line.a;
        self.keys.clear();
        for (k, l) in scene.lines.iter().enumerate() {
            let (a, b) = (l.a as usize, l.b as usize);
            let (x0, y0, x1, y1) = (self.sx[a], self.sy[a], self.sx[b], self.sy[b]);
            if (x0 < 0.0 && x1 < 0.0)
                || (x0 > wf && x1 > wf)
                || (y0 < 0.0 && y1 < 0.0)
                || (y0 > hf && y1 > hf)
            {
                continue;
            }
            let qa = (scene.line_alpha[k] * line_alpha * LINE_LEVELS).round() as u64;
            let qm = (scene.line_mix[k] * 10.0).round() as u64;
            if qa == 0 {
                continue;
            }
            self.keys.push((qa << 4 | qm) << 32 | k as u64);
        }
        self.keys.sort_unstable();
        let width = scene.disp.line as f32 * dev;
        let mut clock = std::time::Instant::now();
        let mut tick = |slot: usize, stats: &mut DrawStats| {
            stats.line_ms[slot] += clock.elapsed().as_secs_f64() * 1e3;
            clock = std::time::Instant::now();
        };
        if !self.mask.begin(cw, ch, dev as f64) {
            return;
        }
        let mut i = 0;
        while i < self.keys.len() {
            let key = self.keys[i] >> 32;
            let mut j = i;
            self.segments.clear();
            while j < self.keys.len() && self.keys[j] >> 32 == key {
                let l = &scene.lines[(self.keys[j] & 0xffff_ffff) as usize];
                let (a, b) = (l.a as usize, l.b as usize);
                let (dx, dy) = (self.sx[b] - self.sx[a], self.sy[b] - self.sy[a]);
                let d = (dx * dx + dy * dy).sqrt();
                let len = d - self.sr[a] - self.sr[b];
                if len > 0.0 && d > 0.0 {
                    let (ux, uy) = (dx / d, dy / d);
                    let (ra, rb) = (self.sr[a], self.sr[b]);
                    self.segments.push([
                        self.sx[a] + ux * ra,
                        self.sy[a] + uy * ra,
                        self.sx[b] - ux * rb,
                        self.sy[b] - uy * rb,
                    ]);
                    self.stats.lines += 1;
                    self.stats.line_px += len as f64;
                }
                j += 1;
            }
            tick(0, &mut self.stats);
            {
                let Some(mut pixels) = self.mask.lines() else {
                    return;
                };
                pixels.segments(&self.segments, width, self.threads);
            }
            tick(1, &mut self.stats);
            let (qa, qm) = ((key >> 4) as f32, (key & 15) as f32);
            let c = scene
                .palette
                .line
                .mix(scene.palette.line_highlight, qm / 10.0);
            self.mask.composite(cr, c.rgb(), qa / LINE_LEVELS);
            tick(2, &mut self.stats);
            i = j;
        }
    }

    fn arrows(&mut self, canvas: &mut Canvas<'_>, scene: &Scene, dev: f32) {
        let zoom = scene.cam.cam.zoom;
        let fade = arrow_opacity(zoom) as f32;
        if !scene.style.show_arrow || fade <= 0.0 {
            return;
        }
        let (wf, hf) = (canvas.w as f32 + 20.0 * dev, canvas.h as f32 + 20.0 * dev);
        let arrow = scene.palette.arrow;
        let lw = scene.disp.line;
        let len = (arrow_length(lw) * 4.0 / 3.0) as f32 * dev;
        let half = len / 2.0;
        let notch = len * 0.75;
        let gap = zoom as f32 * dev;
        for (k, &(a, b)) in scene.arrows.iter().enumerate() {
            let (a, b) = (a as usize, b as usize);
            if self.sx[b] < -20.0 * dev
                || self.sx[b] > wf
                || self.sy[b] < -20.0 * dev
                || self.sy[b] > hf
            {
                continue;
            }
            let alpha = scene.arrow_alpha[k] * arrow.a * fade;
            if (alpha * LINE_LEVELS).round() < 1.0 {
                continue;
            }
            let (dx, dy) = (self.sx[b] - self.sx[a], self.sy[b] - self.sy[a]);
            let d = (dx * dx + dy * dy).sqrt();
            if d <= 0.0 || d - self.sr[a] - self.sr[b] <= lw as f32 * dev {
                continue;
            }
            let (ux, uy) = (dx / d, dy / d);
            let back = self.sr[b] + gap;
            let (tx, ty) = (self.sx[b] - ux * back, self.sy[b] - uy * back);
            // local frame: x along the line toward the tip, y to its side
            let p = |lx: f32, ly: f32| (tx + ux * lx - uy * ly, ty + uy * lx + ux * ly);
            canvas.dart(
                (tx, ty),
                p(-len, -half),
                p(-notch, 0.0),
                p(-len, half),
                arrow.rgb(),
                alpha,
            );
            self.stats.arrows += 1;
        }
    }

    fn nodes(&mut self, canvas: &mut Canvas<'_>, scene: &Scene, dev: f32) {
        let n = scene.len();
        let hl = scene.highlight();
        let (wf, hf) = (canvas.w as f32, canvas.h as f32);
        for i in 0..n {
            if hl == Some(i) {
                continue;
            }
            let r = self.sr[i];
            if self.sx[i] + r + 1.0 < 0.0
                || self.sx[i] - r - 1.0 > wf
                || self.sy[i] + r + 1.0 < 0.0
                || self.sy[i] - r - 1.0 > hf
            {
                continue;
            }
            let alpha = scene.alpha[i] * scene.base_alpha(i);
            if (alpha * NODE_LEVELS).round() < 1.0 {
                continue;
            }
            canvas.disc(self.sx[i], self.sy[i], r, scene.color[i], alpha);
            self.stats.nodes += 1;
        }
        if let Some(i) = hl.filter(|i| *i < n) {
            canvas.disc(
                self.sx[i],
                self.sy[i],
                self.sr[i],
                scene.color[i],
                scene.alpha[i],
            );
            let ring = scene.palette.ring;
            canvas.ring(
                self.sx[i],
                self.sy[i],
                self.sr[i] + 0.5 * dev,
                dev,
                ring.rgb(),
                ring.a,
            );
            self.stats.nodes += 1;
        }
    }

    fn draw_labels(&mut self, cr: &cairo::Context, scene: &Scene, w: f64, h: f64, dev: f64) {
        self.labels.begin_frame(dev);
        self.label_cells.resize(
            (w / 48.0).ceil() as usize * (h / 20.0).ceil() as usize,
            false,
        );
        self.label_cells.fill(false);
        let hl = scene.highlight().filter(|h| *h < scene.len());
        if label_opacity(scene.cam.cam.zoom, scene.disp.label_detail) > 0.001 {
            for i in 0..scene.len() {
                if hl != Some(i) {
                    self.label(cr, scene, i, w, h, dev, false);
                }
            }
        }
        if let Some(i) = hl {
            self.label(cr, scene, i, w, h, dev, true);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn label(
        &mut self,
        cr: &cairo::Context,
        scene: &Scene,
        i: usize,
        w: f64,
        h: f64,
        dev: f64,
        hovered: bool,
    ) {
        let cam = scene.cam.cam;
        let (s, rs) = (cam.zoom, cam.zoom.sqrt());
        let text_a = scene.palette.text.a as f64;
        let fade = label_opacity(s, scene.disp.label_detail);
        let alpha = if hovered {
            text_a
        } else {
            fade * scene.alpha[i] as f64 * text_a
        };
        if alpha <= 0.001 {
            return;
        }
        let (sx, sy, sr) = (
            self.sx[i] as f64 / dev,
            self.sy[i] as f64 / dev,
            self.sr[i] as f64 / dev,
        );
        if sx < -300.0 || sx > w + 300.0 || sy < -20.0 - sr * 3.0 || sy > h + 200.0 {
            return;
        }
        let world = scene.layout.radius()[i];
        let font = label_font_size(world) as f64;
        let on_screen = if hovered && s < 1.0 { font } else { font * rs };
        if on_screen < MIN_LABEL_PX {
            return;
        }
        let drop = scene.drop[i] as f64;
        let text = scene.graph.nodes[i].label();
        let renders = self.labels.renders_this_frame;
        let Some((surface, px, width, height)) = self.labels.get(text, on_screen * dev, font)
        else {
            return;
        };
        let k = on_screen * dev / px / dev; // logical pixels per bitmap pixel
        let lw = width * k;
        let ly = sy + (world as f64 + 5.0) * rs + drop;
        let (x, y) = (sx - lw / 2.0, ly - LABEL_PAD as f64 * k);
        if x + lw < 0.0 || x > w || y + height * k < 0.0 || y > h {
            return;
        }
        if self.dense && !hovered {
            let cols = (w / 48.0).ceil() as usize;
            let rows = (h / 20.0).ceil() as usize;
            let x0 = (x.max(0.0) / 48.0) as usize;
            let x1 = ((x + lw).max(0.0) / 48.0).ceil() as usize;
            let y0 = (y.max(0.0) / 20.0) as usize;
            let y1 = ((y + height * k).max(0.0) / 20.0).ceil() as usize;
            if (y0..y1.min(rows))
                .any(|cy| (x0..x1.min(cols)).any(|cx| self.label_cells[cy * cols + cx]))
            {
                return;
            }
            for cy in y0..y1.min(rows) {
                for cx in x0..x1.min(cols) {
                    self.label_cells[cy * cols + cx] = true;
                }
            }
        }
        let _ = cr.save();
        let c = scene.palette.text;
        cr.set_source_rgba(c.r as f64, c.g as f64, c.b as f64, alpha);
        if (k - 1.0 / dev).abs() < 1e-3 {
            cr.translate((x * dev).round() / dev, (y * dev).round() / dev);
        } else {
            cr.translate(x, y);
        }
        cr.scale(k, k);
        let _ = cr.mask_surface(&surface, 0.0, 0.0);
        let _ = cr.restore();
        self.stats.labels += 1;
        self.stats.label_renders += (self.labels.renders_this_frame - renders) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::scene::Built;
    use crate::Graph;

    fn pixels(painter: &mut Painter, scene: &Scene, scale: f64) -> Vec<u8> {
        let surface = painter
            .render(
                scene,
                240.0,
                180.0,
                PaintOptions {
                    hide_labels: true,
                    scale,
                },
            )
            .expect("surface");
        let mut png = Vec::new();
        surface.write_to_png(&mut png).expect("PNG");
        png
    }

    #[test]
    fn connection_cache_tracks_geometry_style_and_device_scale() {
        let mut scene = Scene::new();
        scene.set_size(240.0, 180.0);
        scene.apply(Built::global(Graph::from_pairs(3, &[(0, 1), (1, 2)])));
        while scene.advance(1.0 / 60.0) {}
        let mut painter = Painter::new();
        let first = pixels(&mut painter, &scene, 1.0);
        assert!(!painter.stats.connections_reused);
        assert_eq!(first, pixels(&mut painter, &scene, 1.0));
        assert!(painter.stats.connections_reused);
        for change in 0..8 {
            match change {
                0 => scene.cam.cam.pan_x += 12.0,
                1 => scene.cam.cam.zoom *= 1.2,
                2 => scene.disp.line *= 2.0,
                3 => scene.palette.line.r = 0.9,
                4 => scene.line_alpha[0] *= 0.5,
                5 => scene.arrow_alpha[0] *= 0.5,
                6 => scene.style.show_arrow = !scene.style.show_arrow,
                _ => scene.palette.background.b *= 0.8,
            }
            let actual = pixels(&mut painter, &scene, 1.0);
            assert!(!painter.stats.connections_reused, "change {change}");
            assert_eq!(
                actual,
                pixels(&mut Painter::new(), &scene, 1.0),
                "change {change}"
            );
        }
        assert_eq!(
            pixels(&mut painter, &scene, 2.0),
            pixels(&mut Painter::new(), &scene, 2.0)
        );
        assert!(!painter.stats.connections_reused);
    }
}
