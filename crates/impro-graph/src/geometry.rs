//! Pure functions the renderer needs: marker sizes, label and arrow fading,
//! wheel zoom, pointer-anchored zoom, easing and hit testing.

pub const MIN_ZOOM: f64 = 0.04;
pub const MAX_ZOOM: f64 = 6.0;
/// Wrap width of labels in world units.
pub const LABEL_WRAP: f32 = 224.0;
/// Gap between a marker and its label.
pub const LABEL_OFFSET: f32 = 12.0;
/// Opacity of everything not related to the hovered node.
pub const HOVER_DIM: f32 = 0.28;
/// A click may move this many pixels.
pub const CLICK_SLOP: f32 = 6.0;
/// Fraction of the remaining distance covered per 60 Hz frame by colour and
/// alpha easing, and by zoom easing.
pub const EASE_K: f64 = 0.16;
pub const ZOOM_EASE_K: f64 = 0.18;
/// Pan velocity filter: share of new input per frame, and retention per frame.
pub const PAN_FILTER: f64 = 0.2;
pub const PAN_DECAY: f64 = 0.86;
/// Longest elapsed time one easing step may account for.
pub const MAX_FRAME_SECONDS: f64 = 0.05;

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// World radius of a marker: `multiplier · min(24, 6 + 2·log2(1 + degree))`.
/// Negative degrees count as zero.
pub fn marker_radius(degree: f32, multiplier: f32) -> f32 {
    multiplier * (6.0 + 2.0 * (1.0 + degree.max(0.0)).log2()).min(24.0)
}

/// Marker radius on screen: shrinks with the square root of the zoom.
pub fn screen_radius(radius: f32, zoom: f32) -> f32 {
    radius * zoom.max(0.0).sqrt()
}

/// Label opacity: `clamp((zoom − 0.12)/0.7 − detail·0.25, 0, 1)`. A larger
/// "label detail" value hides labels until the view is closer.
pub fn label_opacity(zoom: f64, detail: f64) -> f64 {
    clamp((zoom - 0.12) / 0.7 - detail * 0.25, 0.0, 1.0)
}

/// Direction marker opacity: `clamp((zoom − 0.18)/0.62, 0, 1)`.
pub fn arrow_opacity(zoom: f64) -> f64 {
    clamp((zoom - 0.18) / 0.62, 0.0, 1.0)
}

/// Direction marker base dimension in pixels: `6·sqrt(width multiplier)`.
/// The painter uses this as notch depth; tip-to-back length is four thirds of it.
pub fn arrow_length(line_multiplier: f64) -> f64 {
    6.0 * line_multiplier.max(0.0).sqrt()
}

/// Line width in world units that stays constant on screen.
pub fn line_world_width(line_multiplier: f64, zoom: f64) -> f64 {
    line_multiplier / zoom
}

/// Label font size: `12 + radius · 0.2`.
pub fn label_font_size(radius: f32) -> f32 {
    12.0 + radius * 0.2
}

/// Marker weight in the local graph: `12 / (layer + 1)`.
pub fn local_weight(layer: u32) -> f32 {
    12.0 / (layer as f32 + 1.0)
}

