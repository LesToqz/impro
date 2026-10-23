//! Behaviour of the view: builds, live updates, preferences, frames.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;

use super::draw::PaintOptions;
use super::host::{BuildRequest, Mode, Output, Update};
use super::menu::{ContextAction, GraphType};
use super::palette::{Palette, Rgba};
use super::panel::Change;
use super::scene::{Built, Style};
use super::timelapse::Plan;
use super::widget::{Event, FrameStats, GraphSummary, Inner, OpenMode, Replay};
use crate::filter::GraphOptions;
use crate::geometry::Camera;
use crate::index::{kind_of, FileKind, SKIP_DIRS};
use crate::prefs::{self, GraphPrefs};

/// Wait before options typed into the panel rebuild the graph.
const OPTIONS_DELAY_MS: u32 = 80;
/// Wait before file changes rebuild the graph.
const FILE_DELAY_MS: u32 = 60;
/// An unsaved buffer is re-indexed this long after its last change.
pub const BUFFER_DELAY_MS: u32 = 400;
/// Preferences are written this long after the last change.
const SAVE_DELAY_MS: u32 = 250;
/// The zoom is remembered this long after it stopped changing.
const SCALE_DELAY_MS: u32 = 500;
/// Graph size at which the file cap notice appears.
const REPLAY_RATE_FPS: f64 = 30.0;

fn cancel(slot: &std::cell::RefCell<Option<glib::SourceId>>) {
    if let Some(id) = slot.borrow_mut().take() {
        id.remove();
    }
}

impl Inner {
    // ---- worker events --------------------------------------------------------

    pub(super) fn on_event(self: &Rc<Self>, event: Event) {
        if self.closed.get() {
            return;
        }
        match event {
            Event::SaveFailed => {
                self.save_failed.set(true);
                self.emit_error("Graph preferences could not be saved");
            }
            Event::Built(out) => self.on_built(*out),
        }
    }

    fn on_built(self: &Rc<Self>, out: Output) {
        if out.generation != self.generation.get() {
            return;
        }
        match out.result {
            Err(message) => self.emit_error(&message),
            Ok(built) => {
                self.stop_replay();
                self.apply_built(built, out.files, out.elapsed_ms);
            }
        }
    }

    fn apply_built(self: &Rc<Self>, built: Built, files: usize, build_ms: f64) {
        let truncated = built.graph.truncated;
        let (nodes, lines) = {
            let mut scene = self.scene.borrow_mut();
            scene.apply(built);
            (scene.len(), scene.lines.len())
        };
        self.has_data.set(true);
        *self.summary.borrow_mut() = GraphSummary {
            nodes,
            lines,
            truncated,
            files_indexed: files,
            build_ms,
        };
        self.update_status();
        self.kick();
        let summary = self.summary.borrow().clone();
        for f in self.handlers.updated.borrow().clone() {
            f(&summary);
        }
    }

    /// Sends a build request with the current mode and options now.
    pub(super) fn request_build_now(self: &Rc<Self>) {
        cancel(&self.build_timer);
        if self.closed.get() {
            return;
        }
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        let prefs = self.prefs.borrow().clone();
        let mode = match self.graph_type.get() {
            GraphType::Global => Mode::Global,
            GraphType::Note => Mode::Notes,
            GraphType::Local => match self.local_path.borrow().clone() {
                Some(path) => Mode::Local {
                    path,
                    depth: prefs.local_depth,
                    incoming: prefs.local_incoming,
                    outgoing: prefs.local_outgoing,
                },
                None => Mode::Empty,
            },
        };
        self.host.build(BuildRequest {
            generation,
            mode,
            options: GraphOptions::from_prefs(&prefs),
        });
    }

    /// Coalesces bursts of changes into one request.
    pub(super) fn schedule_build(self: &Rc<Self>, delay_ms: u32) {
        if delay_ms == 0 {
            self.request_build_now();
            return;
        }
        cancel(&self.build_timer);
        let weak = self.weak();
        let id = glib::timeout_add_local_once(Duration::from_millis(delay_ms as u64), move || {
            if let Some(inner) = weak.upgrade() {
                inner.build_timer.borrow_mut().take();
                inner.request_build_now();
            }
        });
        *self.build_timer.borrow_mut() = Some(id);
    }

