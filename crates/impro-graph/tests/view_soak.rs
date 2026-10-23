//! Seeded random soak of the real widget at full-screen size: pans, flings,
//! wheel zooms, hovers, clicks, panel toggles, resizes, camera resets and
//! filter changes, with real frames in between. After EVERY painted frame:
//! connections and markers come from one camera (the frame equals a fresh
//! redraw), the camera is finite and in range, the frame took a sane time, and
//! labels hold still while the camera only translates. After every action the
//! controls sit in the corner and the view settles without a leaked tick.
//!
//! `IMPRO_SOAK_SECONDS` (default 20) sets the length, `IMPRO_SOAK_SEED` the seed.

mod gtk_common;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use serde_json::json;

use gtk_common::drive::*;
use gtk_common::*;
use impro_graph::geometry::{MAX_ZOOM, MIN_ZOOM};
use impro_graph::view::draw::LabelDrawn;
use impro_graph::view::testing::{self as t, ControlsGeometry, FrameFacts};

/// Slower than any frame the soak should see; debug builds get more room.
const DRAW_BOUND_MS: f64 = if cfg!(debug_assertions) {
    4_000.0
} else {
    150.0
};
const MARGIN: f64 = 12.0;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

struct Prev {
    segment: u64,
    pan: (f64, f64),
    zoom: f64,
    labels: HashMap<String, LabelDrawn>,
}

#[derive(Default)]
struct Soak {
    /// Frames in which only the camera translated share a segment id.
    segment: u64,
    translating: bool,
    prev: Option<Prev>,
    frames: u64,
    worst_ms: f64,
    violations: Vec<String>,
}

impl Soak {
    fn fail(&mut self, what: String) {
        if self.violations.len() < 40 {
            self.violations
                .push(format!("frame {}: {what}", self.frames));
        }
    }

    fn frame(&mut self, shot: &t::FrameShot) {
        self.frames += 1;
        let facts = shot.facts();
        let i = &facts.info;
        if !(i.zoom.is_finite() && i.pan.0.is_finite() && i.pan.1.is_finite()) {
            self.fail(format!("camera not finite: {i:?}"));
        }
        if i.zoom < MIN_ZOOM * 0.999 || i.zoom > MAX_ZOOM * 1.001 || i.pan.0.abs() > 1e9 {
            self.fail(format!(
                "camera out of range: zoom {} pan {:?}",
                i.zoom, i.pan
            ));
        }
        let ms = shot.draw_ms();
        self.worst_ms = self.worst_ms.max(ms);
        if ms > DRAW_BOUND_MS {
            self.fail(format!("frame took {ms:.1} ms"));
        }
        if let Some(c) = shot.compare_fresh() {
            if c.differing > 0 {
                self.fail(format!(
                    "frame differs from a fresh redraw in {} px {:?} (motion {}, reused {})",
                    c.differing, c.bbox, c.motion, c.reused
                ));
            }
        }
        // easing of label detail, sizes or colours is the documented fade
        let eased = shot
            .reasons()
            .iter()
            .any(|r| !matches!(*r, "velocity" | "tracking" | "keys"));
        self.labels(&facts, !eased);
    }

