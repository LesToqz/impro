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
}

pub fn info(view: &GraphView) -> SceneInfo {
    let s = view.inner.scene.borrow();
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
        while gtk::events_pending() {
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

/// Number of cached label bitmaps.
pub fn label_cache_len(view: &GraphView) -> usize {
    view.inner.painter.borrow().labels.len()
}