    // ---- status ---------------------------------------------------------------

    pub(super) fn update_status(&self) {
        let nodes = self.scene.borrow().len();
        let text = if !self.has_data.get() {
            Some("Mapping this project…".to_string())
        } else if nodes == 0 {
            Some(self.empty_message())
        } else {
            None
        };
        match text {
            Some(t) => {
                self.status.set_text(&t);
                self.status.show();
            }
            None => self.status.hide(),
        }
        let truncated = self.summary.borrow().truncated;
        if truncated && nodes > 0 {
            self.notice.set_text(&format!(
                "View limited to {nodes} files with the most connections"
            ));
            self.notice.show();
        } else {
            self.notice.hide();
        }
    }

    fn empty_message(&self) -> String {
        match self.graph_type.get() {
            GraphType::Local if self.local_path.borrow().is_none() => {
                "Open a file to see its local graph".to_string()
            }
            GraphType::Note => {
                let root = self.root.borrow().clone();
                if impro_core::notes::notes_dir(root).is_dir() {
                    "No notes in the notes folder".to_string()
                } else {
                    "No notes yet. Create a note to see the note graph".to_string()
                }
            }
            _ => "No files match this view".to_string(),
        }
    }

    // ---- mode -----------------------------------------------------------------

    pub(super) fn set_graph_type(self: &Rc<Self>, t: GraphType) {
        let before = (self.graph_type.get(), self.local_path.borrow().clone());
        self.graph_type.set(t);
        *self.local_path.borrow_mut() = match t {
            GraphType::Local => self.active_path.borrow().clone(),
            _ => None,
        };
        self.stop_replay();
        let local = t == GraphType::Local;
        self.panel.set_local(local);
        self.panel.set_graph_type(t);
        if local && before != (t, self.local_path.borrow().clone()) {
            self.recentre();
        }
        self.update_status();
        self.request_build_now();
    }

    pub(super) fn open_local(self: &Rc<Self>, path: &Path, depth: Option<u32>) {
        let path = absolute(path);
        *self.local_path.borrow_mut() = Some(path.clone());
        *self.active_path.borrow_mut() = Some(path.clone());
        let id = self.node_id(&path);
        self.scene.borrow_mut().set_active(id.as_deref());
        self.graph_type.set(GraphType::Local);
        self.stop_replay();
        if let Some(d) = depth {
            let d = d.clamp(1, 6);
            let changed = {
                let mut p = self.prefs.borrow_mut();
                let changed = p.local_depth != d;
                p.local_depth = d;
                changed
            };
            if changed {
                self.schedule_save();
            }
        }
        let prefs = self.prefs.borrow().clone();
        self.panel.sync(&prefs, true, GraphType::Local);
        self.recentre();
        self.update_status();
        self.request_build_now();
    }

    fn recentre(self: &Rc<Self>) {
        let mut scene = self.scene.borrow_mut();
        let (w, h) = scene.size;
        scene.cam.cam.pan_x = w / 2.0;
        scene.cam.cam.pan_y = h / 2.0;
        scene.cam.vx = 0.0;
        scene.cam.vy = 0.0;
        scene.cam.stop_tracking();
        drop(scene);
        self.kick();
    }

    // ---- live updates -----------------------------------------------------------

    /// The project-relative id of `path`, if it is inside the root.
    pub(super) fn node_id(&self, path: &Path) -> Option<String> {
        let root = self.root.borrow();
        let abs = absolute(path);
        let rel = abs.strip_prefix(&*root).ok()?;
        let mut parts = Vec::new();
        for c in rel.components() {
            match c {
                std::path::Component::Normal(s) => parts.push(s.to_str()?),
                _ => return None,
            }
        }
        (!parts.is_empty()).then(|| parts.join("/"))
    }

    /// Whether the graph can show `path` (inside the root, not ignored).
    fn relevant(&self, path: &Path) -> bool {
        match self.node_id(path) {
            Some(id) => {
                let dirs: Vec<&str> = id.split('/').collect();
                !dirs[..dirs.len() - 1].iter().any(|d| SKIP_DIRS.contains(d))
            }
            None => false,
        }
    }

