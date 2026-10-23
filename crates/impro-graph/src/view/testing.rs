//! Helpers for tests, examples and QA: inspect the scene and drive the canvas
//! without a pointer device. Not part of the host API.

use std::time::{Duration, Instant};

use gtk::prelude::*;

use super::widget::GraphView;

/// The state of the scene, read-only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneInfo {
    pub zoom: f64,
    pub target_zoom: f64,
    pub pan: (f64, f64),
    pub size: (f64, f64),
    pub nodes: usize,
    pub lines: usize,
    pub hover: Option<String>,
    pub dragging: bool,
    pub simulating: bool,
    pub alpha: f64,
    pub settled: bool,
    pub ticks: u64,
    /// Camera velocity in logical pixels per millisecond (per move while held).
    pub camera_velocity: (f64, f64),
}

pub fn info(view: &GraphView) -> SceneInfo {
    scene_info(&view.inner)
}

fn scene_info(inner: &super::widget::Inner) -> SceneInfo {
    let s = inner.scene.borrow();
    SceneInfo {
        zoom: s.cam.cam.zoom,
        target_zoom: s.cam.target_zoom,
        pan: (s.cam.cam.pan_x, s.cam.cam.pan_y),
        size: s.size,
        nodes: s.len(),
        lines: s.lines.len(),
        hover: s.hover.map(|i| s.graph.nodes[i].id.to_string()),
        dragging: s.is_dragging(),
        simulating: s.simulating(),
        alpha: s.layout.alpha(),
        settled: s.layout.settled(),
        ticks: s.layout.ticks(),
        camera_velocity: (s.cam.vx, s.cam.vy),
    }
}

/// Label inputs shared with the painter, before glyph-mask coverage.
#[derive(Clone, Debug)]
pub struct LabelPaint {
    pub font_size: f64,
    pub screen_font_size: f64,
    pub wrap_width: f64,
    pub alpha: f64,
    pub bitmap_font_size: Option<f64>,
    pub bitmap_size: Option<(f64, f64)>,
}

/// Connection appearance after the painter's opacity and tint bucketing.
#[derive(Clone, Debug)]
pub struct LinePaint {
    pub endpoints: [String; 2],
    pub width: f64,
    pub color: String,
    pub alpha: f32,
    pub emphasis: f32,
}

/// A direction marker accepted by the painter's visibility/length checks.
#[derive(Clone, Debug)]
pub struct ArrowPaint {
    pub endpoints: [String; 2],
    pub vertices: [(f32, f32); 4],
    pub alpha: f32,
}

/// Nominal label metrics and the cached bitmap actually used by the painter.
pub fn label_paint(view: &GraphView, id: &str) -> Option<LabelPaint> {
    let scene = view.inner.scene.borrow();
    let i = scene.graph.index_of(id)?;
    let mut paint = super::draw::label_metrics(&scene, i);
    if let Some((px, w, h)) = view
        .inner
        .painter
        .borrow()
        .labels
        .metrics(scene.graph.nodes[i].label())
    {
        paint.bitmap_font_size = Some(px);
        paint.bitmap_size = Some((w, h));
    }
    Some(paint)
}

/// Connection metrics, including connections outside the current viewport.
pub fn line_paint(view: &GraphView) -> Vec<LinePaint> {
    let scene = view.inner.scene.borrow();
    scene
        .lines
        .iter()
        .enumerate()
        .map(|(k, line)| {
            let emphasis = (scene.line_mix[k] * 10.0).round() / 10.0;
            LinePaint {
                endpoints: [
                    scene.graph.nodes[line.a as usize].id.to_string(),
                    scene.graph.nodes[line.b as usize].id.to_string(),
                ],
                width: scene.disp.line,
                color: scene
                    .palette
                    .line
                    .mix(scene.palette.line_highlight, emphasis)
                    .to_hex(),
                alpha: (scene.line_alpha[k] * scene.palette.line.a * 40.0).round() / 40.0,
                emphasis,
            }
        })
        .collect()
}

/// Direction geometry from the most recently painted frame, in logical pixels.
pub fn arrow_paint(view: &GraphView) -> Vec<ArrowPaint> {
    view.inner.painter.borrow().arrow_paint(
        &view.inner.scene.borrow(),
        view.canvas().scale_factor() as f32,
    )
}

/// The cursor selected by the normal input handler.
pub fn cursor_name(view: &GraphView) -> &'static str {
    view.inner.cursor_name()
}