    /// A label fully on screen in one frame must be there in the next, at the
    /// place the camera's translation puts it.
    fn labels(&mut self, facts: &FrameFacts, steady: bool) {
        let i = &facts.info;
        let labels: HashMap<String, LabelDrawn> = facts.labels.iter().cloned().collect();
        let quiet = steady && i.hover.is_none() && !i.simulating && !i.dragging;
        let segment = if self.translating && quiet {
            self.segment
        } else {
            u64::MAX
        };
        if let Some(prev) = self.prev.take() {
            if prev.segment == segment && segment != u64::MAX && prev.zoom == i.zoom {
                let (dx, dy) = (i.pan.0 - prev.pan.0, i.pan.1 - prev.pan.1);
                let (w, h) = i.size;
                let mut bad = Vec::new();
                for (id, a) in &prev.labels {
                    let inside = |x: f64, y: f64| {
                        x >= 1.0 && y >= 1.0 && x + a.w <= w - 1.0 && y + a.h <= h - 1.0
                    };
                    if a.alpha < 0.05 || !inside(a.x, a.y) || !inside(a.x + dx, a.y + dy) {
                        continue;
                    }
                    match labels.get(id) {
                        None => bad.push(format!("{id} vanished")),
                        Some(b) => {
                            // a bitmap refreshed at its exact size is a little
                            // wider: the label stays centred under its marker
                            let mx = (b.x + b.w / 2.0) - (a.x + a.w / 2.0) - dx;
                            let my = b.y - a.y - dy;
                            if mx.abs() > 1.0 || my.abs() > 1.0 {
                                bad.push(format!(
                                    "{id} moved off the camera by ({mx:.2}, {my:.2}): {a:?} -> {b:?}"
                                ));
                            }
                            if (b.alpha - a.alpha).abs() > 0.02 {
                                bad.push(format!("{id} alpha {} -> {}", a.alpha, b.alpha));
                            }
                        }
                    }
                }
                if !bad.is_empty() {
                    self.fail(format!(
                        "label flicker: {}",
                        bad[..bad.len().min(4)].join(", ")
                    ));
                }
            }
        }
        self.prev = Some(Prev {
            segment,
            pan: i.pan,
            zoom: i.zoom,
            labels,
        });
    }
}

fn controls_problem(g: &ControlsGeometry) -> Option<String> {
    let pane = g.pane?;
    let near = |a: f64, b: f64| (a - b).abs() <= 0.75;
    if let Some(panel) = g.panel {
        if !near(pane.right() - panel.right(), MARGIN) || !near(panel.y - pane.y, MARGIN) {
            return Some(format!("open panel not in the corner: {g:?}"));
        }
        return None;
    }
    let Some(chip) = g.chip else {
        return Some(format!("neither strip nor panel shown: {g:?}"));
    };
    if !near(pane.right() - chip.right(), MARGIN) || !near(chip.y - pane.y, MARGIN) {
        return Some(format!("strip not in the corner: {g:?}"));
    }
    let axes: Vec<f64> = g.chip_buttons.iter().map(|b| b.centre().0).collect();
    if axes.len() != 3 || axes.iter().any(|a| !near(*a, axes[0])) {
        return Some(format!("icons off one axis: {axes:?}"));
    }
    let rows: Vec<f64> = g.chip_buttons.iter().map(|b| b.centre().1).collect();
    if !near(rows[1] - rows[0], rows[2] - rows[1]) {
        return Some(format!("icon pitch uneven: {rows:?}"));
    }
    None
}