/// Items per second of the history replay: `clamp(total/12, 8, 72)`.
pub fn timeline_speed(total: usize) -> f64 {
    clamp(total as f64 / 12.0, 8.0, 72.0)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WheelUnit {
    Pixel,
    Line,
    Page,
}

/// Wheel delta in pixels: lines count 24, pages 640, and small control-wheel
/// (pinch) deltas double.
pub fn wheel_pixels(delta: f64, unit: WheelUnit, control: bool) -> f64 {
    let mut px = match unit {
        WheelUnit::Pixel => delta,
        WheelUnit::Line => delta * 24.0,
        WheelUnit::Page => delta * 640.0,
    };
    if control && unit == WheelUnit::Pixel && px.abs() < 32.0 {
        px *= 2.0;
    }
    px
}

/// Zoom factor of a wheel movement: `exp(−pixels/480)`.
pub fn wheel_factor(pixels: f64) -> f64 {
    (-pixels / 480.0).exp()
}

pub fn clamp_zoom(zoom: f64) -> f64 {
    if zoom.is_nan() {
        return 1.0;
    }
    clamp(zoom, MIN_ZOOM, MAX_ZOOM)
}

/// New pan (one axis) so the world point under `anchor` stays fixed when the
/// zoom changes from `old_zoom` to `new_zoom`. Screen = world·zoom + pan.
pub fn zoom_about(pan: f64, old_zoom: f64, new_zoom: f64, anchor: f64) -> f64 {
    anchor - (anchor - pan) * (new_zoom / old_zoom)
}

/// Frame-rate independent share of the distance to cover this frame.
pub fn ease_factor(k: f64, dt: f64) -> f64 {
    1.0 - (1.0 - k).powf(clamp(dt, 0.0, MAX_FRAME_SECONDS) * 60.0)
}

/// One easing step toward `target`; reduced motion jumps straight there.
pub fn ease(value: f64, target: f64, k: f64, dt: f64, reduce_motion: bool) -> f64 {
    if reduce_motion {
        target
    } else {
        value + (target - value) * ease_factor(k, dt)
    }
}

/// Screen = world · zoom + pan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn to_screen(&self, wx: f64, wy: f64) -> (f64, f64) {
        (wx * self.zoom + self.pan_x, wy * self.zoom + self.pan_y)
    }

    pub fn to_world(&self, sx: f64, sy: f64) -> (f64, f64) {
        ((sx - self.pan_x) / self.zoom, (sy - self.pan_y) / self.zoom)
    }

    /// Multiplies the zoom by `factor` (clamped) keeping the world point under
    /// the pointer fixed.
    pub fn zoom_at(&mut self, sx: f64, sy: f64, factor: f64) {
        let z = clamp_zoom(self.zoom * factor);
        self.pan_x = zoom_about(self.pan_x, self.zoom, z, sx);
        self.pan_y = zoom_about(self.pan_y, self.zoom, z, sy);
        self.zoom = z;
    }

    /// Centres the world origin in a `width × height` view at zoom 1.
    pub fn reset(&mut self, width: f64, height: f64) {
        *self = Camera {
            pan_x: width / 2.0,
            pan_y: height / 2.0,
            zoom: 1.0,
        };
    }

    /// Fits a world rectangle with `margin` pixels around it.
    pub fn fit(&mut self, bounds: (f32, f32, f32, f32), width: f64, height: f64, margin: f64) {
        let (x0, y0, x1, y1) = (
            bounds.0 as f64,
            bounds.1 as f64,
            bounds.2 as f64,
            bounds.3 as f64,
        );
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let zoom = clamp_zoom(((width - 2.0 * margin) / w).min((height - 2.0 * margin) / h));
        self.zoom = zoom;
        self.pan_x = width / 2.0 - (x0 + x1) / 2.0 * zoom;
        self.pan_y = height / 2.0 - (y0 + y1) / 2.0 * zoom;
    }
}

/// Uniform grid over node centres for pointer queries.
#[derive(Default, Debug)]
pub struct HitGrid {
    cell: f32,
    min_x: f32,
    min_y: f32,
    nx: usize,
    ny: usize,
    max_radius: f32,
    head: Vec<u32>,
    next: Vec<u32>,
}

const NIL: u32 = u32::MAX;

impl HitGrid {
    pub fn new() -> Self {
        Self::default()
    }