/// Advances scene physics and easing by a controlled interval, without running
/// GTK, replay timers or settings writes. Returns whether another frame is needed.
pub fn advance_frame(view: &GraphView, seconds: f64) -> bool {
    let active = view
        .inner
        .scene
        .borrow_mut()
        .advance(seconds.clamp(0.0, 0.05));
    view.inner.canvas.queue_draw();
    active
}

/// Reveal-plan telemetry; each entry gives the item at which files first appear.
#[derive(Clone, Debug)]
pub struct ReplayPlanInfo {
    pub total: usize,
    pub rate: f64,
    pub reveals: Vec<(usize, Vec<String>)>,
}

/// The actual reveal plan, sampled without running the clock or changing data.
pub fn replay_plan(view: &GraphView) -> ReplayPlanInfo {
    let scene = view.inner.scene.borrow();
    let graph = &scene.graph;
    let plan = super::timelapse::Plan::new(graph, view.prefs().show_orphans);
    let mut seen = std::collections::BTreeSet::new();
    let mut reveals = Vec::new();
    for item in 1..=plan.total() {
        let subset = plan.subset(graph, item);
        let mut new: Vec<_> = subset
            .nodes
            .iter()
            .map(|n| n.id.to_string())
            .filter(|id| seen.insert(id.clone()))
            .collect();
        new.sort();
        if !new.is_empty() {
            reveals.push((item, new))
        }
    }
    ReplayPlanInfo {
        total: plan.total(),
        rate: crate::geometry::timeline_speed(plan.total()),
        reveals,
    }
}

/// Ids of the shown nodes, sorted.
pub fn node_ids(view: &GraphView) -> Vec<String> {
    view.inner
        .scene
        .borrow()
        .graph
        .nodes
        .iter()
        .map(|n| n.id.to_string())
        .collect()
}

/// Screen position and radius of a node.
pub fn node_screen(view: &GraphView, id: &str) -> Option<(f64, f64, f64)> {
    let s = view.inner.scene.borrow();
    s.graph.index_of(id).map(|i| s.screen(i))
}

/// World position of a node.
pub fn node_world(view: &GraphView, id: &str) -> Option<(f32, f32)> {
    let s = view.inner.scene.borrow();
    s.graph
        .index_of(id)
        .map(|i| (s.layout.x()[i], s.layout.y()[i]))
}

/// Current opacity and fill colour (`#rrggbb`) of a node.
pub fn node_look(view: &GraphView, id: &str) -> Option<(f32, String)> {
    let s = view.inner.scene.borrow();
    let i = s.graph.index_of(id)?;
    let c = s.color[i];
    Some((
        s.alpha[i],
        super::palette::Rgba::new(c[0], c[1], c[2], 1.0).to_hex(),
    ))
}

/// Radius (world units) of a node marker.
pub fn node_radius(view: &GraphView, id: &str) -> Option<f32> {
    let s = view.inner.scene.borrow();
    s.graph.index_of(id).map(|i| s.layout.radius()[i])
}

pub fn press(view: &GraphView, x: f64, y: f64, button: u32, state: gdk::ModifierType) {
    view.inner.on_press(x, y, button, state, 0);
}

pub fn release(view: &GraphView, x: f64, y: f64, button: u32, state: gdk::ModifierType) {
    view.inner.on_release(x, y, button, state, 0);
}

pub fn motion(view: &GraphView, x: f64, y: f64) {
    view.inner.on_motion(x, y, 0);
}

pub fn leave(view: &GraphView) {
    view.inner.on_leave(gdk::CrossingMode::Normal);
}

/// Like [`press`], with the event time in milliseconds.
pub fn press_at(
    view: &GraphView,
    x: f64,
    y: f64,
    button: u32,
    state: gdk::ModifierType,
    time: u32,
) {
    view.inner.on_press(x, y, button, state, time);
}

/// Like [`motion`], with the event time in milliseconds.
pub fn motion_at(view: &GraphView, x: f64, y: f64, time: u32) {
    view.inner.on_motion(x, y, time);
}

/// Like [`release`], with the event time in milliseconds.
pub fn release_at(
    view: &GraphView,
    x: f64,
    y: f64,
    button: u32,
    state: gdk::ModifierType,
    time: u32,
) {
    view.inner.on_release(x, y, button, state, time);
}

