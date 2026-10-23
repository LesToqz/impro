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
/// During motion nearby masks may be reused until the exact settled frame.
const LABEL_SIZE_TOLERANCE: f64 = 0.1;
/// Bitmaps that may be re-rendered in one frame; stale ones are scaled instead.
const LABEL_RENDER_BUDGET: u32 = 120;
const LABEL_PAD: i32 = 2;
/// While moving, markers are drawn only on connections whose visible shaft is
/// at least this many marker lengths (dense scenes draw none).
const MOTION_ARROW_SHAFTS: f32 = 1.5;
/// Visible nodes per logical pixel from which a scene counts as dense.
const DENSE_NODES: f64 = 0.0008;
/// Declutter cells in logical pixels, anchored to the world (they travel with
/// the camera) so a pan never changes which of two labels wins.
const CELL_W: f64 = 48.0;
const CELL_H: f64 = 20.0;
/// Room around the pane that still takes part in the declutter.
const CELL_MARGIN_X: f64 = 336.0;
const CELL_MARGIN_TOP: f64 = 80.0;
const CELL_MARGIN_BOTTOM: f64 = 240.0;
/// The cache is dropped when it grows past this many labels.
const LABEL_CACHE_LIMIT: usize = 6000;

/// Connection groups with at least this many lines keep their coverage.
const GROUP_CACHE_MIN: usize = 256;
/// Coverage masks kept for the next frame (one full canvas of bytes each).
const GROUP_CACHE_SLOTS: usize = 6;

/// Marker metrics of one frame, in device pixels.
struct ArrowDims {
    fade: f32,
    wf: f32,
    hf: f32,
    lw: f64,
    len: f32,
    half: f32,
    notch: f32,
    gap: f32,
}

/// Keeps marker groups apart from connection groups in the shared cache.
const ARROW_SALT: u64 = 0x9e37_79b9_7f4a_7c15;

/// The coverage of one connection group, kept while its geometry stays.
struct Group {
    key: u64,
    lines: usize,
    line_px: f64,
    used: u64,
    mask: LineMask,
}

/// What one frame drew, for tests and the benchmark.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DrawStats {
    pub nodes: usize,
    pub lines: usize,
    pub arrows: usize,
    pub labels: usize,
    pub label_renders: usize,
    /// Visible masks still waiting for the per-frame render budget.
    pub labels_pending: bool,
    /// The connection surface was reused without rasterising its geometry.
    pub connections_reused: bool,
    /// Connection and marker groups repainted from kept coverage.
    pub group_hits: usize,
    /// Total length of the drawn connections in device pixels.
    pub line_px: f64,
    /// Milliseconds spent on background, lines, arrows, markers and labels.
    pub phase_ms: [f64; 5],
    /// Connections: preparation, rasterising, compositing (milliseconds).
    pub line_ms: [f64; 3],
}

/// One label as painted, in logical pixels (tests).
#[derive(Clone, Debug, PartialEq)]
pub struct LabelDrawn {
    pub node: usize,
    /// Top-left corner of the drawn bitmap.
    pub x: f64,
    pub y: f64,
    /// Drawn size.
    pub w: f64,
    pub h: f64,
    pub alpha: f64,
    /// Font size in device pixels the bitmap was rendered at.
    pub bitmap_px: f64,
    /// The bitmap was stretched to another size than it was rendered at.
    pub stretched: bool,
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
    pending: bool,
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
            pending: false,
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

