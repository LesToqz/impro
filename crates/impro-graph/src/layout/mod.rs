//! Force-directed layout: centre pull, link springs, Barnes–Hut repulsion and
//! collision, advanced in fixed 60 Hz steps.
//!
//! Positions live in flat `f32` arrays (structure of arrays). A tick performs
//! no allocation once the graph has been loaded.

mod collide;
mod place;
mod quadtree;

pub use place::{place_new, Rng, AREA_PER_NODE, NEIGHBOUR_JITTER};
pub use quadtree::{brute_acceleration, QuadTree};

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::geometry::marker_radius;
use crate::model::Graph;

/// Fixed simulation rate.
pub const TICKS_PER_SECOND: u32 = 60;
pub const ALPHA_MIN: f64 = 0.002;
/// Ticks for alpha to cool from 1 to [`ALPHA_MIN`].
pub const COOLING_TICKS: f64 = 240.0;
/// Alpha after a structural edit.
pub const REHEAT: f64 = 0.4;
/// Share of the velocity lost per tick.
pub const VELOCITY_DECAY: f32 = 0.45;
/// Barnes–Hut opening threshold.
pub const THETA: f32 = 0.65;
/// Squared distance below which repulsion stops growing.
pub const MIN_DISTANCE2: f32 = 64.0;
pub const COLLIDE_RADIUS: f32 = 32.0;
pub const COLLIDE_STRENGTH: f32 = 0.7;
/// Seed of the deterministic placement of new nodes.
pub const DEFAULT_SEED: u64 = 17;

/// Strengths driving the simulation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForceParams {
    /// Pull toward the origin.
    pub center: f32,
    /// Spring stiffness.
    pub link: f32,
    /// Spring rest length.
    pub distance: f32,
    /// Many-body strength (negative repels).
    pub repel: f32,
}

impl Default for ForceParams {
    fn default() -> Self {
        ForceParams::from_controls(0.12, 0.35, 160.0, 6.0)
    }
}

impl ForceParams {
    /// Maps the panel controls: centre, link and distance are used directly,
    /// repulsion is `-120 × control`. Inputs are clamped to the control ranges
    /// and non-finite values fall back to the defaults.
    pub fn from_controls(center: f64, link: f64, distance: f64, repel_control: f64) -> Self {
        let bounded = |v: f64, lo: f64, hi: f64, d: f64| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                d
            }
        };
        ForceParams {
            center: bounded(center, 0.0, 0.6, 0.12) as f32,
            link: bounded(link, 0.0, 1.5, 0.35) as f32,
            distance: bounded(distance, 48.0, 480.0, 160.0) as f32,
            repel: (-120.0 * bounded(repel_control, 0.0, 24.0, 6.0)) as f32,
        }
    }
}

#[derive(Clone, Copy)]
struct Spring {
    a: u32,
    b: u32,
    /// Share of the correction applied to `b` (the rest goes to `a`).
    bias: f32,
    strength: f32,
}

pub struct Layout {
    ids: Vec<Arc<str>>,
    index: HashMap<Arc<str>, u32>,
    x: Vec<f32>,
    y: Vec<f32>,
    vx: Vec<f32>,
    vy: Vec<f32>,
    pinned: Vec<bool>,
    pin_x: Vec<f32>,
    pin_y: Vec<f32>,
    weight: Vec<f32>,
    radius: Vec<f32>,
    size_multiplier: f32,
    springs: Vec<Spring>,
    params: ForceParams,
    alpha: f64,
    alpha_target: f64,
    decay: f64,
    ticks: u64,
    theta: f32,
    tree: QuadTree,
    grid: collide::CollideGrid,
    fingerprint: u64,
    loaded: bool,
    epoch: u64,
    seed: u64,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

fn graph_fingerprint(graph: &Graph) -> u64 {
    // order independent: sum of per-item hashes
    let h = |parts: &[&str]| {
        let mut s = std::collections::hash_map::DefaultHasher::new();
        for p in parts {
            p.hash(&mut s);
            0u8.hash(&mut s);
        }
        s.finish()
    };
    let mut acc = 0u64;
    for n in &graph.nodes {
        acc = acc.wrapping_add(h(&["n", &n.id]));
    }
    for &(s, t) in &graph.pairs {
        acc = acc.wrapping_add(h(&[
            "e",
            &graph.nodes[s as usize].id,
            &graph.nodes[t as usize].id,
        ]));
    }
    acc
}

impl Layout {
    pub fn new() -> Self {
        Layout {
            ids: Vec::new(),
            index: HashMap::new(),
            x: Vec::new(),
            y: Vec::new(),
            vx: Vec::new(),
            vy: Vec::new(),
            pinned: Vec::new(),
            pin_x: Vec::new(),
            pin_y: Vec::new(),
            weight: Vec::new(),
            radius: Vec::new(),
            size_multiplier: 1.15,
            springs: Vec::new(),
            params: ForceParams::default(),
            alpha: 1.0,
            alpha_target: 0.0,
            decay: 1.0 - ALPHA_MIN.powf(1.0 / COOLING_TICKS),
            ticks: 0,
            theta: THETA,
            tree: QuadTree::default(),
            grid: collide::CollideGrid::default(),
            fingerprint: 0,
            loaded: false,
            epoch: 0,
            seed: DEFAULT_SEED,
        }
    }