/// Runs the main loop until the canvas frame clock has produced `n` more
/// frames; false if that took longer than `timeout_ms` (or the canvas is not
/// mapped).
pub fn wait_frames(view: &GraphView, n: u32, timeout_ms: u64) -> bool {
    let canvas = view.canvas();
    if !canvas.is_mapped() {
        return false;
    }
    let seen = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let count = seen.clone();
    canvas.add_tick_callback(move |_, _| {
        count.set(count.get() + 1);
        if count.get() >= n {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    wait_for(timeout_ms, || seen.get() >= n)
}

/// Restarts the layout simulation as a structural edit would.
pub fn reheat(view: &GraphView) {
    view.inner.scene.borrow_mut().layout.restart();
    view.inner.kick();
}

/// Number of node centres inside the canvas.
pub fn nodes_on_screen(view: &GraphView) -> usize {
    let s = view.inner.scene.borrow();
    let (w, h) = s.size;
    (0..s.len())
        .map(|i| s.screen(i))
        .filter(|(x, y, _)| *x >= 0.0 && *x <= w && *y >= 0.0 && *y <= h)
        .count()
}

/// A press and release without movement.
pub fn click(view: &GraphView, x: f64, y: f64, button: u32, state: gdk::ModifierType) {
    press(view, x, y, button, state);
    release(view, x, y, button, state);
}

/// A wheel movement in pixels (positive zooms out).
pub fn wheel(view: &GraphView, x: f64, y: f64, pixels: f64, ctrl: bool) {
    view.inner.wheel(x, y, pixels, ctrl);
}

pub fn key(view: &GraphView, key: gdk::keys::Key, down: bool) {
    view.inner.on_key(key, gdk::ModifierType::empty(), down);
}

/// The open context menu, if any.
pub fn context_menu(view: &GraphView) -> Option<gtk::Menu> {
    view.inner.menu.borrow().clone()
}

/// Opens the context menu of a node as a right click would.
pub fn open_context_menu(view: &GraphView, id: &str) -> bool {
    let Some((x, y, _)) = node_screen(view, id) else {
        return false;
    };
    press(view, x, y, 3, gdk::ModifierType::empty());
    context_menu(view).is_some()
}

/// Runs the main loop for `ms` milliseconds.
pub fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        // Frames that take longer than their interval keep events pending
        // forever: the deadline is checked between events, too.
        while gtk::events_pending() && Instant::now() < end {
            gtk::main_iteration_do(false);
        }
        if Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Pumps until `done()` holds or `timeout_ms` passes; true if it held.
pub fn wait_for(timeout_ms: u64, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
            if done() {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
        }
        if done() {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Waits for the first graph, then for the layout and easing to come to rest.
pub fn settle(view: &GraphView, timeout_ms: u64) -> bool {
    wait_for(timeout_ms, || {
        view.inner.has_data.get()
            && !view.inner.scene.borrow().needs_frames()
            && !view.inner.painter.borrow().needs_frames()
            && view.inner.replay.borrow().is_none()
            && view.inner.tick.borrow().is_none()
    })
}

/// Fast-forwards the layout and all easing to rest without waiting for frames.
pub fn settle_now(view: &GraphView) {
    {
        let mut scene = view.inner.scene.borrow_mut();
        scene.layout.settle(100_000);
        for _ in 0..5_000 {
            if !scene.advance(1.0 / 60.0) {
                break;
            }
        }
    }
    view.inner.canvas.queue_draw();
}

/// Paints the view (panel included) into a PNG.
pub fn save_png(view: &GraphView, path: &std::path::Path) -> Result<(), String> {
    let (w, h) = (
        view.allocated_width().max(1),
        view.allocated_height().max(1),
    );
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).map_err(|e| e.to_string())?;
    {
        let cr = cairo::Context::new(&surface).map_err(|e| e.to_string())?;
        view.draw(&cr);
    }
    let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    surface.write_to_png(&mut file).map_err(|e| e.to_string())
}

/// The accent colour of the current palette as `#rrggbb`.
pub fn accent_hex(view: &GraphView) -> String {
    view.inner.scene.borrow().palette.highlight.to_hex()
}

/// The canvas background colour of the current palette as `#rrggbb`.
pub fn background_hex(view: &GraphView) -> String {
    view.inner.scene.borrow().palette.background.to_hex()
}

/// Whether the current palette is a dark one.
pub fn palette_dark(view: &GraphView) -> bool {
    view.inner.scene.borrow().palette.dark
}

/// Whether the camera, layout or easing still need frames.
pub fn needs_frames(view: &GraphView) -> bool {
    view.inner.scene.borrow().needs_frames()
}

/// Why the scene still wants frames (empty when at rest).
pub fn frame_reasons(view: &GraphView) -> Vec<&'static str> {
    view.inner.scene.borrow().frame_reasons()
}

/// Number of cached label bitmaps.
pub fn label_cache_len(view: &GraphView) -> usize {
    view.inner.painter.borrow().labels.len()
}

/// A frame the canvas has just painted, for [`set_probe`] callbacks.
pub struct FrameShot<'a> {
    inner: &'a super::widget::Inner,
}

/// How a painted frame differs from a fresh redraw of the same scene.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameCheck {
    /// The frame was drawn in the cheaper moving level of detail.
    pub motion: bool,
    /// Pixels (before labels) that differ from the fresh redraw.
    pub differing: usize,
    /// Bounding box `[x0, y0, x1, y1]` of the differing pixels, device pixels.
    pub bbox: Option<[i32; 4]>,
    /// The connection layer was reused from an earlier frame.
    pub reused: bool,
    /// Bytes that differ between two fresh redraws of the same scene.
    pub noise: usize,
}

/// What one frame put on screen, independent of how it was rasterised: the
/// assertions on it (markers, labels, controls, camera) hold for any renderer
/// that can report the same facts.
#[derive(Clone, Debug)]
pub struct FrameFacts {
    pub info: SceneInfo,
    /// Screen centre and radius of every marker, logical pixels.
    pub markers: Vec<(f64, f64, f64)>,
    /// Every label drawn (needs [`capture_frames`]), logical pixels.
    pub labels: Vec<(String, super::draw::LabelDrawn)>,
    pub controls: ControlsGeometry,
}

impl FrameShot<'_> {
    pub fn facts(&self) -> FrameFacts {
        FrameFacts {
            info: self.info(),
            markers: self.markers(),
            labels: self.labels(),
            controls: controls_of(self.inner),
        }
    }

    pub fn info(&self) -> SceneInfo {
        scene_info(self.inner)
    }

    pub fn stats(&self) -> super::draw::DrawStats {
        self.inner.painter.borrow().stats
    }

    /// Why the scene still wants frames.
    pub fn reasons(&self) -> Vec<&'static str> {
        self.inner.scene.borrow().frame_reasons()
    }

    /// Milliseconds the canvas spent painting this frame.
    pub fn draw_ms(&self) -> f64 {
        self.inner.stats.get().last_draw_ms
    }

    /// The labels this frame painted (needs [`capture_frames`]).
    pub fn labels(&self) -> Vec<(String, super::draw::LabelDrawn)> {
        let scene = self.inner.scene.borrow();
        self.inner
            .painter
            .borrow()
            .label_log
            .iter()
            .filter(|l| l.node < scene.len())
            .map(|l| (scene.graph.nodes[l.node].id.to_string(), l.clone()))
            .collect()
    }

    /// Screen centre and radius of every marker, in logical pixels, as the
    /// painter projected them for this frame.
    pub fn markers(&self) -> Vec<(f64, f64, f64)> {
        let scene = self.inner.scene.borrow();
        (0..scene.len()).map(|i| scene.screen(i)).collect()
    }

    /// True while the scene's geometry changes between frames.
    pub fn moving(&self) -> bool {
        self.inner.scene.borrow().moving()
    }

    /// Redraws the scene with a fresh painter (no caches, no history) and
    /// compares the result with what this frame painted, ignoring labels.
    /// `None` unless the view was told to capture frames ([`capture_frames`]).
    pub fn compare_fresh(&self) -> Option<FrameCheck> {
        let scene = self.inner.scene.borrow();
        let mut painter = self.inner.painter.borrow_mut();
        let shown = painter.unlabelled.take()?;
        let (w, h) = (
            self.inner.canvas.allocated_width() as f64,
            self.inner.canvas.allocated_height() as f64,
        );
        let mut fresh = super::draw::Painter::new();
        fresh.threads = painter.threads;
        fresh.motion = painter.motion;
        fresh.capture = true;
        fresh.render(
            &scene,
            w,
            h,
            super::draw::PaintOptions {
                hide_labels: true,
                scale: self.inner.canvas.scale_factor() as f64,
            },
        )?;
        let mut other = fresh.unlabelled.take()?;
        let mut again = super::draw::Painter::new();
        again.threads = painter.threads;
        again.motion = painter.motion;
        again.capture = true;
        again.render(
            &scene,
            w,
            h,
            super::draw::PaintOptions {
                hide_labels: true,
                scale: self.inner.canvas.scale_factor() as f64,
            },
        )?;
        let mut again = again.unlabelled.take()?;
        let noise = {
            let (a, b) = (other.data().ok()?, again.data().ok()?);
            a.iter().zip(b.iter()).filter(|(x, y)| x != y).count()
        };
        let mut shown = shown;
        if let Some(dir) = std::env::var_os("IMPRO_FRAME_DUMP") {
            static DUMPED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            if DUMPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 20 == 0 {
                let n = DUMPED.load(std::sync::atomic::Ordering::Relaxed);
                let dir = std::path::PathBuf::from(dir);
                let _ = std::fs::create_dir_all(&dir);
                for (name, surface) in [("shown", &shown), ("fresh", &other)] {
                    if let Ok(mut f) = std::fs::File::create(dir.join(format!("{n:04}-{name}.png")))
                    {
                        let _ = surface.write_to_png(&mut f);
                    }
                }
            }
        }
        let (width, height, stride) = (
            shown.width() as usize,
            shown.height() as usize,
            shown.stride() as usize,
        );
        let a = shown.data().ok()?;
        let b = other.data().ok()?;
        let mut check = FrameCheck {
            noise,
            motion: painter.motion,
            reused: painter.stats.connections_reused,
            ..FrameCheck::default()
        };
        for y in 0..height {
            for x in 0..width {
                let i = y * stride + x * 4;
                if a[i..i + 4] != b[i..i + 4] {
                    check.differing += 1;
                    let (x, y) = (x as i32, y as i32);
                    let bb = check.bbox.get_or_insert([x, y, x, y]);
                    *bb = [bb[0].min(x), bb[1].min(y), bb[2].max(x), bb[3].max(y)];
                }
            }
        }
        Some(check)
    }
}