    /// Indexes the nodes. Reuses its buffers between calls.
    pub fn rebuild(&mut self, xs: &[f32], ys: &[f32], radii: &[f32]) {
        let n = xs.len().min(ys.len()).min(radii.len());
        self.next.clear();
        self.next.resize(n, NIL);
        self.head.clear();
        self.nx = 0;
        self.ny = 0;
        if n == 0 {
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let mut rmax = 0.0f32;
        for i in 0..n {
            if !(xs[i].is_finite() && ys[i].is_finite()) {
                continue;
            }
            x0 = x0.min(xs[i]);
            x1 = x1.max(xs[i]);
            y0 = y0.min(ys[i]);
            y1 = y1.max(ys[i]);
            rmax = rmax.max(radii[i]);
        }
        if x0 > x1 {
            return;
        }
        let mut cell = (rmax * 2.0).max(32.0);
        let budget = (4 * n + 1024) as f32;
        let area = ((x1 - x0) / cell + 1.0) * ((y1 - y0) / cell + 1.0);
        if area > budget {
            cell *= (area / budget).sqrt();
        }
        self.cell = cell;
        self.min_x = x0;
        self.min_y = y0;
        self.max_radius = rmax;
        self.nx = ((x1 - x0) / cell) as usize + 1;
        self.ny = ((y1 - y0) / cell) as usize + 1;
        self.head.resize(self.nx * self.ny, NIL);
        for i in 0..n {
            if !(xs[i].is_finite() && ys[i].is_finite()) {
                continue;
            }
            let c = self.cell_of(xs[i], ys[i]);
            self.next[i] = self.head[c];
            self.head[c] = i as u32;
        }
    }

    fn cell_of(&self, x: f32, y: f32) -> usize {
        let cx = (((x - self.min_x) / self.cell) as usize).min(self.nx - 1);
        let cy = (((y - self.min_y) / self.cell) as usize).min(self.ny - 1);
        cy * self.nx + cx
    }

    /// The topmost node under `(px, py)`; later indices are drawn on top.
    /// `slack` widens every marker (pointer tolerance).
    pub fn hit(
        &self,
        xs: &[f32],
        ys: &[f32],
        radii: &[f32],
        px: f32,
        py: f32,
        slack: f32,
    ) -> Option<usize> {
        self.hit_ranked(xs, ys, radii, px, py, slack, |i| i as u32)
    }

    /// Like [`HitGrid::hit`] with an explicit draw rank (highest wins).
    #[allow(clippy::too_many_arguments)]
    pub fn hit_ranked(
        &self,
        xs: &[f32],
        ys: &[f32],
        radii: &[f32],
        px: f32,
        py: f32,
        slack: f32,
        rank: impl Fn(usize) -> u32,
    ) -> Option<usize> {
        if self.nx == 0 || !px.is_finite() || !py.is_finite() {
            return None;
        }
        let reach = self.max_radius + slack.max(0.0);
        let lo = |v: f32, min: f32, n: usize| {
            (((v - reach - min) / self.cell).floor().max(0.0) as usize).min(n - 1)
        };
        let hi = |v: f32, min: f32, n: usize| {
            (((v + reach - min) / self.cell).floor().max(0.0) as usize).min(n - 1)
        };
        if px + reach < self.min_x || py + reach < self.min_y {
            return None;
        }
        let (cx0, cx1) = (lo(px, self.min_x, self.nx), hi(px, self.min_x, self.nx));
        let (cy0, cy1) = (lo(py, self.min_y, self.ny), hi(py, self.min_y, self.ny));
        let mut best: Option<(u32, usize)> = None;
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                let mut i = self.head[cy * self.nx + cx];
                while i != NIL {
                    let k = i as usize;
                    let (dx, dy) = (xs[k] - px, ys[k] - py);
                    let r = radii[k] + slack;
                    if dx * dx + dy * dy <= r * r {
                        let rk = rank(k);
                        if !matches!(best, Some((r, _)) if r >= rk) {
                            best = Some((rk, k));
                        }
                    }
                    i = self.next[k];
                }
            }
        }
        best.map(|(_, k)| k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "{a} vs {b}");
    }

    #[test]
    fn marker_sizes_grow_logarithmically_and_cap() {
        close(marker_radius(0.0, 1.0) as f64, 6.0);
        close(marker_radius(3.0, 1.0) as f64, 10.0);
        close(marker_radius(15.0, 1.5) as f64, 21.0);
        close(marker_radius(-2.0, 1.0) as f64, 6.0);
        close(marker_radius(1_048_576.0, 2.0) as f64, 48.0);
        close(screen_radius(10.0, 0.25) as f64, 5.0);
        close(screen_radius(10.0, 4.0) as f64, 20.0);
        close(label_font_size(10.0) as f64, 14.0);
    }

    #[test]
    fn labels_fade_with_zoom_and_detail() {
        close(label_opacity(0.12, 0.0), 0.0);
        close(label_opacity(0.47, 0.0), 0.5);
        close(label_opacity(0.82, 0.0), 1.0);
        close(label_opacity(0.47, 1.0), 0.25);
        for z in [0.04, 0.2, 0.8, 3.0, 6.0] {
            assert!(label_opacity(z, -1.0) >= label_opacity(z, 2.0));
        }
    }

    #[test]
    fn lines_and_arrows() {
        for z in [0.04, 0.5, 1.0, 6.0] {
            close(line_world_width(1.25, z) * z, 1.25);
        }
        close(arrow_opacity(0.18), 0.0);
        close(arrow_opacity(0.49), 0.5);
        close(arrow_opacity(0.8), 1.0);
        close(arrow_length(4.0), 12.0);
        close(local_weight(0) as f64, 12.0);
        close(local_weight(1) as f64, 6.0);
        close(local_weight(2) as f64, 4.0);
        assert!(local_weight(6) > 0.0);
        close(timeline_speed(12), 8.0);
        close(timeline_speed(240), 20.0);
        close(timeline_speed(1200), 72.0);
    }

    #[test]
    fn wheel_maps_devices_and_clamps_zoom() {
        close(wheel_pixels(2.0, WheelUnit::Line, false), 48.0);
        close(wheel_pixels(1.0, WheelUnit::Page, false), 640.0);
        close(wheel_pixels(8.0, WheelUnit::Pixel, true), 16.0);
        close(wheel_pixels(40.0, WheelUnit::Pixel, true), 40.0);
        close(wheel_factor(480.0), (-1.0f64).exp());
        close(wheel_factor(480.0) * wheel_factor(-480.0), 1.0);
        close(clamp_zoom(1e-8), 0.04);
        close(clamp_zoom(100.0), 6.0);
        close(clamp_zoom(f64::NAN), 1.0);
    }

    #[test]
    fn zoom_keeps_the_anchor_world_point_fixed() {
        for (pan, before, after, anchor) in [(28.0, 1.0, 2.0, 180.0), (-14.0, 0.2, 0.7, 500.0)] {
            let world = (anchor - pan) / before;
            close(
                zoom_about(pan, before, after, anchor) + world * after,
                anchor,
            );
        }
        let mut cam = Camera {
            pan_x: 30.0,
            pan_y: -20.0,
            zoom: 0.8,
        };
        let anchor = (200.0, 120.0);
        let world = cam.to_world(anchor.0, anchor.1);
        for f in [1.7, 0.3, 5.0, 100.0, 1e-9] {
            cam.zoom_at(anchor.0, anchor.1, f);
            let s = cam.to_screen(world.0, world.1);
            close(s.0, anchor.0);
            close(s.1, anchor.1);
            assert!((MIN_ZOOM..=MAX_ZOOM).contains(&cam.zoom));
        }
    }

    #[test]
    fn anchored_zoom_holds_for_random_cameras() {
        let mut rng = crate::layout::Rng::new(3);
        let mut r = move || rng.unit() as f64;
        for _ in 0..2000 {
            let mut cam = Camera {
                pan_x: r() * 4000.0 - 2000.0,
                pan_y: r() * 4000.0 - 2000.0,
                zoom: MIN_ZOOM + r() * (MAX_ZOOM - MIN_ZOOM),
            };
            let (ax, ay) = (r() * 1500.0, r() * 900.0);
            let world = cam.to_world(ax, ay);
            cam.zoom_at(
                ax,
                ay,
                wheel_factor(wheel_pixels(
                    r() * 800.0 - 400.0,
                    WheelUnit::Pixel,
                    r() < 0.5,
                )),
            );
            let s = cam.to_screen(world.0, world.1);
            assert!((s.0 - ax).abs() < 1e-6 && (s.1 - ay).abs() < 1e-6);
        }
    }

    #[test]
    fn camera_fit_contains_the_bounds() {
        let mut cam = Camera::default();
        cam.fit((-100.0, -50.0, 300.0, 150.0), 800.0, 600.0, 20.0);
        let (x0, y0) = cam.to_screen(-100.0, -50.0);
        let (x1, y1) = cam.to_screen(300.0, 150.0);
        assert!(x0 >= 19.9 && y0 >= 19.9 && x1 <= 780.1 && y1 <= 580.1);
    }

    #[test]
    fn easing_is_frame_independent() {
        let mut r = Vec::new();
        for hz in [30.0, 60.0, 120.0] {
            let mut v = 0.0;
            for _ in 0..hz as usize {
                v = ease(v, 1.0, EASE_K, 1.0 / hz, false);
            }
            r.push(v);
        }
        close(r[0], r[1]);
        close(r[1], r[2]);
        close(ease_factor(EASE_K, 1.0 / 60.0), 0.16);
        close(ease_factor(0.2, 2.0), ease_factor(0.2, 0.05));
        close(ease(0.0, 1.0, 0.2, 1.0 / 60.0, true), 1.0);
    }

    #[test]
    fn hit_testing_uses_draw_order_and_slack() {
        let (xs, ys, rs) = ([0.0f32, 3.0], [0.0f32, 0.0], [5.0f32, 5.0]);
        let mut g = HitGrid::new();
        g.rebuild(&xs, &ys, &rs);
        assert_eq!(g.hit(&xs, &ys, &rs, 2.0, 0.0, 0.0), Some(1));
        assert_eq!(
            g.hit_ranked(&xs, &ys, &rs, 2.0, 0.0, 0.0, |i| 1 - i as u32),
            Some(0)
        );
        assert_eq!(g.hit(&xs, &ys, &rs, 9.0, 0.0, 0.0), None);
        assert_eq!(g.hit(&xs, &ys, &rs, 9.0, 0.0, 4.0), Some(1));
        assert_eq!(g.hit(&xs, &ys, &rs, f32::NAN, 0.0, 0.0), None);
        assert_eq!(g.hit(&xs, &ys, &rs, -1e9, 0.0, 0.0), None);
        let mut empty = HitGrid::new();
        empty.rebuild(&[], &[], &[]);
        assert_eq!(empty.hit(&[], &[], &[], 0.0, 0.0, 0.0), None);
    }

    #[test]
    fn hit_grid_agrees_with_brute_force() {
        let mut seed = 7u64;
        let mut rnd = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as f32) / (1u64 << 31) as f32
        };
        let n = 500;
        let xs: Vec<f32> = (0..n).map(|_| rnd() * 4000.0 - 2000.0).collect();
        let ys: Vec<f32> = (0..n).map(|_| rnd() * 4000.0 - 2000.0).collect();
        let rs: Vec<f32> = (0..n).map(|_| 6.0 + rnd() * 18.0).collect();
        let mut g = HitGrid::new();
        g.rebuild(&xs, &ys, &rs);
        for _ in 0..2000 {
            let (px, py) = (rnd() * 4400.0 - 2200.0, rnd() * 4400.0 - 2200.0);
            let brute = (0..n)
                .rev()
                .find(|&i| (xs[i] - px).powi(2) + (ys[i] - py).powi(2) <= (rs[i] + 3.0).powi(2));
            assert_eq!(g.hit(&xs, &ys, &rs, px, py, 3.0), brute);
        }
    }
}