    pub(super) fn set_root(self: &Rc<Self>, path: &Path) {
        let root = absolute(path);
        self.cancel_buffers();
        self.stop_replay();
        *self.root.borrow_mut() = root.clone();
        *self.local_path.borrow_mut() = None;
        *self.active_path.borrow_mut() = None;
        self.scene.borrow_mut().set_active(None);
        self.host.scan(&root);
        self.update_status();
        self.request_build_now();
    }

    pub(super) fn rebuild(self: &Rc<Self>) {
        let root = self.root.borrow().clone();
        self.host.scan(&root);
        self.request_build_now();
    }

    pub(super) fn file_saved(self: &Rc<Self>, path: &Path) {
        if !self.relevant(path) {
            return;
        }
        let path = absolute(path);
        self.drop_buffer(&path);
        self.host.update(Update::Text(path, None));
        self.schedule_build(FILE_DELAY_MS);
    }

    pub(super) fn buffer_changed(self: &Rc<Self>, path: &Path, text: Option<String>) {
        if !self.relevant(path) {
            return;
        }
        let path = absolute(path);
        let id = self.node_id(&path).unwrap_or_default();
        if kind_of(&id) == FileKind::Attachment {
            return;
        }
        self.buffer_texts.borrow_mut().insert(path.clone(), text);
        if let Some(old) = self.buffer_timers.borrow_mut().remove(&path) {
            old.remove();
        }
        let weak = self.weak();
        let key = path.clone();
        let id = glib::timeout_add_local_once(
            Duration::from_millis(BUFFER_DELAY_MS as u64),
            move || {
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                inner.buffer_timers.borrow_mut().remove(&key);
                let text = inner.buffer_texts.borrow_mut().remove(&key);
                if let Some(text) = text {
                    inner.host.update(Update::Text(key.clone(), text));
                    inner.schedule_build(0);
                }
            },
        );
        self.buffer_timers.borrow_mut().insert(path, id);
    }

    fn drop_buffer(&self, path: &Path) {
        if let Some(t) = self.buffer_timers.borrow_mut().remove(path) {
            t.remove();
        }
        self.buffer_texts.borrow_mut().remove(path);
    }

    fn cancel_buffers(&self) {
        for (_, t) in self.buffer_timers.borrow_mut().drain() {
            t.remove();
        }
        self.buffer_texts.borrow_mut().clear();
    }

    pub(super) fn path_renamed(self: &Rc<Self>, from: &Path, to: &Path) {
        let (from, to) = (absolute(from), absolute(to));
        let stale: Vec<PathBuf> = self
            .buffer_timers
            .borrow()
            .keys()
            .filter(|p| p.starts_with(&from))
            .cloned()
            .collect();
        for p in stale {
            self.drop_buffer(&p);
        }
        if !self.relevant(&from) && !self.relevant(&to) {
            return;
        }
        // an active or local file that moved keeps being followed
        for slot in [&self.active_path, &self.local_path] {
            let moved = slot
                .borrow()
                .as_ref()
                .and_then(|p| p.strip_prefix(&from).ok().map(|rest| to.join(rest)));
            if moved.is_some() {
                *slot.borrow_mut() = moved;
            }
        }
        self.host.update(Update::Rename(from, to));
        self.schedule_build(FILE_DELAY_MS);
    }

    pub(super) fn path_deleted(self: &Rc<Self>, path: &Path) {
        let path = absolute(path);
        let stale: Vec<PathBuf> = self
            .buffer_timers
            .borrow()
            .keys()
            .filter(|p| p.starts_with(&path))
            .cloned()
            .collect();
        for p in stale {
            self.drop_buffer(&p);
        }
        if !self.relevant(&path) {
            return;
        }
        self.host.update(Update::Remove(path));
        self.schedule_build(FILE_DELAY_MS);
    }

    pub(super) fn set_active_file(self: &Rc<Self>, path: Option<&Path>) {
        let abs = path.map(absolute);
        let id = abs.as_deref().and_then(|p| self.node_id(p));
        *self.active_path.borrow_mut() = abs.clone();
        self.scene.borrow_mut().set_active(id.as_deref());
        if self.graph_type.get() == GraphType::Local {
            let changed = *self.local_path.borrow() != abs;
            *self.local_path.borrow_mut() = abs;
            if changed {
                self.recentre();
                self.update_status();
                self.request_build_now();
            }
        }
        self.kick();
    }