/// Keeps a copy of every painted frame as it stood before its labels, so
/// probes can call [`FrameShot::compare_fresh`].
pub fn capture_frames(view: &GraphView, on: bool) {
    view.inner.painter.borrow_mut().capture = on;
}

/// Calls `f` after every frame the canvas paints (replaces an earlier probe).
pub fn set_probe(view: &GraphView, f: impl Fn(&FrameShot) + 'static) {
    *view.inner.probe.borrow_mut() = Some(std::rc::Rc::new(move |inner| f(&FrameShot { inner })));
}

/// Removes the probe.
pub fn clear_probe(view: &GraphView) {
    *view.inner.probe.borrow_mut() = None;
}

/// A widget rectangle in logical pixels relative to the view's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn centre(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }
}

/// Where the controls ended up.
#[derive(Clone, Debug, Default)]
pub struct ControlsGeometry {
    /// The pane the view fills.
    pub pane: Option<Rect>,
    /// The root of the controls (strip or panel).
    pub controls: Option<Rect>,
    /// The panel's revealer is shown (open, or fading out).
    pub revealer_shown: bool,
    /// The closed strip and its buttons (empty while the panel is open).
    pub chip: Option<Rect>,
    pub chip_buttons: Vec<Rect>,
    /// The open panel's frame and its header buttons.
    pub panel: Option<Rect>,
    pub head_buttons: Vec<Rect>,
}

