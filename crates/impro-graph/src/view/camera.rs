//! Camera motion: eased zoom, drag panning with inertia, keyboard movement and
//! focus tracking. Pure maths on top of [`crate::geometry`].

use crate::geometry::{
    clamp_zoom, ease, zoom_about, Camera, PAN_DECAY, PAN_FILTER, ZOOM_EASE_K,
};

/// Zoom steps per 60 Hz frame while a zoom key is held (shift: faster).
const KEY_ZOOM: f64 = 1.03;
const KEY_ZOOM_FAST: f64 = 1.1;
/// Longest the camera follows a focused node.
const TRACK_SECONDS: f64 = 2.5;
/// Velocity (px/ms) below which inertia stops.
const STOP_SPEED: f64 = 0.002;
/// Without movement for this long (ms) a release does not fling.
const FLING_WINDOW_MS: f64 = 100.0;

/// Which movement keys are held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub zoom_in: bool,
    pub zoom_out: bool,
    pub fast: bool,
}

impl Keys {
    pub fn any(&self) -> bool {
        self.left || self.right || self.up || self.down || self.zoom_in || self.zoom_out
    }
}

#[derive(Clone, Copy, Debug)]
struct Track {
    node: usize,
    elapsed: f64,
}

#[derive(Clone, Debug)]
pub struct CameraState {
    pub cam: Camera,
    /// Zoom the camera eases toward.
    pub target_zoom: f64,
    /// Screen point the zoom keeps fixed.
    pub anchor: (f64, f64),
    /// Pan velocity in pixels per millisecond.
    pub vx: f64,
    pub vy: f64,
    pub keys: Keys,
    track: Option<Track>,
    /// Smoothed time between pointer moves of the current drag, in ms.
    drag_interval: f64,
}

impl Default for CameraState {
    fn default() -> Self {
        CameraState {
            cam: Camera::default(),
            target_zoom: 1.0,
            anchor: (0.0, 0.0),
            vx: 0.0,
            vy: 0.0,
            keys: Keys::default(),
            track: None,
            drag_interval: 16.0,
        }
    }
}

impl CameraState {
    /// Centre of the world at the middle of a `w × h` view, zoom 1.
    pub fn centred(w: f64, h: f64) -> Self {
        let mut c = CameraState::default();
        c.reset(w, h);
        c
    }

    pub fn reset(&mut self, w: f64, h: f64) {
        self.cam.reset(w, h);
        self.target_zoom = 1.0;
        self.anchor = (w / 2.0, h / 2.0);
        self.vx = 0.0;
        self.vy = 0.0;
        self.track = None;
    }

    /// Jumps to a zoom (no easing), keeping the view centre fixed.
    pub fn set_zoom_now(&mut self, zoom: f64, w: f64, h: f64) {
        let z = clamp_zoom(zoom);
        self.cam.pan_x = zoom_about(self.cam.pan_x, self.cam.zoom, z, w / 2.0);
        self.cam.pan_y = zoom_about(self.cam.pan_y, self.cam.zoom, z, h / 2.0);
        self.cam.zoom = z;
        self.target_zoom = z;
    }

    /// Eases toward a zoom about the view centre.
    pub fn set_zoom_target(&mut self, zoom: f64, w: f64, h: f64) {
        self.target_zoom = clamp_zoom(zoom);
        self.anchor = (w / 2.0, h / 2.0);
    }

    /// Keeps the view centre when the widget is resized.
    pub fn resized(&mut self, old: (f64, f64), new: (f64, f64)) {
        if old.0 > 0.0 && old.1 > 0.0 {
            self.cam.pan_x += (new.0 - old.0) / 2.0;
            self.cam.pan_y += (new.1 - old.1) / 2.0;
        } else {
            self.cam.pan_x = new.0 / 2.0;
            self.cam.pan_y = new.1 / 2.0;
        }
    }