    pub fn with_seed(seed: u64) -> Self {
        Layout {
            seed,
            ..Self::new()
        }
    }

    /// Replaces the node and edge set. Surviving nodes keep position,
    /// velocity and pin; new nodes are placed beside their surviving
    /// neighbours or in a disc. Returns true when the structure changed (the
    /// simulation was reheated to 0.4, or started at 1 for the first graph).
    pub fn set_graph(&mut self, graph: &Graph) -> bool {
        let n = graph.nodes.len();
        let adj = graph.adjacency();
        let mut pos: Vec<Option<(f32, f32)>> = Vec::with_capacity(n);
        let mut ids: Vec<Arc<str>> = Vec::with_capacity(n);
        let mut x = vec![0.0f32; n];
        let mut y = vec![0.0f32; n];
        let mut vx = vec![0.0f32; n];
        let mut vy = vec![0.0f32; n];
        let mut pinned = vec![false; n];
        let mut pin_x = vec![0.0f32; n];
        let mut pin_y = vec![0.0f32; n];
        for (i, node) in graph.nodes.iter().enumerate() {
            ids.push(node.id.clone());
            match self.index.get(&node.id) {
                Some(&o) => {
                    let o = o as usize;
                    pos.push(Some((self.x[o], self.y[o])));
                    vx[i] = self.vx[o];
                    vy[i] = self.vy[o];
                    pinned[i] = self.pinned[o];
                    pin_x[i] = self.pin_x[o];
                    pin_y[i] = self.pin_y[o];
                }
                None => pos.push(None),
            }
        }
        self.epoch += 1;
        let mut rng = Rng::new(self.seed ^ self.epoch.wrapping_mul(0x2545_f491_4f6c_dd1d));
        place_new(&adj, &mut pos, &mut rng);
        for (i, p) in pos.iter().enumerate() {
            let (px, py) = p.unwrap_or((0.0, 0.0));
            x[i] = px;
            y[i] = py;
        }
        self.index = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i as u32))
            .collect();
        self.ids = ids;
        self.x = x;
        self.y = y;
        self.vx = vx;
        self.vy = vy;
        self.pinned = pinned;
        self.pin_x = pin_x;
        self.pin_y = pin_y;

        // springs follow distinct directed pairs, self pairs excluded
        let mut count = vec![0u32; n];
        let mut springs = Vec::with_capacity(graph.pairs.len());
        for &(s, t) in &graph.pairs {
            if s != t {
                springs.push((s, t));
                count[s as usize] += 1;
                count[t as usize] += 1;
            }
        }
        self.springs = springs
            .into_iter()
            .map(|(s, t)| {
                let (cs, ct) = (count[s as usize] as f32, count[t as usize] as f32);
                Spring {
                    a: s,
                    b: t,
                    bias: cs / (cs + ct),
                    strength: 1.0 / cs.min(ct),
                }
            })
            .collect();

        self.weight = graph.nodes.iter().map(|n| n.degree() as f32).collect();
        self.refresh_radii();
        self.tree.build(&self.x, &self.y);
        self.grid.reserve(n);