    // ---- camera -------------------------------------------------------------

    pub(super) fn reset_camera(self: &Rc<Self>) {
        self.stop_replay();
        {
            let mut scene = self.scene.borrow_mut();
            let (w, h) = scene.size;
            scene.cam.reset(w, h);
            scene.restart_layout();
        }
        let changed = {
            let mut p = self.prefs.borrow_mut();
            let changed = (p.scale - 1.0).abs() > 1e-9;
            p.scale = 1.0;
            changed
        };
        if changed {
            self.schedule_save();
        }
        self.kick();
    }

    pub(super) fn fit_camera(self: &Rc<Self>) {
        {
            let mut scene = self.scene.borrow_mut();
            let Some(bounds) = scene.layout.bounds() else {
                return;
            };
            let (w, h) = scene.size;
            let mut cam = Camera::default();
            cam.fit(bounds, w, h, 40.0);
            scene.cam.cam = cam;
            scene.cam.target_zoom = cam.zoom;
            scene.cam.vx = 0.0;
            scene.cam.vy = 0.0;
        }
        self.kick();
    }

    pub(super) fn focus_file(self: &Rc<Self>, path: &Path) -> bool {
        let Some(id) = self.node_id(path) else {
            return false;
        };
        let found = {
            let mut scene = self.scene.borrow_mut();
            match scene.graph.index_of(&id) {
                Some(i) => {
                    scene.cam.track_node(i);
                    true
                }
                None => false,
            }
        };
        if found {
            self.kick();
        }
        found
    }

    // ---- history replay -------------------------------------------------------

    pub(super) fn start_replay(self: &Rc<Self>) {
        if self.graph_type.get() == GraphType::Local {
            return;
        }
        self.stop_replay();
        let full = self.scene.borrow().graph.clone();
        if full.is_empty() {
            return;
        }
        let plan = Plan::new(&full, self.prefs.borrow().show_orphans);
        if plan.total() == 0 {
            return;
        }
        *self.replay.borrow_mut() = Some(Replay {
            plan,
            full,
            started: Instant::now(),
            shown: 0,
        });
        self.panel.set_replaying(true);
        self.kick();
    }

    pub(super) fn stop_replay(&self) {
        if self.replay.borrow_mut().take().is_some() {
            self.panel.set_replaying(false);
        }
    }

    /// Reveals more of the history; returns true while it runs.
    fn step_replay(self: &Rc<Self>) -> bool {
        let next = {
            let mut guard = self.replay.borrow_mut();
            let Some(r) = guard.as_mut() else {
                return false;
            };
            let n = r.plan.progress(r.started.elapsed().as_secs_f64());
            if n == r.shown {
                return true;
            }
            r.shown = n;
            if n > r.plan.total() {
                Some((r.full.clone(), true))
            } else {
                Some((r.plan.subset(&r.full, n), false))
            }
        };
        if let Some((graph, done)) = next {
            {
                let mut scene = self.scene.borrow_mut();
                scene.apply(Built::global(graph));
                let mut summary = self.summary.borrow_mut();
                summary.nodes = scene.len();
                summary.lines = scene.lines.len();
            }
            self.update_status();
            if done {
                self.stop_replay();
                return false;
            }
        }
        true
    }

    // ---- appearance -------------------------------------------------------------

    pub(super) fn effective_reduce(&self) -> bool {
        let p = self.prefs.borrow();
        self.effective_reduce_with(&p)
    }

    fn effective_reduce_with(&self, p: &GraphPrefs) -> bool {
        p.reduce_motion
            || self.host_reduce.get()
            || std::env::var("IMPRO_REDUCE_MOTION").is_ok_and(|v| v == "1")
            || gtk::Settings::default().is_some_and(|s| !s.is_gtk_enable_animations())
    }

    pub(super) fn style_from(&self, p: &GraphPrefs) -> Style {
        Style {
            show_arrow: p.show_arrow,
            node_size: p.node_size_multiplier,
            line_width: p.line_size_multiplier,
            label_detail: p.text_fade_multiplier,
            rule_colors: p
                .color_groups
                .iter()
                .map(|g| {
                    let (r, gr, b, _) = g.color.rgba();
                    Some(Rgba::from_u8(r, gr, b, 1.0))
                })
                .collect(),
            reduce_motion: self.effective_reduce_with(p),
        }
    }