    /// A wheel movement of `pixels` (positive zooms out) about `(x, y)`.
    pub fn wheel(&mut self, x: f64, y: f64, pixels: f64) {
        self.track = None;
        self.target_zoom = clamp_zoom(self.target_zoom * crate::geometry::wheel_factor(pixels));
        self.anchor = (x, y);
    }

    /// Eases to `zoom` about the pointer (used by pinch gestures and tests).
    pub fn zoom_by(&mut self, x: f64, y: f64, factor: f64) {
        self.track = None;
        self.target_zoom = clamp_zoom(self.target_zoom * factor);
        self.anchor = (x, y);
    }

    pub fn track_node(&mut self, node: usize) {
        if self.target_zoom < 0.5 {
            self.target_zoom = 1.0;
        }
        self.track = Some(Track { node, elapsed: 0.0 });
        self.vx = 0.0;
        self.vy = 0.0;
    }

    pub fn stop_tracking(&mut self) {
        self.track = None;
    }

    pub fn is_tracking(&self) -> bool {
        self.track.is_some()
    }

    /// A pan drag starts: inertia stops.
    pub fn begin_pan(&mut self) {
        self.track = None;
        self.vx = 0.0;
        self.vy = 0.0;
        self.drag_interval = 16.0;
    }

    /// Moves the pan to `(px, py)` during a drag; `elapsed_ms` since the last move.
    pub fn drag_pan(&mut self, px: f64, py: f64, elapsed_ms: f64) {
        let (dx, dy) = (px - self.cam.pan_x, py - self.cam.pan_y);
        self.vx = self.vx * (1.0 - PAN_FILTER) + dx * PAN_FILTER;
        self.vy = self.vy * (1.0 - PAN_FILTER) + dy * PAN_FILTER;
        self.drag_interval = self.drag_interval * 0.8 + elapsed_ms.max(1.0) * 0.2;
        self.cam.pan_x = px;
        self.cam.pan_y = py;
    }

    /// The drag ends `idle_ms` after the last move: fling unless idle.
    pub fn end_pan(&mut self, idle_ms: f64, reduce_motion: bool) {
        if reduce_motion || idle_ms > FLING_WINDOW_MS {
            self.vx = 0.0;
            self.vy = 0.0;
        } else {
            self.vx /= self.drag_interval;
            self.vy /= self.drag_interval;
        }
    }

