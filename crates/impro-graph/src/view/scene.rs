//! The state behind the canvas: graph, layout, eased per-element appearance,
//! hover and drag, camera. GTK-free, so every rule is unit-testable.

use std::sync::Arc;

use super::camera::CameraState;
use super::palette::{Palette, Rgba};
use crate::delta::diff;
use crate::geometry::{ease_factor, EASE_K, HOVER_DIM, LABEL_OFFSET};
use crate::layout::{ForceParams, Layout};
use crate::model::{Adjacency, Graph, Line, NodeKind};

/// Pace of palette changes: a full blend takes this many seconds.
const THEME_SECONDS: f32 = 0.18;
/// Share of the remaining distance display multipliers cover per frame.
const DISPLAY_K: f64 = 0.25;
/// Nothing is drawn for faded-out nodes below this opacity.
const GHOST_FLOOR: f32 = 0.02;
const MAX_GHOSTS: usize = 3000;
const NONE: u32 = u32::MAX;
/// Largest number of physics ticks one frame may run.
const MAX_TICKS_PER_FRAME: u32 = 3;
const TICK: f64 = 1.0 / 60.0;

/// A node as the worker delivers it.
#[derive(Clone, Debug, Default)]
pub struct Built {
    pub graph: Graph,
    /// Breadth-first layer of each node in a local graph.
    pub layers: Option<Vec<u8>>,
    /// Index of the local focus.
    pub focus: Option<usize>,
}

impl Built {
    pub fn global(graph: Graph) -> Built {
        Built {
            graph,
            layers: None,
            focus: None,
        }
    }
}

/// Appearance inputs from the preferences.
#[derive(Clone, Debug)]
pub struct Style {
    pub show_arrow: bool,
    pub node_size: f64,
    pub line_width: f64,
    pub label_detail: f64,
    /// Colour of every colour rule (`None` = palette colour of its index).
    pub rule_colors: Vec<Option<Rgba>>,
    pub reduce_motion: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            show_arrow: true,
            node_size: 1.15,
            line_width: 1.25,
            label_detail: 0.2,
            rule_colors: Vec::new(),
            reduce_motion: false,
        }
    }
}

/// Display multipliers as currently drawn (eased toward [`Style`]).
#[derive(Clone, Copy, Debug)]
pub struct Display {
    pub node: f64,
    pub line: f64,
    pub label_detail: f64,
}

/// A removed node fading out.
#[derive(Clone, Copy, Debug)]
pub struct Ghost {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub color: [f32; 3],
    pub alpha: f32,
}

/// What an update did, for the host and the tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub added: usize,
    pub removed: usize,
    /// The set of nodes or connections changed (the layout reheated).
    pub structural: bool,
    /// Nothing of the previous graph survived.
    pub replaced: bool,
}

pub struct Scene {
    pub graph: Graph,
    pub layout: Layout,
    pub(crate) adj: Adjacency,
    pub(crate) lines: Vec<Line>,
    /// Directed non-self pairs (one direction marker each).
    pub(crate) arrows: Vec<(u32, u32)>,
    pub(crate) layers: Option<Vec<u8>>,
    pub focus: Option<usize>,
    // eased appearance, per node / line / arrow
    pub(crate) alpha: Vec<f32>,
    pub(crate) color: Vec<[f32; 3]>,
    pub(crate) drop: Vec<f32>,
    pub(crate) line_alpha: Vec<f32>,
    pub(crate) line_mix: Vec<f32>,
    pub(crate) arrow_alpha: Vec<f32>,
    near: Vec<bool>,
    near_for: Option<usize>,
    near_stale: bool,
    // interaction
    pub hover: Option<usize>,
    pub drag: Option<usize>,
    pub active: Option<usize>,
    active_id: Option<Arc<str>>,
    // appearance
    pub style: Style,
    pub disp: Display,
    pub palette: Palette,
    pal_from: Palette,
    pal_to: Palette,
    pal_t: f32,
    pub ghosts: Vec<Ghost>,
    pub cam: CameraState,
    pub size: (f64, f64),
    acc: f64,
    pub has_data: bool,
}

/// Colour of one node: highlight and focus first, then colour rules (files
/// only), then the kind colour.
fn color_for(
    p: &Palette,
    rules: &[Option<Rgba>],
    node: &crate::model::Node,
    highlighted: bool,
    active: bool,
) -> Rgba {
    if highlighted {
        return p.highlight;
    }
    if active && p.focused.a > 0.0 {
        return p.focused;
    }
    match (node.kind, node.group) {
        (NodeKind::Tag, _) => p.tag,
        (NodeKind::Unresolved, _) => p.unresolved,
        (_, Some(g)) => rules
            .get(g as usize)
            .copied()
            .flatten()
            .unwrap_or_else(|| p.group(g as usize)),
        (NodeKind::Attachment, None) => p.attachment,
        (NodeKind::Code, None) => p.code,
        (NodeKind::Note, None) => p.fill,
    }
}