    pub(super) fn preset_colors(&self) -> Vec<Rgba> {
        self.scene.borrow().palette.groups.clone()
    }

    pub(super) fn set_theme(self: &Rc<Self>, ui: &impro_core::themes::UiColors, accent: &str) {
        *self.ui.borrow_mut() = ui.clone();
        *self.accent.borrow_mut() = accent.to_string();
        super::css::install(ui, accent);
        let mut palette = Palette::from_theme(ui, accent, impro_core::settings::accent_presets());
        palette.font = self.font.borrow().clone().unwrap_or_else(|| "sans".into());
        let presets = palette.groups.clone();
        self.scene.borrow_mut().set_palette(palette);
        self.panel.set_presets(presets);
        self.kick();
        self.canvas.queue_draw();
    }

    pub(super) fn set_font(self: &Rc<Self>, family: Option<&str>) {
        *self.font.borrow_mut() = family.map(str::to_string);
        self.painter
            .borrow_mut()
            .labels
            .set_family(family.unwrap_or("sans"));
        self.canvas.queue_draw();
    }

    pub(super) fn set_reduce_motion(self: &Rc<Self>, reduce: bool) {
        self.host_reduce.set(reduce);
        let changed = {
            let mut p = self.prefs.borrow_mut();
            let changed = p.reduce_motion != reduce;
            p.reduce_motion = reduce;
            changed
        };
        self.apply_style();
        if changed {
            self.schedule_save();
        }
    }

    pub(super) fn apply_style(self: &Rc<Self>) {
        let prefs = self.prefs.borrow().clone();
        let style = self.style_from(&prefs);
        let animated = !style.reduce_motion;
        {
            let mut scene = self.scene.borrow_mut();
            scene.set_style(style);
            scene.set_forces(prefs.forces());
        }
        self.panel.set_animated(animated);
        self.kick();
    }

    // ---- panel ------------------------------------------------------------------