    /// Advances by `dt` seconds. `node` supplies the world position of the
    /// tracked node, `hot` tells whether the layout still moves, `panning`
    /// whether the pointer holds the canvas. Returns true while moving.
    pub fn step(
        &mut self,
        dt: f64,
        reduce_motion: bool,
        size: (f64, f64),
        hot: bool,
        panning: bool,
        node: impl Fn(usize) -> Option<(f32, f32)>,
    ) -> bool {
        let mut moved = false;
        let keys = self.keys;
        let zk = (keys.zoom_in as i32 - keys.zoom_out as i32) as f64;
        if zk != 0.0 {
            let base = if keys.fast { KEY_ZOOM_FAST } else { KEY_ZOOM };
            self.target_zoom = clamp_zoom(self.target_zoom * base.powf(zk * dt * 60.0));
            self.anchor = (size.0 / 2.0, size.1 / 2.0);
        }
        self.target_zoom = clamp_zoom(self.target_zoom);

        // eased zoom
        let (s, ts) = (self.cam.zoom, self.target_zoom);
        if (s / ts - 1.0).abs() >= 0.01 {
            let s1 = ease(s, ts, ZOOM_EASE_K, dt, reduce_motion);
            self.cam.pan_x = zoom_about(self.cam.pan_x, s, s1, self.anchor.0);
            self.cam.pan_y = zoom_about(self.cam.pan_y, s, s1, self.anchor.1);
            self.cam.zoom = s1;
            moved = true;
        } else if s != ts {
            self.cam.pan_x = zoom_about(self.cam.pan_x, s, ts, self.anchor.0);
            self.cam.pan_y = zoom_about(self.cam.pan_y, s, ts, self.anchor.1);
            self.cam.zoom = ts;
            moved = true;
        }

        // follow a node
        if let Some(mut t) = self.track {
            match node(t.node) {
                Some((nx, ny)) => {
                    let z = self.cam.zoom;
                    let (tx, ty) = (size.0 / 2.0 - z * nx as f64, size.1 / 2.0 - z * ny as f64);
                    let px = ease(self.cam.pan_x, tx, ZOOM_EASE_K, dt, reduce_motion);
                    let py = ease(self.cam.pan_y, ty, ZOOM_EASE_K, dt, reduce_motion);
                    t.elapsed += dt;
                    if (px - tx).abs() < 0.5 && (py - ty).abs() < 0.5 && !hot {
                        self.cam.pan_x = tx;
                        self.cam.pan_y = ty;
                        self.track = None;
                    } else {
                        self.cam.pan_x = px;
                        self.cam.pan_y = py;
                        self.track = (t.elapsed <= TRACK_SECONDS).then_some(t);
                    }
                    moved = true;
                }
                None => self.track = None,
            }
        }

        // inertia and keyboard pan
        if !panning {
            let speed = if keys.fast { 3.0 } else { 1.0 };
            let kx = (keys.left as i32 - keys.right as i32) as f64 * speed;
            let ky = (keys.up as i32 - keys.down as i32) as f64 * speed;
            if reduce_motion && kx == 0.0 && ky == 0.0 {
                self.vx = 0.0;
                self.vy = 0.0;
            }
            if self.vx != 0.0 || self.vy != 0.0 || kx != 0.0 || ky != 0.0 {
                let ms = dt * 1000.0;
                if reduce_motion && (kx != 0.0 || ky != 0.0) {
                    self.vx = kx;
                    self.vy = ky;
                }
                self.cam.pan_x += self.vx * ms;
                self.cam.pan_y += self.vy * ms;
                let d = PAN_DECAY.powf(dt * 60.0);
                self.vx = self.vx * d + kx * (1.0 - d);
                self.vy = self.vy * d + ky * (1.0 - d);
                if kx == 0.0 && ky == 0.0 && self.vx.abs() < STOP_SPEED && self.vy.abs() < STOP_SPEED
                {
                    self.vx = 0.0;
                    self.vy = 0.0;
                }
                moved = true;
            }
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (f64, f64) = (800.0, 600.0);

    fn run(c: &mut CameraState, frames: usize) -> usize {
        let mut n = 0;
        for _ in 0..frames {
            if !c.step(1.0 / 60.0, false, SIZE, false, false, |_| None) {
                break;
            }
            n += 1;
        }
        n
    }

    #[test]
    fn wheel_zoom_eases_to_target_and_keeps_the_pointer_fixed() {
        let mut c = CameraState::centred(SIZE.0, SIZE.1);
        let anchor = (200.0, 150.0);
        let world = c.cam.to_world(anchor.0, anchor.1);
        c.wheel(anchor.0, anchor.1, -480.0);
        assert!((c.target_zoom - 1.0f64.exp()).abs() < 1e-9);
        for _ in 0..200 {
            let moving = c.step(1.0 / 60.0, false, SIZE, false, false, |_| None);
            let s = c.cam.to_screen(world.0, world.1);
            assert!((s.0 - anchor.0).abs() < 1e-6 && (s.1 - anchor.1).abs() < 1e-6);
            if !moving {
                break;
            }
        }
        assert_eq!(c.cam.zoom, c.target_zoom);
    }

    #[test]
    fn zoom_is_clamped_to_the_limits() {
        let mut c = CameraState::centred(SIZE.0, SIZE.1);
        c.wheel(0.0, 0.0, -1e9);
        assert_eq!(c.target_zoom, crate::geometry::MAX_ZOOM);
        c.wheel(0.0, 0.0, 1e9);
        assert_eq!(c.target_zoom, crate::geometry::MIN_ZOOM);
    }

    #[test]
    fn reduced_motion_jumps_to_the_target() {
        let mut c = CameraState::centred(SIZE.0, SIZE.1);
        c.wheel(100.0, 100.0, -480.0);
        c.step(1.0 / 60.0, true, SIZE, false, false, |_| None);
        assert!((c.cam.zoom - c.target_zoom).abs() < 1e-12);
    }

    #[test]
    fn a_fling_decays_by_the_documented_factor() {
        let mut c = CameraState::centred(SIZE.0, SIZE.1);
        c.begin_pan();
        for i in 1..=10 {
            c.drag_pan(400.0 + 10.0 * i as f64, 300.0, 16.0);
        }
        c.end_pan(5.0, false);
        let v0 = c.vx;
        assert!(v0 > 0.0);
        c.step(1.0 / 60.0, false, SIZE, false, false, |_| None);
        assert!((c.vx / v0 - PAN_DECAY).abs() < 1e-9);
        let frames = run(&mut c, 2000);
        assert!(frames < 2000, "inertia must stop");
        assert_eq!((c.vx, c.vy), (0.0, 0.0));
    }

    #[test]
    fn a_stale_release_or_reduced_motion_does_not_fling() {
        let mut c = CameraState::centred(SIZE.0, SIZE.1);
        c.begin_pan();
        c.drag_pan(450.0, 300.0, 16.0);
        c.end_pan(250.0, false);
        assert_eq!((c.vx, c.vy), (0.0, 0.0));
        c.begin_pan();
        c.drag_pan(450.0, 300.0, 16.0);
        c.end_pan(10.0, true);
        assert_eq!((c.vx, c.vy), (0.0, 0.0));
    }

    #[test]
    fn keyboard_pans_and_zooms_about_the_centre() {
        let mut c = CameraState::centred(SIZE.0, SIZE.1);
        c.keys.left = true;
        c.step(1.0 / 60.0, false, SIZE, false, false, |_| None);
        c.step(1.0 / 60.0, false, SIZE, false, false, |_| None);
        assert!(c.cam.pan_x > SIZE.0 / 2.0);
        c.keys = Keys {
            zoom_in: true,
            ..Keys::default()
        };
        let before = c.target_zoom;
        c.step(1.0 / 60.0, false, SIZE, false, false, |_| None);
        assert!((c.target_zoom / before - KEY_ZOOM).abs() < 1e-9);
        assert_eq!(c.anchor, (SIZE.0 / 2.0, SIZE.1 / 2.0));
    }

    #[test]
    fn tracking_centres_the_node_and_gives_up_on_a_vanished_one() {
        let mut c = CameraState::centred(SIZE.0, SIZE.1);
        c.track_node(3);
        for _ in 0..600 {
            if !c.step(1.0 / 60.0, false, SIZE, false, false, |_| Some((100.0, -50.0))) {
                break;
            }
        }
        let s = c.cam.to_screen(100.0, -50.0);
        assert!((s.0 - 400.0).abs() < 0.6 && (s.1 - 300.0).abs() < 0.6);
        assert!(!c.is_tracking());
        c.track_node(9);
        c.step(1.0 / 60.0, false, SIZE, false, false, |_| None);
        assert!(!c.is_tracking());
    }

    #[test]
    fn resize_keeps_the_view_centre() {
        let mut c = CameraState::centred(800.0, 600.0);
        let world = c.cam.to_world(400.0, 300.0);
        c.resized((800.0, 600.0), (1000.0, 500.0));
        let s = c.cam.to_screen(world.0, world.1);
        assert_eq!(s, (500.0, 250.0));
        let mut fresh = CameraState::default();
        fresh.resized((0.0, 0.0), (400.0, 200.0));
        assert_eq!((fresh.cam.pan_x, fresh.cam.pan_y), (200.0, 100.0));
    }
}