impl Default for Scene {
    fn default() -> Self {
        Scene::new()
    }
}

impl Scene {
    pub fn new() -> Scene {
        let style = Style::default();
        let palette = Palette::fallback();
        Scene {
            graph: Graph::empty(),
            layout: Layout::new(),
            adj: Adjacency::default(),
            lines: Vec::new(),
            arrows: Vec::new(),
            layers: None,
            focus: None,
            alpha: Vec::new(),
            color: Vec::new(),
            drop: Vec::new(),
            line_alpha: Vec::new(),
            line_mix: Vec::new(),
            arrow_alpha: Vec::new(),
            near: Vec::new(),
            near_for: None,
            near_stale: true,
            hover: None,
            drag: None,
            active: None,
            active_id: None,
            disp: Display {
                node: style.node_size,
                line: style.line_width,
                label_detail: style.label_detail,
            },
            style,
            pal_from: palette.clone(),
            pal_to: palette.clone(),
            palette,
            pal_t: 1.0,
            ghosts: Vec::new(),
            cam: CameraState::default(),
            size: (0.0, 0.0),
            acc: TICK,
            has_data: false,
        }
    }

    pub fn len(&self) -> usize {
        self.graph.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.graph.nodes.is_empty()
    }

    /// The node under the pointer or being dragged.
    pub fn highlight(&self) -> Option<usize> {
        self.drag.or(self.hover)
    }

    pub fn reduce_motion(&self) -> bool {
        self.style.reduce_motion
    }

    // ---- data -----------------------------------------------------------

    fn marker_weights(built: &Built) -> Option<Vec<f32>> {
        built.layers.as_ref().map(|layers| {
            layers
                .iter()
                .map(|l| crate::geometry::local_weight(*l as u32))
                .collect()
        })
    }