        let fp = graph_fingerprint(graph);
        let changed = !self.loaded || fp != self.fingerprint;
        if !self.loaded {
            self.alpha = 1.0;
        } else if changed {
            self.alpha = self.alpha.max(REHEAT);
        }
        self.loaded = true;
        self.fingerprint = fp;
        changed
    }

    pub fn len(&self) -> usize {
        self.x.len()
    }

    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    pub fn ids(&self) -> &[Arc<str>] {
        &self.ids
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.index.get(id).map(|i| *i as usize)
    }

    pub fn x(&self) -> &[f32] {
        &self.x
    }

    pub fn y(&self) -> &[f32] {
        &self.y
    }

    pub fn vx(&self) -> &[f32] {
        &self.vx
    }

    pub fn vy(&self) -> &[f32] {
        &self.vy
    }

    /// Marker weights (degree, or local-graph layer weights).
    pub fn weight(&self) -> &[f32] {
        &self.weight
    }

    /// World radius of each marker.
    pub fn radius(&self) -> &[f32] {
        &self.radius
    }

    fn refresh_radii(&mut self) {
        let m = self.size_multiplier;
        self.radius.clear();
        self.radius
            .extend(self.weight.iter().map(|w| marker_radius(*w, m)));
    }

    /// Replaces the marker weights (for example with local-graph layer weights).
    pub fn set_weights(&mut self, weights: &[f32]) {
        if weights.len() == self.weight.len() {
            self.weight.copy_from_slice(weights);
            self.refresh_radii();
        }
    }

    /// File marker size multiplier from the display controls.
    pub fn set_size_multiplier(&mut self, multiplier: f32) {
        if multiplier.is_finite() && multiplier > 0.0 {
            self.size_multiplier = multiplier;
            self.refresh_radii();
        }
    }

    pub fn set_params(&mut self, params: ForceParams) {
        self.params = params;
    }

    pub fn params(&self) -> ForceParams {
        self.params
    }

    /// Opening threshold; values of 0.7 and above are refused because the
    /// tree relies on never approximating the cell that holds the point.
    pub fn set_theta(&mut self, theta: f32) {
        if theta.is_finite() && theta > 0.0 && theta < 0.7 {
            self.theta = theta;
        }
    }

    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Raises alpha to at least `alpha` (default [`REHEAT`] via [`Layout::reheat`]).
    pub fn reheat_to(&mut self, alpha: f64) {
        if alpha.is_finite() {
            self.alpha = self.alpha.max(alpha.clamp(0.0, 1.0));
        }
    }

    pub fn reheat(&mut self) {
        self.reheat_to(REHEAT);
    }

    /// Restarts cooling from 1 (for "reset camera / restart layout").
    pub fn restart(&mut self) {
        self.alpha = 1.0;
    }

    pub fn set_alpha_target(&mut self, target: f64) {
        self.alpha_target = if target.is_finite() {
            target.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    /// True when cooling is finished and nothing keeps the simulation hot.
    /// A settled layout needs no further ticks (and no timer).
    pub fn settled(&self) -> bool {
        self.alpha < ALPHA_MIN && self.alpha_target <= 0.0
    }

    /// Fixes node `i` at a position (dragging); it stops feeling forces.
    pub fn pin(&mut self, i: usize, x: f32, y: f32) {
        if i < self.pinned.len() && x.is_finite() && y.is_finite() {
            self.pinned[i] = true;
            self.pin_x[i] = x;
            self.pin_y[i] = y;
        }
    }

    pub fn unpin(&mut self, i: usize) {
        if i < self.pinned.len() {
            self.pinned[i] = false;
        }
    }

    pub fn is_pinned(&self, i: usize) -> bool {
        self.pinned.get(i).copied().unwrap_or(false)
    }

    /// Starts dragging: pins the node and keeps the simulation warm.
    pub fn begin_drag(&mut self, i: usize, x: f32, y: f32) {
        self.pin(i, x, y);
        self.set_alpha_target(REHEAT);
        self.reheat();
    }

    pub fn drag_to(&mut self, i: usize, x: f32, y: f32) {
        self.pin(i, x, y);
    }

    /// Releases the node; the layout cools again.
    pub fn end_drag(&mut self, i: usize) {
        self.unpin(i);
        self.set_alpha_target(0.0);
    }

    /// Moves a node (and clears its velocity), e.g. to restore saved positions.
    pub fn set_position(&mut self, i: usize, x: f32, y: f32) {
        if i < self.x.len() && x.is_finite() && y.is_finite() {
            self.x[i] = x;
            self.y[i] = y;
            self.vx[i] = 0.0;
            self.vy[i] = 0.0;
        }
    }

    /// `(min x, min y, max x, max y)` of all nodes, padded by their radii.
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        if self.x.is_empty() {
            return None;
        }
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for i in 0..self.x.len() {
            let r = self.radius.get(i).copied().unwrap_or(0.0);
            b.0 = b.0.min(self.x[i] - r);
            b.1 = b.1.min(self.y[i] - r);
            b.2 = b.2.max(self.x[i] + r);
            b.3 = b.3.max(self.y[i] + r);
        }
        Some(b)
    }

    /// Advances one 1/60 s tick. Returns false (doing nothing) when settled.
    pub fn step(&mut self) -> bool {
        if self.settled() {
            return false;
        }
        self.alpha += (self.alpha_target - self.alpha) * self.decay;
        let alpha = self.alpha as f32;
        let n = self.x.len();
        let p = self.params;

        // centre pull toward the origin
        let kc = alpha * p.center;
        for i in 0..n {
            self.vx[i] -= self.x[i] * kc;
            self.vy[i] -= self.y[i] * kc;
        }

        // link springs
        let kl = alpha * p.link;
        for s in &self.springs {
            let (a, b) = (s.a as usize, s.b as usize);
            let mut dx = self.x[b] + self.vx[b] - self.x[a] - self.vx[a];
            let mut dy = self.y[b] + self.vy[b] - self.y[a] - self.vy[a];
            if dx == 0.0 {
                dx = 0.1;
            }
            if dy == 0.0 {
                dy = 0.1;
            }
            let len = (dx * dx + dy * dy).sqrt();
            let k = (len - p.distance) / len * kl * s.strength;
            dx *= k;
            dy *= k;
            self.vx[b] -= dx * s.bias;
            self.vy[b] -= dy * s.bias;
            self.vx[a] += dx * (1.0 - s.bias);
            self.vy[a] += dy * (1.0 - s.bias);
        }

        // repulsion
        if n > 1 && p.repel != 0.0 {
            self.tree.build(&self.x, &self.y);
            let strength = alpha * p.repel;
            let inv_theta2 = 1.0 / (self.theta * self.theta);
            for k in 0..n {
                let i = self.tree.order()[k] as usize;
                let (ax, ay) = self.tree.acceleration(
                    i as u32,
                    self.x[i],
                    self.y[i],
                    &self.x,
                    &self.y,
                    strength,
                    inv_theta2,
                    MIN_DISTANCE2,
                );
                self.vx[i] += ax;
                self.vy[i] += ay;
            }
        }

        // collision keeps a gap for labels
        self.grid.apply(
            COLLIDE_RADIUS,
            COLLIDE_STRENGTH,
            &self.x,
            &self.y,
            &mut self.vx,
            &mut self.vy,
        );

        // integrate
        let keep = 1.0 - VELOCITY_DECAY;
        for i in 0..n {
            if self.pinned[i] {
                self.x[i] = self.pin_x[i];
                self.y[i] = self.pin_y[i];
                self.vx[i] = 0.0;
                self.vy[i] = 0.0;
                continue;
            }
            self.vx[i] *= keep;
            self.vy[i] *= keep;
            self.x[i] += self.vx[i];
            self.y[i] += self.vy[i];
            if !(self.x[i].is_finite() && self.y[i].is_finite()) {
                // numerical blow-up guard; deterministic recovery near the origin
                self.x[i] = (i % 17) as f32;
                self.y[i] = (i % 13) as f32;
                self.vx[i] = 0.0;
                self.vy[i] = 0.0;
            }
        }
        self.ticks += 1;
        true
    }

    /// Runs ticks until settled or `max_ticks`; returns the ticks performed.
    pub fn settle(&mut self, max_ticks: usize) -> usize {
        let mut done = 0;
        while done < max_ticks && self.step() {
            done += 1;
        }
        done
    }

    /// Exact repulsion accelerations for the current positions (O(n²); tests).
    pub fn exact_repulsion(&self, strength: f32) -> Vec<(f32, f32)> {
        brute_acceleration(&self.x, &self.y, strength, MIN_DISTANCE2)
    }

    /// Barnes–Hut repulsion accelerations for the current positions.
    pub fn approx_repulsion(&mut self, strength: f32) -> Vec<(f32, f32)> {
        self.tree.build(&self.x, &self.y);
        let inv = 1.0 / (self.theta * self.theta);
        (0..self.x.len())
            .map(|i| {
                self.tree.acceleration(
                    i as u32,
                    self.x[i],
                    self.y[i],
                    &self.x,
                    &self.y,
                    strength,
                    inv,
                    MIN_DISTANCE2,
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