    pub(super) fn metrics(&self, text: &str) -> Option<(f64, f64, f64)> {
        self.entries
            .get(text)
            .map(|label| (label.px, label.width, label.height))
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
        self.pending = false;
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
        exact: bool,
    ) -> Option<(cairo::ImageSurface, f64, f64, f64)> {
        let stale = match self.entries.get(text) {
            Some(l) if exact => l.px != px,
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
            } else {
                self.pending = true;
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

/// Nominal metrics used by both the painter and its read-only diagnostics.
pub(super) fn label_metrics(scene: &Scene, i: usize) -> super::testing::LabelPaint {
    let zoom = scene.cam.cam.zoom;
    let hovered = scene.highlight() == Some(i);
    let font_size = label_font_size(scene.layout.radius()[i]) as f64;
    super::testing::LabelPaint {
        font_size,
        screen_font_size: if hovered && zoom < 1.0 {
            font_size
        } else {
            font_size * zoom.sqrt()
        },
        wrap_width: LABEL_WRAP as f64,
        alpha: scene.palette.text.a as f64
            * if hovered {
                1.0
            } else {
                label_opacity(zoom, scene.disp.label_detail) * scene.alpha[i] as f64
            },
        bitmap_font_size: None,
        bitmap_size: None,
    }
}

/// Reusable drawing state: scratch arrays, the back buffer and the label cache.
pub struct Painter {
    sx: Vec<f32>,
    sy: Vec<f32>,
    sr: Vec<f32>,
    keys: Vec<u64>,
    segments: Vec<[f32; 4]>,
    shapes: Vec<[(f32, f32); 4]>,
    dense: bool,
    label_cells: Vec<bool>,
    label_grid: (usize, usize),
    label_origin: (f64, f64),
    connections: Option<cairo::ImageSurface>,
    connections_key: Option<u64>,
    connection_stats: DrawStats,
    /// Threads the connection raster may use.
    pub threads: usize,
    /// Draw the cheaper "moving" level of detail: half-resolution lines, flat
    /// markers on long enough connections, only the most important labels.
    /// Set it while the scene's geometry changes between frames; the next
    /// frame without it renders the full-quality picture.
    pub motion: bool,
    mask: LineMask,
    groups: Vec<Group>,
    group_clock: u64,
    back: Option<cairo::ImageSurface>,
    back_size: (i32, i32),
    /// Tests: keep a copy of every frame as it stood before its labels.
    pub capture: bool,
    pub(super) unlabelled: Option<cairo::ImageSurface>,
    pub(super) label_log: Vec<LabelDrawn>,
    pub labels: LabelCache,
    pub stats: DrawStats,
}

impl Default for Painter {
    fn default() -> Self {
        Self::new()
    }
}

impl Painter {
    /// True while visible label masks need another bounded warm-up frame.
    pub fn needs_frames(&self) -> bool {
        self.stats.labels_pending
    }
    pub(super) fn arrow_paint(&self, scene: &Scene, dev: f32) -> Vec<super::testing::ArrowPaint> {
        let (w, h) = scene.size;
        let Some(dims) = self.arrow_dims(
            scene,
            (w * dev as f64) as usize,
            (h * dev as f64) as usize,
            dev,
        ) else {
            return Vec::new();
        };
        // Screen arrays belong to the last painted graph and may be older
        // than an asynchronous data delivery.
        if self.sx.len() != scene.len() {
            return Vec::new();
        }
        scene
            .arrows
            .iter()
            .enumerate()
            .filter_map(|(k, &(a, b))| {
                let (alpha, vertices) = self.arrow_shape(scene, k, &dims, dev)?;
                Some(super::testing::ArrowPaint {
                    endpoints: [
                        scene.graph.nodes[a as usize].id.to_string(),
                        scene.graph.nodes[b as usize].id.to_string(),
                    ],
                    alpha: (alpha * LINE_LEVELS).round() / LINE_LEVELS,
                    vertices: vertices.map(|(x, y)| (x / dev, y / dev)),
                })
            })
            .collect()
    }
    pub fn new() -> Painter {
        Painter {
            sx: Vec::new(),
            sy: Vec::new(),
            sr: Vec::new(),
            keys: Vec::new(),
            segments: Vec::new(),
            shapes: Vec::new(),
            dense: false,
            label_cells: Vec::new(),
            label_grid: (0, 0),
            label_origin: (0.0, 0.0),
            connections: None,
            connections_key: None,
            connection_stats: DrawStats::default(),
            threads: std::thread::available_parallelism().map_or(1, |n| n.get().min(4)),
            motion: false,
            mask: LineMask::default(),
            groups: Vec::new(),
            group_clock: 0,
            back: None,
            back_size: (0, 0),
            capture: false,
            unlabelled: None,
            label_log: Vec::new(),
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
        self.label_log.clear();
        self.labels.pending = false;
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
            // A Schmitt trigger: the density of the nodes in view changes with
            // every pan, and a flip would re-declutter every label at once.
            let density = visible as f64 / (w * h).max(1.0);
            let limit = if self.dense {
                DENSE_NODES * 0.9
            } else {
                DENSE_NODES * 1.1
            };
            self.dense = density > limit && arrow_opacity(scene.cam.cam.zoom) > 0.0;
            let cacheable = !scene.needs_frames() && !self.motion;
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
                if !self.motion && scene.needs_frames() {
                    if let Ok(cr) = cairo::Context::new(&back) {
                        self.arrows_masked(&cr, scene, pw as usize, ph as usize, dev as f32);
                    }
                } else if let Some(mut canvas) = Self::canvas(&mut back, pw, ph) {
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
        if self.capture {
            self.unlabelled = cairo::ImageSurface::create(cairo::Format::ARgb32, pw, ph)
                .ok()
                .inspect(|copy| {
                    if let Ok(cr) = cairo::Context::new(copy) {
                        cr.set_operator(cairo::Operator::Source);
                        let _ = cr.set_source_surface(&back, 0.0, 0.0);
                        let _ = cr.paint();
                    }
                });
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
        self.stats.labels_pending = self.labels.pending;
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
        let flat = self.motion;
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
        // Moving frames and one-off pictures use the scratch mask. Otherwise a
        // group's coverage is kept: while appearance eases (hover fades) the
        // geometry and the membership of a group stay, only its colour moves.
        let keep = !flat;
        let geometry = if keep {
            self.line_geometry_key(cw, ch, dev, scene)
        } else {
            0
        };
        let mut i = 0;
        while i < self.keys.len() {
            let key = self.keys[i] >> 32;
            let mut j = i;
            let mut member = 0xcbf2_9ce4_8422_2325u64;
            while j < self.keys.len() && self.keys[j] >> 32 == key {
                let l = &scene.lines[(self.keys[j] & 0xffff_ffff) as usize];
                member = (member ^ (u64::from(l.a) << 32 | u64::from(l.b)))
                    .wrapping_mul(0x0100_0000_01b3);
                j += 1;
            }
            let (qa, qm) = ((key >> 4) as f32, (key & 15) as f32);
            let c = scene
                .palette
                .line
                .mix(scene.palette.line_highlight, qm / 10.0);
            // Moving frames never keep coverage: their key would hold no
            // geometry and a later frame would replay another camera's lines.
            let cache_key =
                (keep && j - i >= GROUP_CACHE_MIN).then(|| geometry ^ member.rotate_left(17));
            if let Some(hit) = cache_key.and_then(|k| self.group_take(k)) {
                self.stats.group_hits += 1;
                hit.mask.paint(cr, c.rgb(), qa / LINE_LEVELS);
                self.stats.lines += hit.lines;
                self.stats.line_px += hit.line_px;
                self.group_put(hit);
                tick(2, &mut self.stats);
                i = j;
                continue;
            }
            self.segments.clear();
            let (mut lines, mut line_px) = (0, 0.0);
            for x in i..j {
                let l = &scene.lines[(self.keys[x] & 0xffff_ffff) as usize];
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
                    lines += 1;
                    line_px += len as f64;
                }
            }
            self.stats.lines += lines;
            self.stats.line_px += line_px;
            tick(0, &mut self.stats);
            let mut group = match cache_key {
                Some(k) => self.group_new(k, lines, line_px),
                None => None,
            };
            let mask = match group.as_mut() {
                Some(g) => &mut g.mask,
                None => &mut self.mask,
            };
            if !mask.begin(cw, ch, dev as f64) {
                return;
            }
            {
                let Some(mut pixels) = mask.lines() else {
                    return;
                };
                if flat {
                    pixels.flat_segments(&self.segments, width, self.threads);
                } else {
                    pixels.segments(&self.segments, width, self.threads);
                }
            }
            tick(1, &mut self.stats);
            match group {
                Some(g) => {
                    g.mask.paint(cr, c.rgb(), qa / LINE_LEVELS);
                    self.group_put(g);
                }
                None => self.mask.composite(cr, c.rgb(), qa / LINE_LEVELS),
            }
            tick(2, &mut self.stats);
            i = j;
        }
    }

    /// Everything the rasterised coverage of a connection group depends on
    /// besides which lines it holds: canvas, device scale, width, projection.
    fn line_geometry_key(&self, w: usize, h: usize, dev: f32, scene: &Scene) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (w, h, dev.to_bits(), scene.disp.line.to_bits()).hash(&mut hash);
        for values in [&self.sx, &self.sy, &self.sr] {
            for value in values {
                value.to_bits().hash(&mut hash);
            }
        }
        hash.finish()
    }

    /// Takes the kept group with this key out of the cache.
    fn group_take(&mut self, key: u64) -> Option<Group> {
        let at = self.groups.iter().position(|g| g.key == key)?;
        Some(self.groups.swap_remove(at))
    }

    /// A group slot to rasterise into: a free one or the least recently used.
    fn group_new(&mut self, key: u64, lines: usize, line_px: f64) -> Option<Group> {
        let mut group = if self.groups.len() >= GROUP_CACHE_SLOTS {
            let oldest = (0..self.groups.len()).min_by_key(|&k| self.groups[k].used)?;
            self.groups.swap_remove(oldest)
        } else {
            Group {
                key,
                lines,
                line_px,
                used: 0,
                mask: LineMask::default(),
            }
        };
        group.key = key;
        group.lines = lines;
        group.line_px = line_px;
        Some(group)
    }

    fn group_put(&mut self, mut group: Group) {
        self.group_clock += 1;
        group.used = self.group_clock;
        self.groups.push(group);
    }

    /// Marker metrics, or `None` if no markers are drawn this frame.
    fn arrow_dims(&self, scene: &Scene, w: usize, h: usize, dev: f32) -> Option<ArrowDims> {
        let zoom = scene.cam.cam.zoom;
        let fade = arrow_opacity(zoom) as f32;
        // Dense scenes show no markers while moving; they return with the
        // settled frame.
        if !scene.style.show_arrow || fade <= 0.0 || (self.motion && self.dense) {
            return None;
        }
        let lw = scene.disp.line;
        let len = (arrow_length(lw) * 4.0 / 3.0) as f32 * dev;
        Some(ArrowDims {
            fade,
            wf: w as f32 + 20.0 * dev,
            hf: h as f32 + 20.0 * dev,
            lw,
            len,
            half: len / 2.0,
            notch: len * 0.75,
            gap: zoom as f32 * dev,
        })
    }

    /// Opacity and outline (tip, wing, notch, wing) of marker `k`, if drawn.
    fn arrow_shape(
        &self,
        scene: &Scene,
        k: usize,
        m: &ArrowDims,
        dev: f32,
    ) -> Option<(f32, [(f32, f32); 4])> {
        let (a, b) = scene.arrows[k];
        let (a, b) = (a as usize, b as usize);
        if self.sx[b] < -20.0 * dev
            || self.sx[b] > m.wf
            || self.sy[b] < -20.0 * dev
            || self.sy[b] > m.hf
        {
            return None;
        }
        let alpha = scene.arrow_alpha[k] * scene.palette.arrow.a * m.fade;
        if (alpha * LINE_LEVELS).round() < 1.0 {
            return None;
        }
        let (dx, dy) = (self.sx[b] - self.sx[a], self.sy[b] - self.sy[a]);
        let d = (dx * dx + dy * dy).sqrt();
        if d <= 0.0 || d - self.sr[a] - self.sr[b] <= m.lw as f32 * dev {
            return None;
        }
        if self.motion && d - self.sr[a] - self.sr[b] < m.len * MOTION_ARROW_SHAFTS {
            return None;
        }
        let (ux, uy) = (dx / d, dy / d);
        let back = self.sr[b] + m.gap;
        let (tx, ty) = (self.sx[b] - ux * back, self.sy[b] - uy * back);
        // local frame: x along the line toward the tip, y to its side
        let p = |lx: f32, ly: f32| (tx + ux * lx - uy * ly, ty + uy * lx + ux * ly);
        Some((
            alpha,
            [
                (tx, ty),
                p(-m.len, -m.half),
                p(-m.notch, 0.0),
                p(-m.len, m.half),
            ],
        ))
    }

    fn arrows(&mut self, canvas: &mut Canvas<'_>, scene: &Scene, dev: f32) {
        let Some(dims) = self.arrow_dims(scene, canvas.w, canvas.h, dev) else {
            return;
        };
        let rgb = scene.palette.arrow.rgb();
        for k in 0..scene.arrows.len() {
            let Some((alpha, [tip, wing_a, notch, wing_b])) =
                self.arrow_shape(scene, k, &dims, dev)
            else {
                continue;
            };
            if self.motion {
                canvas.dart_flat([tip, wing_a, notch, wing_b], rgb, alpha);
            } else {
                canvas.dart(tip, wing_a, notch, wing_b, rgb, alpha);
            }
            self.stats.arrows += 1;
        }
    }

    /// Markers while appearance eases (hover fades): coverage masks per
    /// opacity level, kept as long as the geometry and the members stay, so a
    /// frame only repaints them in the new opacity. Opacities are quantised to
    /// the same 1/40 steps as connections, which the settled frame then refines.
    fn arrows_masked(&mut self, cr: &cairo::Context, scene: &Scene, w: usize, h: usize, dev: f32) {
        let Some(dims) = self.arrow_dims(scene, w, h, dev) else {
            return;
        };
        let geometry = self.line_geometry_key(w, h, dev, scene)
            ^ u64::from(scene.cam.cam.zoom.to_bits() as u32).rotate_left(29)
            ^ ARROW_SALT;
        self.keys.clear();
        for k in 0..scene.arrows.len() {
            if let Some((alpha, _)) = self.arrow_shape(scene, k, &dims, dev) {
                let level = (alpha * LINE_LEVELS).round() as u64;
                self.keys.push(level << 32 | k as u64);
            }
        }
        self.keys.sort_unstable();
        let rgb = scene.palette.arrow.rgb();
        let mut i = 0;
        while i < self.keys.len() {
            let level = self.keys[i] >> 32;
            let mut j = i;
            let mut member = 0xcbf2_9ce4_8422_2325u64;
            while j < self.keys.len() && self.keys[j] >> 32 == level {
                let (a, b) = scene.arrows[(self.keys[j] & 0xffff_ffff) as usize];
                member =
                    (member ^ (u64::from(a) << 32 | u64::from(b))).wrapping_mul(0x0100_0000_01b3);
                j += 1;
            }
            let opacity = level as f32 / LINE_LEVELS;
            self.stats.arrows += j - i;
            let key = geometry ^ member.rotate_left(17);
            if let Some(hit) = self.group_take(key) {
                self.stats.group_hits += 1;
                hit.mask.paint(cr, rgb, opacity);
                self.group_put(hit);
                i = j;
                continue;
            }
            self.shapes.clear();
            for x in i..j {
                let k = (self.keys[x] & 0xffff_ffff) as usize;
                if let Some((_, shape)) = self.arrow_shape(scene, k, &dims, dev) {
                    self.shapes.push(shape);
                }
            }
            let mut group = self.group_new(key, j - i, 0.0);
            let mask = match group.as_mut() {
                Some(g) => &mut g.mask,
                None => &mut self.mask,
            };
            if mask.begin(w, h, dev as f64) {
                if let Some(mut pixels) = mask.lines() {
                    for [tip, wing_a, notch, wing_b] in &self.shapes {
                        pixels.dart(*tip, *wing_a, *notch, *wing_b);
                    }
                }
            }
            match group {
                Some(g) => {
                    g.mask.paint(cr, rgb, opacity);
                    self.group_put(g);
                }
                None => self.mask.composite(cr, rgb, opacity),
            }
            i = j;
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
            if self.motion {
                canvas.disc_flat(self.sx[i], self.sy[i], r, scene.color[i], alpha);
            } else {
                canvas.disc(self.sx[i], self.sy[i], r, scene.color[i], alpha);
            }
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
        let cols = ((w + 2.0 * CELL_MARGIN_X) / CELL_W).ceil() as usize + 1;
        let rows = ((h + CELL_MARGIN_TOP + CELL_MARGIN_BOTTOM) / CELL_H).ceil() as usize + 1;
        self.label_grid = (cols, rows);
        self.label_cells.clear();
        self.label_cells.resize(cols * rows, false);
        let cam = scene.cam.cam;
        self.label_origin = (cam.pan_x.rem_euclid(CELL_W), cam.pan_y.rem_euclid(CELL_H));
        let hl = scene.highlight().filter(|h| *h < scene.len());
        if label_opacity(cam.zoom, scene.disp.label_detail) > 0.001 {
            // Moving or settled, the same labels win in the same order: only
            // their bitmaps may still be missing.
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

    /// Marks the declutter cells a label covers; false if one is taken.
    fn claim_cells(&mut self, x: f64, y: f64, lw: f64, lh: f64) -> bool {
        let (cols, rows) = self.label_grid;
        let (ox, oy) = self.label_origin;
        let col = |v: f64| ((v - ox + CELL_MARGIN_X) / CELL_W).floor().max(0.0) as usize;
        let row = |v: f64| ((v - oy + CELL_MARGIN_TOP) / CELL_H).floor().max(0.0) as usize;
        let end_col = |v: f64| ((v - ox + CELL_MARGIN_X) / CELL_W).ceil().max(0.0) as usize;
        let end_row = |v: f64| ((v - oy + CELL_MARGIN_TOP) / CELL_H).ceil().max(0.0) as usize;
        let (x0, x1) = (col(x), end_col(x + lw).min(cols));
        let (y0, y1) = (row(y), end_row(y + lh).min(rows));
        if (y0..y1).any(|cy| (x0..x1).any(|cx| self.label_cells[cy * cols + cx])) {
            return false;
        }
        for cy in y0..y1 {
            for cx in x0..x1 {
                self.label_cells[cy * cols + cx] = true;
            }
        }
        true
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
        let rs = cam.zoom.sqrt();
        let metrics = label_metrics(scene, i);
        let alpha = metrics.alpha;
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
        let font = metrics.font_size;
        let on_screen = metrics.screen_font_size;
        if on_screen < MIN_LABEL_PX {
            return;
        }
        let drop = scene.drop[i] as f64;
        let text = scene.graph.nodes[i].label();
        let renders = self.labels.renders_this_frame;
        let Some((surface, px, width, height)) =
            self.labels.get(text, on_screen * dev, font, !self.motion)
        else {
            return;
        };
        let k = on_screen * dev / px / dev; // logical pixels per bitmap pixel
        let lw = width * k;
        let ly = sy + (world as f64 + 5.0) * rs + drop;
        let (x, y) = (sx - lw / 2.0, ly - LABEL_PAD as f64 * k);
        if self.dense && !hovered && !self.claim_cells(x, y, lw, height * k) {
            return;
        }
        if x + lw < 0.0 || x > w || y + height * k < 0.0 || y > h {
            return;
        }
        let _ = cr.save();
        let c = scene.palette.text;
        cr.set_source_rgba(c.r as f64, c.g as f64, c.b as f64, alpha);
        if (k - 1.0 / dev).abs() < 1e-3 {
            cr.translate((x * dev).round() / dev, (y * dev).round() / dev);
        } else {
            cr.translate(x, y);
        }
        if self.capture {
            self.label_log.push(LabelDrawn {
                node: i,
                x,
                y,
                w: lw,
                h: height * k,
                alpha,
                bitmap_px: px,
                stretched: (k - 1.0 / dev).abs() >= 1e-3,
            });
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

    /// A chain whose single connection group is large enough to be cached.
    fn chain_scene() -> Scene {
        let mut scene = Scene::new();
        scene.set_size(240.0, 180.0);
        let pairs: Vec<(u32, u32)> = (0..299).map(|i| (i, i + 1)).collect();
        scene.apply(Built::global(Graph::from_pairs(300, &pairs)));
        while scene.advance(1.0 / 60.0) {}
        let bounds = scene.layout.bounds().expect("bounds");
        let mut fit = crate::geometry::Camera::default();
        fit.fit(bounds, 240.0, 180.0, 10.0);
        scene.cam.cam = fit;
        scene.cam.target_zoom = fit.zoom;
        scene
    }

    #[test]
    fn moving_frames_never_replay_connections_of_another_camera() {
        let mut scene = chain_scene();
        let mut painter = Painter::new();
        painter.motion = true;
        let first = pixels(&mut painter, &scene, 1.0);
        let mut fresh = Painter::new();
        fresh.motion = true;
        assert!(first == pixels(&mut fresh, &scene, 1.0));
        for step in 1..6 {
            scene.cam.cam.pan_x += 9.0 * step as f64;
            scene.cam.cam.zoom *= 0.93;
            let moved = pixels(&mut painter, &scene, 1.0);
            let mut fresh = Painter::new();
            fresh.motion = true;
            assert!(
                moved == pixels(&mut fresh, &scene, 1.0),
                "step {step}: a moving frame differs from a fresh redraw"
            );
        }
        // the settled frame after the motion is exact as well
        painter.motion = false;
        assert_eq!(
            pixels(&mut painter, &scene, 1.0),
            pixels(&mut Painter::new(), &scene, 1.0)
        );
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