    pub(super) fn panel_change(self: &Rc<Self>, change: Change) {
        let mut rebuild: Option<u32> = None;
        let mut style = false;
        let mut forces = false;
        let mut resync = false;
        {
            let mut p = self.prefs.borrow_mut();
            match &change {
                Change::Search { text, now } => {
                    p.search = text.clone();
                    rebuild = Some(if *now { 0 } else { OPTIONS_DELAY_MS });
                }
                Change::Toggle { key, value } => {
                    let v = *value;
                    match *key {
                        "showTags" => p.show_tags = v,
                        "showAttachments" => p.show_attachments = v,
                        "hideUnresolved" => p.hide_unresolved = v,
                        "showOrphans" => p.show_orphans = v,
                        "showCode" => p.show_code = v,
                        "showArrow" => {
                            p.show_arrow = v;
                            style = true;
                        }
                        "localIncoming" => p.local_incoming = v,
                        "localOutgoing" => p.local_outgoing = v,
                        _ => {}
                    }
                    if *key != "showArrow" {
                        rebuild = Some(OPTIONS_DELAY_MS);
                    }
                }
                Change::Slider { key, value } => {
                    let (lo, hi) = prefs::limits(key).unwrap_or((f64::MIN, f64::MAX));
                    let v = value.clamp(lo, hi);
                    match *key {
                        "textFadeMultiplier" => {
                            p.text_fade_multiplier = v;
                            style = true;
                        }
                        "nodeSizeMultiplier" => {
                            p.node_size_multiplier = v;
                            style = true;
                        }
                        "lineSizeMultiplier" => {
                            p.line_size_multiplier = v;
                            style = true;
                        }
                        "centerStrength" => {
                            p.center_strength = v;
                            forces = true;
                        }
                        "repelStrength" => {
                            p.repel_strength = v;
                            forces = true;
                        }
                        "linkStrength" => {
                            p.link_strength = v;
                            forces = true;
                        }
                        "linkDistance" => {
                            p.link_distance = v;
                            forces = true;
                        }
                        "localDepth" => {
                            p.local_depth = v.round() as u32;
                            rebuild = Some(OPTIONS_DELAY_MS);
                        }
                        _ => {}
                    }
                }
                Change::AddRule => {
                    let dark = self.scene.borrow().palette.dark;
                    if p.add_color_rule("", dark) {
                        rebuild = Some(OPTIONS_DELAY_MS);
                        style = true;
                        resync = true;
                    }
                }
                Change::RemoveRule(i) => {
                    if *i < p.color_groups.len() {
                        p.color_groups.remove(*i);
                        rebuild = Some(OPTIONS_DELAY_MS);
                        style = true;
                        resync = true;
                    }
                }
                Change::RuleQuery(i, q) => {
                    if let Some(rule) = p.color_groups.get_mut(*i) {
                        if rule.query != *q {
                            rule.query = q.clone();
                            rebuild = Some(OPTIONS_DELAY_MS);
                        }
                    }
                }
                Change::RuleColor(i, rgb) => {
                    if let Some(rule) = p.color_groups.get_mut(*i) {
                        // saved colours keep their form: hex stays hex
                        rule.color.rgb = *rgb & 0xff_ffff;
                        if rule.color.packed {
                            rule.color.alpha = 1.0;
                        }
                        style = true;
                        resync = true;
                    }
                }
                Change::Collapse { key, collapsed } => match *key {
                    "collapse-filter" => p.collapse_filter = *collapsed,
                    "collapse-color-groups" => p.collapse_color_groups = *collapsed,
                    "collapse-display" => p.collapse_display = *collapsed,
                    "collapse-forces" => p.collapse_forces = *collapsed,
                    _ => {}
                },
                Change::Reset => {
                    let local = self.graph_type.get() == GraphType::Local;
                    p.reset(local);
                    rebuild = Some(0);
                    style = true;
                    forces = true;
                    resync = true;
                }
                Change::Open(_) | Change::Replay | Change::GraphType(_) => {}
            }
        }
        let save_delay = match &change {
            Change::Reset | Change::RuleColor(..) => OPTIONS_DELAY_MS,
            _ => rebuild.unwrap_or(SAVE_DELAY_MS),
        };
        match change {
            Change::Open(open) => {
                self.set_panel_open(open, true);
                return;
            }
            Change::Replay => {
                self.start_replay();
                return;
            }
            Change::GraphType(t) => {
                self.set_graph_type(t);
                return;
            }
            _ => {}
        }
        if style || forces {
            self.apply_style();
        }
        if forces {
            self.scene.borrow_mut().layout.reheat();
            self.kick();
        }
        if resync {
            let prefs = self.prefs.borrow().clone();
            let local = self.graph_type.get() == GraphType::Local;
            self.panel.sync(&prefs, local, self.graph_type.get());
        }
        if let Some(delay) = rebuild {
            self.stop_replay();
            self.schedule_build(delay);
        }
        self.schedule_save_after(save_delay);
    }

    pub(super) fn set_panel_open(self: &Rc<Self>, open: bool, user: bool) {
        self.panel.set_open(open);
        let changed = {
            let mut p = self.prefs.borrow_mut();
            let changed = p.close == open;
            p.close = !open;
            changed
        };
        if changed && user {
            self.schedule_save();
        }
        if !open && self.panel.is_open() {
            return;
        }
        self.canvas.queue_draw();
    }

    pub(super) fn apply_external_prefs(self: &Rc<Self>, new: GraphPrefs) {
        let old = self.prefs.borrow().clone();
        if old == new {
            return;
        }
        let rebuild = old.search != new.search
            || old.show_tags != new.show_tags
            || old.show_attachments != new.show_attachments
            || old.hide_unresolved != new.hide_unresolved
            || old.show_orphans != new.show_orphans
            || old.show_code != new.show_code
            || old.color_groups != new.color_groups
            || old.local_depth != new.local_depth
            || old.local_incoming != new.local_incoming
            || old.local_outgoing != new.local_outgoing;
        let forces = old.forces() != new.forces();
        let zoom = (old.scale - new.scale).abs() > 1e-9;
        *self.prefs.borrow_mut() = new.clone();
        self.apply_style();
        if forces {
            self.scene.borrow_mut().layout.reheat();
        }
        if zoom {
            let mut scene = self.scene.borrow_mut();
            let (w, h) = scene.size;
            scene.cam.set_zoom_target(new.scale, w, h);
        }
        let local = self.graph_type.get() == GraphType::Local;
        self.panel.sync(&new, local, self.graph_type.get());
        self.panel.set_open(!new.close);
        if rebuild {
            self.schedule_build(OPTIONS_DELAY_MS);
        }
        self.kick();
    }