fn rect_in(widget: &impl IsA<gtk::Widget>, parent: &gtk::Widget) -> Option<Rect> {
    let w: &gtk::Widget = widget.upcast_ref();
    if !w.is_visible() || !w.is_mapped() {
        return None;
    }
    let (x, y) = w.translate_coordinates(parent, 0, 0)?;
    let a = w.allocation();
    Some(Rect {
        x: x as f64,
        y: y as f64,
        w: a.width() as f64,
        h: a.height() as f64,
    })
}

/// Allocations of the controls strip, the panel and their buttons.
pub fn controls_geometry(view: &GraphView) -> ControlsGeometry {
    controls_of(&view.inner)
}

fn controls_of(inner: &super::widget::Inner) -> ControlsGeometry {
    let root: gtk::Widget = inner.root_box.clone().upcast();
    let panel = &inner.panel;
    let mut g = ControlsGeometry {
        pane: rect_in(&inner.root_box, &root),
        controls: rect_in(panel.controls_root(), &root),
        revealer_shown: panel.revealer_shown(),
        ..ControlsGeometry::default()
    };
    if panel.chip_visible() {
        g.chip = rect_in(panel.chip(), &root);
        g.chip_buttons = panel
            .chip_buttons()
            .iter()
            .filter_map(|b| rect_in(b, &root))
            .collect();
    }
    if panel.is_open() {
        g.panel = rect_in(panel.controls_root(), &root);
        g.head_buttons = [panel.close_button(), panel.reset_button()]
            .iter()
            .filter_map(|b| rect_in(*b, &root))
            .collect();
    }
    g
}