fn logical(w: i32, h: i32) -> (i32, i32) {
    let scale: i32 = std::env::var("GDK_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    (w / scale, h / scale)
}

fn env_num(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[test]
fn random_use_of_the_full_screen_view_stays_correct() {
    run(|| {
        let seconds = env_num("IMPRO_SOAK_SECONDS", 20);
        let seed = env_num("IMPRO_SOAK_SEED", 0x5eed_1234);
        let mut rng = Rng(seed | 1);
        let (w, h) = logical(1920, 1080);
        let f = Fixture::big((w, h));
        t::capture_frames(&f.view, true);
        let soak = Rc::new(RefCell::new(Soak::default()));
        {
            let soak = soak.clone();
            t::set_probe(&f.view, move |shot| soak.borrow_mut().frame(shot));
        }
        let end = Instant::now() + Duration::from_secs(seconds);
        let mut actions = 0u64;
        let mut log: Vec<String> = Vec::new();
        while Instant::now() < end {
            actions += 1;
            let (vw, vh) = t::info(&f.view).size;
            let kind = rng.below(12);
            let mut settle = true;
            let mut translating = false;
            let name;
            match kind {
                0..=2 => {
                    name = "pan";
                    translating = true;
                    soak.borrow_mut().segment += 1;
                    soak.borrow_mut().translating = true;
                    let mut p = Pointer::new(&f.view, empty_spot(&f, 6.0));
                    p.frame();
                    p.press();
                    let speed = rng.range(1.0, 30.0);
                    let angle = rng.range(0.0, std::f64::consts::TAU);
                    let step = (speed * angle.cos(), speed * angle.sin());
                    p.drag(step, 10 + rng.below(50) as u32);
                    if rng.below(2) == 0 {
                        p.release();
                        // the fling coasts on its own frames
                        for _ in 0..rng.below(40) {
                            p.frame();
                        }
                    } else {
                        p.release();
                    }
                }
                3..=4 => {
                    name = "wheel";
                    let at = (rng.range(0.0, vw), rng.range(0.0, vh));
                    let dir = if rng.below(2) == 0 { 60.0 } else { -60.0 };
                    wheel_frames(&f.view, at, dir, 3 + rng.below(25) as u32);
                }
                5 => {
                    name = "hover";
                    let mut p = Pointer::new(&f.view, (vw / 2.0, vh / 2.0));
                    for _ in 0..rng.below(40) + 5 {
                        p.frame();
                        p.move_to((rng.range(0.0, vw), rng.range(0.0, vh)), false);
                    }
                }
                6 => {
                    name = "click";
                    let at = (rng.range(0.0, vw), rng.range(0.0, vh));
                    t::wait_frames(&f.view, 1, 5_000);
                    t::click(&f.view, at.0, at.1, 1, none());
                }
                7 => {
                    name = "panel";
                    f.view.set_panel_open(!f.view.panel_open());
                }
                8 => {
                    name = "resize";
                    let size = logical(
                        rng.range(1100.0, 1920.0) as i32,
                        rng.range(700.0, 1080.0) as i32,
                    );
                    f.window.resize(size.0, size.1);
                    t::pump(200);
                }
                9 => {
                    name = "reset-camera";
                    if rng.below(2) == 0 {
                        f.view.reset_camera();
                    } else {
                        f.view.fit_camera();
                    }
                }
                10 => {
                    name = "filter";
                    let search = ["", "cl1", "loose/0", "hub", "cl4 cl5"][rng.below(5) as usize];
                    f.view.apply_prefs(&json!({ "search": search }));
                    // the rebuild lands from a worker thread
                    t::pump(800);
                }
                _ => {
                    name = "full-size";
                    f.window.resize(w, h);
                    t::pump(200);
                }
            }
            log.push(name.to_string());
            if !translating {
                soak.borrow_mut().translating = false;
            }
            if settle {
                settle = t::settle(&f.view, 20_000);
                assert!(
                    settle,
                    "never settled after {name} (reasons {:?}); actions: {}",
                    t::frame_reasons(&f.view),
                    log.join(",")
                );
            }
            soak.borrow_mut().translating = false;
            t::pump(250);
            if let Some(problem) = controls_problem(&t::controls_geometry(&f.view)) {
                panic!(
                    "after {name} (action {actions}, seed {seed}): {problem}\nactions: {}",
                    log.join(",")
                );
            }
            if matches!(name, "reset-camera") && f.view.summary().nodes > 0 {
                let on_screen = t::nodes_on_screen(&f.view);
                assert!(on_screen > 0, "blank canvas after {name} (seed {seed})");
            }
            assert!(
                !t::needs_frames(&f.view) && !f.view.frame_stats().animating,
                "a frame callback outlived the settle after {name}: reasons {:?}, animating {}",
                t::frame_reasons(&f.view),
                f.view.frame_stats().animating
            );
            let s = soak.borrow();
            assert!(
                s.violations.is_empty(),
                "after {name} (action {actions}, seed {seed}, {} frames):\n{}\nactions: {}",
                s.frames,
                s.violations.join("\n"),
                log.join(",")
            );
        }
        t::clear_probe(&f.view);
        let s = soak.borrow();
        eprintln!(
            "soak: {} actions, {} frames, worst draw {:.1} ms, seed {seed}",
            actions, s.frames, s.worst_ms
        );
        assert!(s.frames > 50, "the soak painted only {} frames", s.frames);
    });
}