    // ---- persistence ------------------------------------------------------------------

    pub(super) fn schedule_save(self: &Rc<Self>) {
        self.schedule_save_after(SAVE_DELAY_MS);
    }

    fn schedule_save_after(self: &Rc<Self>, delay_ms: u32) {
        cancel(&self.save_timer);
        if delay_ms == 0 {
            self.save_now();
            return;
        }
        let weak = self.weak();
        let id = glib::timeout_add_local_once(Duration::from_millis(delay_ms as u64), move || {
            if let Some(inner) = weak.upgrade() {
                inner.save_timer.borrow_mut().take();
                inner.save_now();
            }
        });
        *self.save_timer.borrow_mut() = Some(id);
    }

    fn save_now(&self) {
        let value = prefs::normalize_value(&self.prefs.borrow().to_value());
        self.writer.save(value.clone());
        for f in self.handlers.settings_changed.borrow().clone() {
            f(&value);
        }
    }

    pub(super) fn flush_prefs(&self) {
        if self.save_timer.borrow().is_some() {
            cancel(&self.save_timer);
            self.save_now();
        }
        cancel(&self.scale_timer);
        self.writer.flush();
    }

    fn schedule_scale(self: &Rc<Self>) {
        cancel(&self.scale_timer);
        let weak = self.weak();
        let id =
            glib::timeout_add_local_once(Duration::from_millis(SCALE_DELAY_MS as u64), move || {
                if let Some(inner) = weak.upgrade() {
                    inner.scale_timer.borrow_mut().take();
                    let target = inner.scene.borrow().cam.target_zoom;
                    let changed = {
                        let mut p = inner.prefs.borrow_mut();
                        let changed = (p.scale - target).abs() > 1e-6;
                        p.scale = target;
                        changed
                    };
                    if changed {
                        inner.schedule_save();
                    }
                }
            });
        *self.scale_timer.borrow_mut() = Some(id);
    }

    // ---- frames -------------------------------------------------------------------------

    pub(super) fn resized(self: &Rc<Self>, w: f64, h: f64) {
        self.scene.borrow_mut().set_size(w, h);
        self.kick();
    }

    pub(super) fn draw(&self, area: &gtk::DrawingArea, cr: &cairo::Context) {
        let t0 = Instant::now();
        let (w, h) = (
            area.allocated_width() as f64,
            area.allocated_height() as f64,
        );
        let scale = area.scale_factor() as f64;
        let mut stalled = false;
        if let (Ok(scene), Ok(mut painter)) =
            (self.scene.try_borrow(), self.painter.try_borrow_mut())
        {
            // something changed the scene without asking for frames
            stalled = scene.needs_frames()
                && self.tick.try_borrow().is_ok_and(|t| t.is_none())
                && self.replay.try_borrow().is_ok_and(|r| r.is_none());
            // The frame after the last moving one is drawn at full quality.
            painter.motion = scene.moving();
            painter.paint(
                cr,
                &scene,
                w,
                h,
                PaintOptions {
                    hide_labels: false,
                    scale,
                },
            );
        }
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let mut s = self.stats.get();
        s.draw_ms = if s.frames == 0 {
            ms
        } else {
            s.draw_ms * 0.9 + ms * 0.1
        };
        s.frames += 1;
        self.stats.set(s);
        if stalled {
            if let Some(me) = self.weak().upgrade() {
                me.kick();
            }
        }
    }

    /// Makes sure frames run while something moves.
    pub(super) fn kick(self: &Rc<Self>) {
        if self.closed.get() || self.tick.borrow().is_some() || !self.canvas.is_mapped() {
            return;
        }
        let needs = self.replay.borrow().is_some() || self.scene.borrow().needs_frames();
        if !needs {
            self.canvas.queue_draw();
            return;
        }
        let weak = self.weak();
        let id = self
            .canvas
            .add_tick_callback(move |_, clock| match weak.upgrade() {
                Some(inner) => inner.frame(clock),
                None => glib::ControlFlow::Break,
            });
        *self.tick.borrow_mut() = Some(id);
        let mut s = self.stats.get();
        s.animating = true;
        self.stats.set(s);
    }