    /// For every new node its index in the old graph (`u32::MAX` if new).
    fn old_indices(old: &Graph, new: &Graph) -> Vec<u32> {
        let mut map = vec![NONE; new.nodes.len()];
        let (mut i, mut j) = (0, 0);
        while i < old.nodes.len() && j < new.nodes.len() {
            match old.nodes[i].id.cmp(&new.nodes[j].id) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    map[j] = i as u32;
                    i += 1;
                    j += 1;
                }
            }
        }
        map
    }

    /// Replaces the graph, keeping position and appearance of surviving nodes
    /// and fading removed ones out.
    pub fn apply(&mut self, built: Built) -> ApplyReport {
        let delta = diff(&self.graph, &built.graph);
        let old_of_new = Self::old_indices(&self.graph, &built.graph);
        let survivors = old_of_new.iter().filter(|o| **o != NONE).count();
        let n = built.graph.nodes.len();
        let old_n = self.graph.nodes.len();
        let replaced = self.has_data && old_n > 0 && survivors == 0;
        let report = ApplyReport {
            added: delta.added_nodes.len(),
            removed: delta.removed_nodes.len(),
            structural: delta.is_structural(),
            replaced,
        };

        // ghosts of removed nodes
        if !self.style.reduce_motion {
            let mut kept = vec![false; old_n];
            for o in old_of_new.iter().filter(|o| **o != NONE) {
                kept[*o as usize] = true;
            }
            for (i, _) in kept.iter().enumerate().filter(|(_, k)| !**k) {
                if self.alpha[i] > GHOST_FLOOR && self.ghosts.len() < MAX_GHOSTS {
                    self.ghosts.push(Ghost {
                        x: self.layout.x()[i],
                        y: self.layout.y()[i],
                        radius: self.layout.radius()[i],
                        color: self.color[i],
                        alpha: self.alpha[i],
                    });
                }
            }
        }

        // identity of the interaction targets
        let hover_id = self.hover.map(|i| self.graph.nodes[i].id.clone());
        let drag_id = self.drag.map(|i| self.graph.nodes[i].id.clone());

        // previous per-node state, moved out for remapping
        let old_alpha = std::mem::take(&mut self.alpha);
        let old_color = std::mem::take(&mut self.color);
        let old_drop = std::mem::take(&mut self.drop);
        let old_lines = std::mem::take(&mut self.lines);
        let old_line_alpha = std::mem::take(&mut self.line_alpha);
        let old_line_mix = std::mem::take(&mut self.line_mix);

        let Built {
            graph,
            layers,
            focus,
        } = built;
        let weights = Self::marker_weights(&Built {
            graph: Graph::empty(),
            layers: layers.clone(),
            focus,
        });
        self.layout.set_graph(&graph);
        if let Some(w) = weights {
            self.layout.set_weights(&w);
        }
        self.layout.set_size_multiplier(self.disp.node as f32);
        self.adj = graph.adjacency();
        self.lines = graph.lines();
        self.arrows = graph
            .pairs
            .iter()
            .copied()
            .filter(|(s, t)| s != t)
            .collect();
        self.graph = graph;
        self.layers = layers;
        self.focus = focus;

        if replaced {
            self.cam.cam.pan_x = self.size.0 / 2.0;
            self.cam.cam.pan_y = self.size.1 / 2.0;
            self.cam.vx = 0.0;
            self.cam.vy = 0.0;
            self.cam.stop_tracking();
        }

        // per-node appearance
        self.alpha = vec![0.0; n];
        self.color = vec![[0.0; 3]; n];
        self.drop = vec![0.0; n];
        self.active = self
            .active_id
            .as_ref()
            .and_then(|id| self.graph.index_of(id));
        self.hover = hover_id.and_then(|id| self.graph.index_of(&id));
        self.drag = drag_id.and_then(|id| self.graph.index_of(&id));
        let fresh_alpha = if self.style.reduce_motion { 1.0 } else { 0.0 };
        for i in 0..n {
            match old_of_new[i] {
                NONE => {
                    self.alpha[i] = fresh_alpha;
                    self.color[i] = self.target_color(i).rgb();
                }
                o => {
                    let o = o as usize;
                    self.alpha[i] = old_alpha[o];
                    self.color[i] = old_color[o];
                    self.drop[i] = old_drop[o];
                }
            }
        }

        // per-line appearance: surviving lines keep theirs
        let fresh_line = if self.style.reduce_motion { 1.0 } else { 0.2 };
        self.line_alpha = vec![fresh_line; self.lines.len()];
        self.line_mix = vec![0.0; self.lines.len()];
        for (k, l) in self.lines.iter().enumerate() {
            let (oa, ob) = (old_of_new[l.a as usize], old_of_new[l.b as usize]);
            if oa == NONE || ob == NONE {
                continue;
            }
            if let Ok(o) = old_lines.binary_search_by_key(&(oa, ob), |l| (l.a, l.b)) {
                self.line_alpha[k] = old_line_alpha[o];
                self.line_mix[k] = old_line_mix[o];
            }
        }
        self.arrow_alpha = vec![1.0; self.arrows.len()];
        self.near.clear();
        self.near.resize(n, false);
        self.near_stale = true;
        if !self.has_data {
            self.layout.restart();
            self.has_data = true;
        }
        report
    }

    /// Drops all data; the next [`Scene::apply`] places nodes afresh.
    pub fn clear(&mut self) {
        self.apply(Built::default());
        self.layout = Layout::new();
        self.has_data = false;
    }

    /// Drops positions and places the nodes afresh (reset camera).
    pub fn restart_layout(&mut self) {
        let graph = std::mem::take(&mut self.graph);
        let built = Built {
            layers: self.layers.clone(),
            focus: self.focus,
            graph,
        };
        self.clear();
        self.ghosts.clear();
        self.apply(built);
    }

    // ---- state setters ---------------------------------------------------

    pub fn set_active(&mut self, id: Option<&str>) {
        self.active_id = id.map(Arc::from);
        self.active = id.and_then(|id| self.graph.index_of(id));
    }

    pub fn set_size(&mut self, w: f64, h: f64) {
        let old = self.size;
        self.size = (w, h);
        self.cam.resized(old, self.size);
    }

    /// Eased change to a new palette.
    pub fn set_palette(&mut self, next: Palette) {
        if next == self.pal_to {
            return;
        }
        self.pal_from = self.palette.clone();
        self.pal_to = next;
        if self.style.reduce_motion {
            self.palette = self.pal_to.clone();
            self.pal_t = 1.0;
        } else {
            self.pal_t = 0.0;
        }
    }

    pub fn set_style(&mut self, style: Style) {
        if style.reduce_motion {
            self.disp = Display {
                node: style.node_size,
                line: style.line_width,
                label_detail: style.label_detail,
            };
            self.layout.set_size_multiplier(self.disp.node as f32);
            self.palette = self.pal_to.clone();
            self.pal_t = 1.0;
        }
        self.style = style;
    }

    pub fn set_forces(&mut self, forces: ForceParams) {
        self.layout.set_params(forces);
    }

    // ---- appearance ------------------------------------------------------

    /// Fill colour a node eases toward.
    pub fn target_color(&self, i: usize) -> Rgba {
        color_for(
            &self.palette,
            &self.style.rule_colors,
            &self.graph.nodes[i],
            self.highlight() == Some(i),
            self.active == Some(i),
        )
    }

    /// Opacity of a node's marker: unresolved targets are translucent unless
    /// they carry a highlight.
    pub fn base_alpha(&self, i: usize) -> f32 {
        let node = &self.graph.nodes[i];
        if node.kind == NodeKind::Unresolved && self.active != Some(i) {
            self.palette.unresolved.a
        } else {
            1.0
        }
    }

    pub fn is_near(&self, i: usize) -> bool {
        self.near.get(i).copied().unwrap_or(false)
    }

    // ---- interaction -----------------------------------------------------

    pub fn set_hover(&mut self, node: Option<usize>) -> bool {
        if self.hover == node {
            return false;
        }
        self.hover = node;
        true
    }

    /// Screen position and radius of a node.
    pub fn screen(&self, i: usize) -> (f64, f64, f64) {
        let z = self.cam.cam.zoom;
        let (sx, sy) = self
            .cam
            .cam
            .to_screen(self.layout.x()[i] as f64, self.layout.y()[i] as f64);
        (sx, sy, self.layout.radius()[i] as f64 * z.sqrt())
    }

    /// The node under `(x, y)`; the highlighted one wins, later ones are on top.
    pub fn pick(&self, x: f64, y: f64, slack: f64) -> Option<usize> {
        let z = self.cam.cam.zoom;
        let rs = z.sqrt();
        let (xs, ys, rad) = (self.layout.x(), self.layout.y(), self.layout.radius());
        let hl = self.highlight();
        let mut best = None;
        for i in (0..xs.len()).rev() {
            let dx = self.cam.cam.pan_x + z * xs[i] as f64 - x;
            let dy = self.cam.cam.pan_y + z * ys[i] as f64 - y;
            let r = rad[i] as f64 * rs + slack;
            if dx * dx + dy * dy <= r * r {
                if hl == Some(i) {
                    return Some(i);
                }
                best.get_or_insert(i);
            }
        }
        best
    }

    /// Pointer movement: keeps the current hover while the pointer stays on
    /// it (with a little slack), otherwise picks again.
    pub fn update_hover(&mut self, x: f64, y: f64) -> bool {
        if let Some(h) = self.hover.filter(|h| *h < self.len()) {
            let z = self.cam.cam.zoom;
            let (sx, sy, r) = self.screen(h);
            let r = r + 2.0 * z;
            if (sx - x) * (sx - x) + (sy - y) * (sy - y) <= r * r {
                return false;
            }
        }
        let hit = self.pick(x, y, 0.0);
        self.set_hover(hit)
    }

    /// Pins a node at a world position while it is dragged.
    pub fn drag_to(&mut self, i: usize, wx: f32, wy: f32) {
        if self.drag == Some(i) {
            self.layout.drag_to(i, wx, wy);
        } else {
            self.drag = Some(i);
            self.layout.begin_drag(i, wx, wy);
        }
    }

    pub fn end_drag(&mut self) {
        if let Some(i) = self.drag.take() {
            if i < self.len() {
                self.layout.end_drag(i);
            }
        }
    }

    // ---- stepping --------------------------------------------------------

    /// True while the scene needs frames: layout cooling, camera, easing.
    pub fn needs_frames(&self) -> bool {
        self.simulating()
            || self.cam.keys.any()
            || self.cam.vx != 0.0
            || self.cam.vy != 0.0
            || self.pal_t < 1.0
            || !self.ghosts.is_empty()
            || self.cam.is_tracking()
            || self.cam.cam.zoom != self.cam.target_zoom
            || self.eased_pending()
    }

    /// The force simulation still moves nodes.
    pub fn simulating(&self) -> bool {
        !self.layout.is_empty() && !self.layout.settled()
    }

    fn eased_pending(&self) -> bool {
        let s = &self.style;
        let d = &self.disp;
        (d.node - s.node_size).abs() >= 1e-3
            || (d.line - s.line_width).abs() >= 1e-3
            || (d.label_detail - s.label_detail).abs() >= 1e-3
            || self.elements_busy()
    }

    fn elements_busy(&self) -> bool {
        if self.near_stale {
            return true;
        }
        let hl = self.highlight();
        for i in 0..self.len() {
            let ta = if hl.is_none() || self.near[i] {
                1.0
            } else {
                HOVER_DIM
            };
            let td = if Some(i) == hl { LABEL_OFFSET } else { 0.0 };
            if (ta - self.alpha[i]).abs() > 1e-3 || (td - self.drop[i]).abs() > 0.05 {
                return true;
            }
            let t = self.target_color(i);
            let c = self.color[i];
            // colours move in 0..255 steps in the drawing; mirror that tolerance
            if (t.r - c[0]).abs() * 255.0 > 0.3
                || (t.g - c[1]).abs() * 255.0 > 0.3
                || (t.b - c[2]).abs() * 255.0 > 0.3
            {
                return true;
            }
        }
        for (k, l) in self.lines.iter().enumerate() {
            let touch = hl.is_some_and(|h| l.a as usize == h || l.b as usize == h);
            let ta = if hl.is_none() || touch {
                1.0
            } else {
                HOVER_DIM
            };
            let th = if touch { 1.0 } else { 0.0 };
            if (ta - self.line_alpha[k]).abs() > 1e-3 || (th - self.line_mix[k]).abs() > 1e-3 {
                return true;
            }
        }
        if self.style.show_arrow {
            for (k, (s, t)) in self.arrows.iter().enumerate() {
                let ta = if hl.is_none_or(|h| h == *s as usize || h == *t as usize) {
                    1.0
                } else {
                    HOVER_DIM
                };
                if (ta - self.arrow_alpha[k]).abs() > 1e-3 {
                    return true;
                }
            }
        }
        false
    }

    /// Advances physics, camera and easing by `dt` seconds. Returns true
    /// while another frame is needed.
    pub fn advance(&mut self, dt: f64) -> bool {
        let dt = dt.clamp(0.0, crate::geometry::MAX_FRAME_SECONDS);
        let mut active = false;
        if self.simulating() {
            self.acc += dt;
            let mut ticks = 0;
            while self.acc >= TICK - 1e-6 && ticks < MAX_TICKS_PER_FRAME {
                self.layout.step();
                self.acc -= TICK;
                ticks += 1;
            }
            if ticks == MAX_TICKS_PER_FRAME {
                self.acc = 0.0;
            }
            active = true;
        } else {
            self.acc = TICK;
        }
        let reduce = self.style.reduce_motion;
        let size = self.size;
        let hot = self.simulating();
        let panning = false;
        let (xs, ys) = (self.layout.x(), self.layout.y());
        let node = |i: usize| xs.get(i).copied().zip(ys.get(i).copied());
        // the camera borrows positions immutably, so step it on a copy of its state
        let mut cam = std::mem::take(&mut self.cam);
        let moved = cam.step(dt, reduce, size, hot, panning, node);
        self.cam = cam;
        active |= moved;
        active |= self.step_palette(dt);
        active |= self.step_display(dt);
        active |= self.step_elements(dt);
        active |= !self.ghosts.is_empty();
        active
    }

    fn step_palette(&mut self, dt: f64) -> bool {
        if self.pal_t >= 1.0 {
            return false;
        }
        self.pal_t = if self.style.reduce_motion {
            1.0
        } else {
            (self.pal_t + dt as f32 / THEME_SECONDS).min(1.0)
        };
        self.palette = if self.pal_t >= 1.0 {
            self.pal_to.clone()
        } else {
            self.pal_from.blend(&self.pal_to, self.pal_t)
        };
        self.pal_t < 1.0
    }

    fn step_display(&mut self, dt: f64) -> bool {
        let reduce = self.style.reduce_motion;
        let mut changed = false;
        let mut e = |v: f64, t: f64| -> f64 {
            if (v - t).abs() < 1e-3 {
                return t;
            }
            changed = true;
            crate::geometry::ease(v, t, DISPLAY_K, dt, reduce)
        };
        let s = self.style.clone();
        let node = e(self.disp.node, s.node_size);
        let line = e(self.disp.line, s.line_width);
        let detail = e(self.disp.label_detail, s.label_detail);
        if node != self.disp.node {
            self.layout.set_size_multiplier(node as f32);
        }
        self.disp = Display {
            node,
            line,
            label_detail: detail,
        };
        changed
    }

    fn refresh_near(&mut self) {
        let hl = self.highlight();
        if !self.near_stale && hl == self.near_for {
            return;
        }
        self.near.iter_mut().for_each(|v| *v = false);
        if let Some(h) = hl.filter(|h| *h < self.len()) {
            self.near[h] = true;
            for &j in self.adj.outgoing(h).iter().chain(self.adj.incoming(h)) {
                self.near[j as usize] = true;
            }
        }
        self.near_for = hl;
        self.near_stale = false;
    }

    /// Eases opacity, tint, label offset and line state toward their targets.
    fn step_elements(&mut self, dt: f64) -> bool {
        self.refresh_near();
        let f = if self.style.reduce_motion {
            1.0
        } else {
            ease_factor(EASE_K, dt) as f32
        };
        let hl = self.highlight();
        let mut busy = false;
        let mut approach = |v: &mut f32, target: f32, tol: f32| {
            let d = target - *v;
            if d > tol || d < -tol {
                *v += d * f;
                busy = true;
            } else {
                *v = target;
            }
        };
        for i in 0..self.graph.nodes.len() {
            let ta = if hl.is_none() || self.near[i] {
                1.0
            } else {
                HOVER_DIM
            };
            approach(&mut self.alpha[i], ta, 1e-3);
            let t = color_for(
                &self.palette,
                &self.style.rule_colors,
                &self.graph.nodes[i],
                hl == Some(i),
                self.active == Some(i),
            );
            let c = &mut self.color[i];
            approach(&mut c[0], t.r, 0.3 / 255.0);
            approach(&mut c[1], t.g, 0.3 / 255.0);
            approach(&mut c[2], t.b, 0.3 / 255.0);
            let td = if hl == Some(i) { LABEL_OFFSET } else { 0.0 };
            approach(&mut self.drop[i], td, 0.05);
        }
        for (k, l) in self.lines.iter().enumerate() {
            let touch = hl.is_some_and(|h| l.a as usize == h || l.b as usize == h);
            let ta = if hl.is_none() || touch {
                1.0
            } else {
                HOVER_DIM
            };
            let th = if touch { 1.0 } else { 0.0 };
            approach(&mut self.line_alpha[k], ta, 1e-3);
            approach(&mut self.line_mix[k], th, 1e-3);
        }
        if self.style.show_arrow {
            for (k, (s, t)) in self.arrows.iter().enumerate() {
                let ta = if hl.is_none_or(|h| h == *s as usize || h == *t as usize) {
                    1.0
                } else {
                    HOVER_DIM
                };
                approach(&mut self.arrow_alpha[k], ta, 1e-3);
            }
        }
        let floor = 0.01;
        self.ghosts.retain_mut(|g| {
            g.alpha -= g.alpha * f + dt as f32 * 0.5;
            g.alpha > floor
        });
        busy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(n: usize) -> Graph {
        let pairs: Vec<(u32, u32)> = (0..n as u32 - 1).map(|i| (i, i + 1)).collect();
        Graph::from_pairs(n, &pairs)
    }

    fn settle(s: &mut Scene) {
        for _ in 0..2000 {
            if !s.advance(1.0 / 60.0) {
                return;
            }
        }
        panic!("scene never settled");
    }

    fn loaded(n: usize) -> Scene {
        let mut s = Scene::new();
        s.set_size(800.0, 600.0);
        s.apply(Built::global(chain(n)));
        settle(&mut s);
        s
    }

    #[test]
    fn a_fresh_scene_settles_and_then_needs_no_frames() {
        let s = loaded(12);
        assert!(!s.needs_frames());
        assert!(s.layout.settled());
        assert!(s.alpha.iter().all(|a| (*a - 1.0).abs() < 1e-3));
        assert_eq!(s.lines.len(), 11);
        assert_eq!(s.arrows.len(), 11);
    }

    #[test]
    fn an_empty_scene_is_idle() {
        let mut s = Scene::new();
        s.apply(Built::default());
        assert!(!s.advance(0.016));
        assert!(!s.needs_frames() || s.layout.settled());
    }

    #[test]
    fn hover_dims_everything_but_the_neighbourhood() {
        let mut s = loaded(6);
        assert!(s.set_hover(Some(2)));
        settle(&mut s);
        for i in 0..6 {
            let want = if (1..=3).contains(&i) { 1.0 } else { HOVER_DIM };
            assert!((s.alpha[i] - want).abs() < 2e-3, "node {i}: {}", s.alpha[i]);
        }
        // lines touching the hovered node stay lit and tinted
        for (k, l) in s.lines.iter().enumerate() {
            let touch = l.a == 2 || l.b == 2;
            assert_eq!(s.line_mix[k] > 0.99, touch);
            assert!((s.line_alpha[k] - if touch { 1.0 } else { HOVER_DIM }).abs() < 2e-3);
        }
        // the label of the hovered node drops
        assert!((s.drop[2] - LABEL_OFFSET).abs() < 0.06);
        assert!(s.drop[0].abs() < 1e-6);
        // and everything returns when the pointer leaves
        s.set_hover(None);
        settle(&mut s);
        assert!(s.alpha.iter().all(|a| (*a - 1.0).abs() < 2e-3));
        assert!(s.drop.iter().all(|d| d.abs() < 0.06));
    }

    #[test]
    fn hovered_node_takes_the_highlight_colour() {
        let mut s = loaded(4);
        let before = s.color[1];
        s.set_hover(Some(1));
        settle(&mut s);
        let want = s.palette.highlight;
        assert!((s.color[1][0] - want.r).abs() < 2.0 / 255.0);
        assert_ne!(before, s.color[1]);
    }

    #[test]
    fn active_file_is_tinted_with_the_focus_colour() {
        let mut s = loaded(4);
        s.set_active(Some("n000002.md"));
        assert_eq!(s.active, Some(2));
        settle(&mut s);
        let want = s.palette.focused;
        assert!((s.color[2][2] - want.b).abs() < 2.0 / 255.0);
        s.set_active(None);
        assert_eq!(s.active, None);
        s.set_active(Some("missing.md"));
        assert_eq!(s.active, None);
    }

    #[test]
    fn kind_and_group_colours() {
        let mut s = Scene::new();
        let mut g = chain(5);
        g.nodes[0].kind = NodeKind::Tag;
        g.nodes[1].kind = NodeKind::Unresolved;
        g.nodes[2].kind = NodeKind::Attachment;
        g.nodes[3].kind = NodeKind::Code;
        g.nodes[4].group = Some(1);
        s.apply(Built::global(g));
        let p = s.palette.clone();
        assert_eq!(s.target_color(0), p.tag);
        assert_eq!(s.target_color(1), p.unresolved);
        assert_eq!(s.target_color(2), p.attachment);
        assert_eq!(s.target_color(3), p.code);
        assert_eq!(s.target_color(4), p.group(1));
        s.style.rule_colors = vec![None, Some(Rgba::new(1.0, 0.0, 0.0, 1.0))];
        assert_eq!(s.target_color(4), Rgba::new(1.0, 0.0, 0.0, 1.0));
        // groups never recolour tags or missing targets
        s.graph.nodes[0].group = Some(0);
        assert_eq!(s.target_color(0), p.tag);
        assert!((s.base_alpha(1) - 0.65).abs() < 1e-3);
        assert_eq!(s.base_alpha(0), 1.0);
    }

    #[test]
    fn updates_keep_positions_of_unaffected_nodes() {
        let mut s = loaded(20);
        let before: Vec<(String, f32, f32)> = (0..20)
            .map(|i| {
                (
                    s.graph.nodes[i].id.to_string(),
                    s.layout.x()[i],
                    s.layout.y()[i],
                )
            })
            .collect();
        // drop the last node and add a branch from node 3
        let mut pairs: Vec<(u32, u32)> = (0..18u32).map(|i| (i, i + 1)).collect();
        pairs.push((3, 19));
        let mut g = Graph::from_pairs(20, &pairs);
        g.nodes.truncate(20);
        let report = s.apply(Built::global(g));
        assert!(report.structural);
        assert!(!report.replaced);
        for (id, x, y) in &before[..19] {
            let i = s.layout.index_of(id).expect("survivor");
            assert_eq!((s.layout.x()[i], s.layout.y()[i]), (*x, *y), "{id} moved");
        }
        assert!(!s.layout.settled(), "a structural edit reheats");
    }

    #[test]
    fn removed_nodes_leave_a_fading_ghost() {
        let mut s = loaded(10);
        s.apply(Built::global(chain(8)));
        assert_eq!(s.ghosts.len(), 2);
        assert!(s.ghosts.iter().all(|g| g.alpha > 0.9));
        settle(&mut s);
        assert!(s.ghosts.is_empty());
    }

    #[test]
    fn reduced_motion_skips_ghosts_and_fade_in() {
        let mut s = Scene::new();
        s.set_style(Style {
            reduce_motion: true,
            ..Style::default()
        });
        s.apply(Built::global(chain(6)));
        assert!(s.alpha.iter().all(|a| *a == 1.0));
        s.apply(Built::global(chain(4)));
        assert!(s.ghosts.is_empty());
    }

    #[test]
    fn new_nodes_fade_in_and_survivors_keep_their_appearance() {
        let mut s = loaded(5);
        s.set_hover(Some(0));
        settle(&mut s);
        let dim = s.alpha[4];
        assert!((dim - HOVER_DIM).abs() < 2e-3);
        s.apply(Built::global(chain(7)));
        assert_eq!(s.alpha[4], dim, "survivor keeps its eased opacity");
        assert_eq!(s.alpha[6], 0.0, "new node starts invisible");
        assert_eq!(s.hover, Some(0), "hover survives an update");
    }

    #[test]
    fn replacing_everything_recentres_the_camera() {
        let mut s = loaded(5);
        s.cam.cam.pan_x = 5.0;
        let other = Graph::from_pairs(3, &[(0, 1)]);
        let mut nodes = other.nodes.clone();
        for n in &mut nodes {
            n.id = Arc::from(format!("other-{}", n.id));
        }
        let renamed = Graph { nodes, ..other };
        let r = s.apply(Built::global(renamed));
        assert!(r.replaced);
        assert_eq!(s.cam.cam.pan_x, 400.0);
    }

    #[test]
    fn picking_prefers_the_top_node_and_honours_slack() {
        let mut s = loaded(3);
        let (x, y, r) = s.screen(1);
        assert_eq!(s.pick(x, y, 0.0), Some(1));
        assert_eq!(s.pick(x + r + 3.0, y, 0.0).filter(|i| *i == 1), None);
        assert_eq!(s.pick(x + r + 3.0, y, 4.0), Some(1));
        assert_eq!(s.pick(-1e6, -1e6, 0.0), None);
        // stacked nodes: the later one is drawn on top
        s.layout.set_position(0, s.layout.x()[2], s.layout.y()[2]);
        let (x2, y2, _) = s.screen(2);
        assert_eq!(s.pick(x2, y2, 0.0), Some(2));
        // a highlighted node wins over those above it
        s.set_hover(Some(0));
        assert_eq!(s.pick(x2, y2, 0.0), Some(0));
    }

    #[test]
    fn hover_sticks_while_the_pointer_stays_near() {
        let mut s = loaded(3);
        let (x, y, r) = s.screen(1);
        assert!(s.update_hover(x, y));
        assert_eq!(s.hover, Some(1));
        assert!(!s.update_hover(x + r + 1.0, y), "slack keeps the hover");
        assert!(s.update_hover(x + 500.0, y + 500.0));
        assert_eq!(s.hover, None);
    }

    #[test]
    fn dragging_pins_the_node_and_heats_the_layout() {
        let mut s = loaded(6);
        assert!(s.layout.settled());
        s.drag_to(3, 120.0, -40.0);
        assert_eq!(s.drag, Some(3));
        assert_eq!(s.highlight(), Some(3));
        assert!(!s.layout.settled());
        for _ in 0..30 {
            s.advance(1.0 / 60.0);
        }
        assert_eq!((s.layout.x()[3], s.layout.y()[3]), (120.0, -40.0));
        s.end_drag();
        assert_eq!(s.drag, None);
        settle(&mut s);
    }

    #[test]
    fn local_graphs_weight_markers_by_layer() {
        let g = chain(4);
        let layers = vec![0u8, 1, 2, 3];
        let mut s = Scene::new();
        s.apply(Built {
            graph: g,
            layers: Some(layers),
            focus: Some(0),
        });
        let r = s.layout.radius();
        assert!(r[0] > r[1] && r[1] > r[2] && r[2] > r[3]);
        assert_eq!(s.focus, Some(0));
    }

    #[test]
    fn display_multipliers_ease_and_resize_markers() {
        let mut s = loaded(4);
        let r0 = s.layout.radius()[0];
        let mut style = s.style.clone();
        style.node_size = 2.0;
        s.set_style(style);
        assert!(s.needs_frames());
        settle(&mut s);
        assert!((s.disp.node - 2.0).abs() < 1e-9);
        assert!(s.layout.radius()[0] > r0 * 1.5);
    }

    #[test]
    fn theme_changes_blend_over_a_fraction_of_a_second() {
        let mut s = loaded(3);
        let mut next = Palette::fallback();
        next.background = Rgba::new(1.0, 1.0, 1.0, 1.0);
        next.fill = Rgba::new(0.1, 0.1, 0.1, 1.0);
        s.set_palette(next.clone());
        assert!(s.needs_frames());
        s.advance(0.09);
        assert!(s.palette.background.r > 0.2 && s.palette.background.r < 0.99);
        settle(&mut s);
        assert_eq!(s.palette.background, next.background);
        // nodes follow the new fill colour
        assert!((s.color[1][0] - 0.1).abs() < 2.0 / 255.0);
    }

    #[test]
    fn restart_layout_places_nodes_afresh() {
        let mut s = loaded(8);
        let mut fresh = Scene::new();
        fresh.apply(Built::global(chain(8)));
        s.layout.set_position(0, 999.0, 999.0);
        s.restart_layout();
        assert_eq!(s.len(), 8);
        assert_eq!(s.layout.x(), fresh.layout.x(), "deterministic placement");
        assert!(!s.layout.settled());
    }

    #[test]
    fn frame_cost_is_bounded_when_the_clock_stalls() {
        let mut s = Scene::new();
        s.apply(Built::global(chain(30)));
        let t0 = s.layout.ticks();
        s.advance(5.0);
        assert!(s.layout.ticks() - t0 <= 3);
    }
}