/// The view painted as it appears on screen (panel and strip included), as
/// premultiplied ARGB32 rows: `(width, height, stride, bytes)` in device pixels.
pub fn snapshot(view: &GraphView) -> Option<(i32, i32, i32, Vec<u8>)> {
    let scale = view.scale_factor();
    let (w, h) = (
        view.allocated_width().max(1) * scale,
        view.allocated_height().max(1) * scale,
    );
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w, h).ok()?;
    surface.set_device_scale(scale as f64, scale as f64);
    {
        let cr = cairo::Context::new(&surface).ok()?;
        view.draw(&cr);
    }
    surface.flush();
    let stride = surface.stride();
    let data = surface.data().ok()?.to_vec();
    Some((w, h, stride, data))
}

// ---- a project shaped like a real large one ----------------------------------------

/// What the generator produces.
#[derive(Clone, Copy, Debug)]
pub struct BigProject {
    /// Files in total (the view's cap).
    pub total: usize,
    /// Linked clusters of three to ten files (stars, rings, chains, cliques).
    pub clusters: usize,
    /// Files of the hub, each linking to the next `hub_reach` ones.
    pub hub: usize,
    pub hub_reach: usize,
    pub seed: u64,
}

impl Default for BigProject {
    fn default() -> Self {
        BigProject {
            total: 3_000,
            clusters: 80,
            hub: 50,
            hub_reach: 24,
            seed: 20_261_023,
        }
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next() as usize) % (hi - lo + 1)
    }
}

fn note(path: &str, links: &[String]) -> (String, String) {
    let mut text = String::from("# note\n");
    for link in links {
        text.push_str(&format!("[[{link}]] "));
    }
    text.push('\n');
    (path.to_string(), text)
}

impl BigProject {
    /// Relative path and text of every file.
    pub fn files(&self) -> Vec<(String, String)> {
        let mut rng = Lcg(self.seed);
        let mut out: Vec<(String, String)> = Vec::new();
        let name = |dir: &str, i: usize| format!("{dir}-{i:04}");
        // the hub
        for i in 0..self.hub {
            let links: Vec<String> = (1..=self.hub_reach)
                .map(|k| name("hub", (i + k) % self.hub))
                .collect();
            out.push(note(&format!("hub/{}.md", name("hub", i)), &links));
        }
        // clusters
        for c in 0..self.clusters {
            let size = rng.range(3, 10);
            let id = |i: usize| name(&format!("cl{c:02}"), i);
            let links = |i: usize| -> Vec<String> {
                match c % 4 {
                    // star around file 0
                    0 => {
                        if i == 0 {
                            (1..size).map(id).collect()
                        } else {
                            Vec::new()
                        }
                    }
                    // ring
                    1 => vec![id((i + 1) % size)],
                    // chain
                    2 => {
                        if i + 1 < size {
                            vec![id(i + 1)]
                        } else {
                            Vec::new()
                        }
                    }
                    // clique
                    _ => (i + 1..size).map(id).collect(),
                }
            };
            for i in 0..size {
                out.push(note(&format!("cluster/{c:02}/{}.md", id(i)), &links(i)));
            }
        }
        // the unlinked disc fills the rest up to the cap
        let rest = self.total.saturating_sub(out.len());
        for i in 0..rest {
            out.push(note(
                &format!("loose/{:02}/{}.md", i % 40, name("loose", i)),
                &[],
            ));
        }
        out
    }

    /// Writes the files below `root`.
    pub fn write(&self, root: &std::path::Path) {
        for (rel, text) in self.files() {
            let path = root.join(&rel);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, text);
        }
    }
}