    /// Hidden: no frames at all.
    pub(super) fn pause(&self) {
        if let Some(id) = self.tick.borrow_mut().take() {
            id.remove();
        }
        self.last_frame.set(None);
        let mut s = self.stats.get();
        s.animating = false;
        self.stats.set(s);
    }

    fn frame(self: &Rc<Self>, clock: &gdk::FrameClock) -> glib::ControlFlow {
        if self.closed.get() {
            self.tick.borrow_mut().take();
            return glib::ControlFlow::Break;
        }
        let t0 = Instant::now();
        let now = clock.frame_time();
        let dt = match self.last_frame.replace(Some(now)) {
            Some(prev) => ((now - prev) as f64 / 1e6).clamp(0.0, 0.05),
            None => 1.0 / REPLAY_RATE_FPS / 2.0,
        };
        let replaying = self.step_replay();
        let (active, zoom_moved) = {
            let mut scene = self.scene.borrow_mut();
            if self.press.borrow().is_none() {
                if let Some((x, y)) = self.pointer.get() {
                    scene.update_hover(x, y);
                }
            }
            let previous_zoom = scene.cam.cam.zoom;
            let active = scene.advance(dt);
            let moved = previous_zoom != scene.cam.cam.zoom;
            (active, moved)
        };
        if zoom_moved {
            self.schedule_scale();
        }
        self.update_cursor();
        self.canvas.queue_draw();
        let busy = active || replaying;
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let mut s: FrameStats = self.stats.get();
        s.tick_ms = if s.tick_ms == 0.0 {
            ms
        } else {
            s.tick_ms * 0.9 + ms * 0.1
        };
        if busy {
            self.stats.set(s);
            glib::ControlFlow::Continue
        } else {
            s.animating = false;
            self.stats.set(s);
            self.tick.borrow_mut().take();
            self.last_frame.set(None);
            glib::ControlFlow::Break
        }
    }

    // ---- host actions ----------------------------------------------------------------------

    pub(super) fn abs_path_of(&self, id: &str) -> PathBuf {
        self.root.borrow().join(id)
    }

    pub(super) fn emit_open(&self, path: &Path, mode: OpenMode) {
        for f in self.handlers.open_file.borrow().clone() {
            f(path, mode);
        }
    }

    pub(super) fn emit_error(&self, message: &str) {
        let list = self.handlers.error.borrow().clone();
        if list.is_empty() {
            eprintln!("Graph view: {message}");
        }
        for f in list {
            f(message);
        }
    }

    pub(super) fn context_action(&self, action: ContextAction) {
        let Some(id) = self.menu_node.borrow().clone() else {
            return;
        };
        let path = self.abs_path_of(&id);
        match action {
            ContextAction::Open => self.emit_open(&path, OpenMode::Current),
            ContextAction::OpenNewTab => self.emit_open(&path, OpenMode::NewTab),
            ContextAction::OpenRightPane => self.emit_open(&path, OpenMode::RightPane),
            ContextAction::Reveal => {
                for f in self.handlers.reveal.borrow().clone() {
                    f(&path);
                }
            }
            ContextAction::CopyPath => {
                if let Some(text) = path.to_str() {
                    gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(text);
                }
                for f in self.handlers.copy_path.borrow().clone() {
                    f(&path);
                }
            }
        }
    }

    // ---- lifecycle ---------------------------------------------------------------------------

    pub(super) fn shutdown(&self) {
        if self.closed.replace(true) {
            return;
        }
        if let Some(id) = self.tick.borrow_mut().take() {
            id.remove();
        }
        for slot in [&self.build_timer, &self.scale_timer] {
            cancel(slot);
        }
        self.cancel_buffers();
        self.panel.close_popovers();
        self.close_menu();
        self.host.shutdown();
        // changes still waiting for their timer are written now
        if self.save_timer.borrow_mut().take().is_some() {
            self.save_now();
        }
        self.writer.shutdown();
    }
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
