//! `GraphView`: the Graph view widget and its public surface.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Instant;

use gtk::prelude::*;
use serde_json::Value;

use super::draw::Painter;
use super::host::{Host, Output};
use super::menu::GraphType;
use super::panel::Panel;
use super::scene::Scene;
use super::store::{PrefsStore, SettingsStore, Writer};
use super::timelapse::Plan;
use crate::prefs::{self, GraphPrefs};

/// How a file should be opened by the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    /// In the current editor tab (or focus the existing tab).
    Current,
    /// In another tab.
    NewTab,
    /// In the right pane.
    RightPane,
}

/// Construction options.
#[derive(Clone)]
pub struct ViewOptions {
    /// Where preferences are loaded from and saved to.
    pub store: Arc<dyn PrefsStore>,
    /// Start indexing right away.
    pub scan: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        ViewOptions {
            store: Arc::new(SettingsStore),
            scan: true,
        }
    }
}

/// Events from worker threads, delivered on the main loop.
pub(super) enum Event {
    Built(Box<Output>),
    SaveFailed,
}

/// Signal handlers connected by the host.
#[derive(Default)]
pub(super) struct Handlers {
    pub open_file: Slot<OpenHandler>,
    pub search_tag: Slot<dyn Fn(&str)>,
    pub create_note: Slot<dyn Fn(&str)>,
    pub reveal: Slot<dyn Fn(&Path)>,
    pub copy_path: Slot<dyn Fn(&Path)>,
    pub settings_changed: Slot<dyn Fn(&Value)>,
    pub error: Slot<dyn Fn(&str)>,
    pub updated: Slot<dyn Fn(&GraphSummary)>,
}

/// A handler of the open-file signal.
pub(super) type OpenHandler = dyn Fn(&Path, OpenMode);

/// The handlers connected to one signal.
pub(super) type Slot<F> = RefCell<Vec<Rc<F>>>;

/// What the view currently shows (for hosts, tests and diagnostics).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphSummary {
    pub nodes: usize,
    pub lines: usize,
    pub truncated: bool,
    pub files_indexed: usize,
    /// Worker time of the last build in milliseconds.
    pub build_ms: f64,
}

pub(super) enum Press {
    Node {
        id: Arc<str>,
        button: u32,
        x0: f64,
        y0: f64,
        click: bool,
        ctrl: bool,
    },
    Pan {
        x0: f64,
        y0: f64,
        pan0: (f64, f64),
        last_time: u32,
        last_move: u32,
    },
}

pub(super) struct Replay {
    pub plan: Plan,
    pub full: crate::model::Graph,
    pub started: Instant,
    pub shown: usize,
}

/// Measured frame costs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    pub frames: u64,
    /// Smoothed draw time in milliseconds.
    pub draw_ms: f64,
    /// Draw time of the latest frame in milliseconds.
    pub last_draw_ms: f64,
    /// Smoothed tick (simulation + easing) time in milliseconds.
    pub tick_ms: f64,
    /// True while a frame callback is attached.
    pub animating: bool,
}

/// A test hook called after every painted frame.
pub(super) type ProbeFn = Rc<dyn Fn(&Inner)>;

pub(super) struct Inner {
    pub root_box: gtk::Box,
    pub overlay: gtk::Overlay,
    pub canvas: gtk::DrawingArea,
    pub status: gtk::Label,
    pub notice: gtk::Label,
    pub panel: Panel,
    // model
    pub scene: RefCell<Scene>,
    pub painter: RefCell<Painter>,
    pub prefs: RefCell<GraphPrefs>,
    pub root: RefCell<PathBuf>,
    pub graph_type: Cell<GraphType>,
    pub local_path: RefCell<Option<PathBuf>>,
    pub active_path: RefCell<Option<PathBuf>>,
    pub replay: RefCell<Option<Replay>>,
    pub summary: RefCell<GraphSummary>,
    pub host_reduce: Cell<bool>,
    pub font: RefCell<Option<String>>,
    pub accent: RefCell<String>,
    pub ui: RefCell<std::collections::BTreeMap<String, String>>,
    // worker and persistence
    pub host: Host,
    pub writer: Writer,
    pub generation: Cell<u64>,
    pub has_data: Cell<bool>,
    pub save_failed: Cell<bool>,
    // interaction
    pub press: RefCell<Option<Press>>,
    pub pointer: Cell<Option<(f64, f64)>>,
    pub tick: RefCell<Option<gtk::TickCallbackId>>,
    pub last_frame: Cell<Option<i64>>,
    pub menu: RefCell<Option<gtk::Menu>>,
    pub menu_node: RefCell<Option<Arc<str>>>,
    // timers
    pub build_timer: RefCell<Option<glib::SourceId>>,
    pub save_timer: RefCell<Option<glib::SourceId>>,
    pub scale_timer: RefCell<Option<glib::SourceId>>,
    pub buffer_timers: RefCell<HashMap<PathBuf, glib::SourceId>>,
    pub buffer_texts: RefCell<HashMap<PathBuf, Option<String>>>,
    // misc
    pub handlers: Handlers,
    pub closed: Cell<bool>,
    pub stats: Cell<FrameStats>,
    /// Tests: called after every painted frame.
    pub probe: RefCell<Option<ProbeFn>>,
    pub me: RefCell<Weak<Inner>>,
}

/// The Graph view: a native canvas plus the controls panel.
///
/// Cloning gives another handle to the same view. It dereferences to its
/// `gtk::Box`, so it can be packed like any widget.
#[derive(Clone)]
pub struct GraphView {
    pub(super) inner: Rc<Inner>,
}

impl Deref for GraphView {
    type Target = gtk::Box;
    fn deref(&self) -> &gtk::Box {
        &self.inner.root_box
    }
}

impl GraphView {
    /// A view of the project at `root`, with preferences from the settings file.
    pub fn new(root: impl AsRef<Path>) -> GraphView {
        GraphView::with_options(root, ViewOptions::default())
    }

    /// Like [`GraphView::new`] with a custom store (tests, previews).
    pub fn with_options(root: impl AsRef<Path>, options: ViewOptions) -> GraphView {
        super::build::create(root.as_ref(), options)
    }

    pub(super) fn from_inner(inner: Rc<Inner>) -> GraphView {
        GraphView { inner }
    }

    /// The root widget (same as dereferencing).
    pub fn widget(&self) -> &gtk::Box {
        &self.inner.root_box
    }

    /// The drawing surface.
    pub fn canvas(&self) -> &gtk::DrawingArea {
        &self.inner.canvas
    }

    /// The controls panel (inspection and tests).
    pub fn panel(&self) -> &Panel {
        &self.inner.panel
    }

    // ---- signals ----------------------------------------------------------

    /// A file marker was activated or chosen in the context menu.
    pub fn connect_open_file(&self, f: impl Fn(&Path, OpenMode) + 'static) {
        self.inner.handlers.open_file.borrow_mut().push(Rc::new(f));
    }

    /// A tag marker was clicked; the argument is the tag without `#`. Hosts
    /// search with [`tag_query`].
    pub fn connect_search_tag(&self, f: impl Fn(&str) + 'static) {
        self.inner.handlers.search_tag.borrow_mut().push(Rc::new(f));
    }

    /// A missing-target marker was clicked: create a note with this name.
    pub fn connect_create_note(&self, f: impl Fn(&str) + 'static) {
        self.inner
            .handlers
            .create_note
            .borrow_mut()
            .push(Rc::new(f));
    }

    /// "Locate in project tree" was chosen.
    pub fn connect_reveal(&self, f: impl Fn(&Path) + 'static) {
        self.inner.handlers.reveal.borrow_mut().push(Rc::new(f));
    }

    /// "Copy file location" was chosen (the view also copies it to the clipboard).
    pub fn connect_copy_path(&self, f: impl Fn(&Path) + 'static) {
        self.inner.handlers.copy_path.borrow_mut().push(Rc::new(f));
    }

    /// The preferences changed through the panel (the normalised blob, as saved).
    pub fn connect_settings_changed(&self, f: impl Fn(&Value) + 'static) {
        self.inner
            .handlers
            .settings_changed
            .borrow_mut()
            .push(Rc::new(f));
    }

    /// Indexing or saving failed.
    pub fn connect_error(&self, f: impl Fn(&str) + 'static) {
        self.inner.handlers.error.borrow_mut().push(Rc::new(f));
    }

    /// A build was applied.
    pub fn connect_updated(&self, f: impl Fn(&GraphSummary) + 'static) {
        self.inner.handlers.updated.borrow_mut().push(Rc::new(f));
    }

    // ---- graph mode -------------------------------------------------------

    pub fn graph_type(&self) -> GraphType {
        self.inner.graph_type.get()
    }

    /// Switches between the whole project, the active file's neighbourhood
    /// and the notes folder.
    pub fn set_graph_type(&self, t: GraphType) {
        self.inner.set_graph_type(t);
    }

    /// Back to the global graph.
    pub fn open_global(&self) {
        self.inner.set_graph_type(GraphType::Global);
    }

    /// The neighbourhood of `path`; it follows [`GraphView::set_active_file`]
    /// until another mode is chosen.
    pub fn open_local(&self, path: &Path) {
        self.inner.open_local(path, None);
    }

    /// Like [`GraphView::open_local`] with an explicit number of hops (saved).
    pub fn open_local_with_depth(&self, path: &Path, depth: u32) {
        self.inner.open_local(path, Some(depth));
    }

    // ---- live updates -----------------------------------------------------

    /// Open Folder: index another project. The previous graph stays until the
    /// new one is ready.
    pub fn set_root(&self, path: &Path) {
        self.inner.set_root(path);
    }

    pub fn root(&self) -> PathBuf {
        self.inner.root.borrow().clone()
    }

    /// A file was saved (or changed on disk): re-read it.
    pub fn file_saved(&self, path: &Path) {
        self.inner.file_saved(path);
    }

    /// An editor buffer changed. `Some(text)` is the unsaved content, `None`
    /// returns to the file on disk. Applied 400 ms after the last change.
    pub fn buffer_changed(&self, path: &Path, text: Option<String>) {
        self.inner.buffer_changed(path, text);
    }

    /// A file or folder was renamed.
    pub fn path_renamed(&self, from: &Path, to: &Path) {
        self.inner.path_renamed(from, to);
    }

    /// A file or folder was deleted.
    pub fn path_deleted(&self, path: &Path) {
        self.inner.path_deleted(path);
    }

    /// The editor's current file: highlighted in the graph, and followed by
    /// the local graph.
    pub fn set_active_file(&self, path: Option<&Path>) {
        self.inner.set_active_file(path);
    }

    /// Re-scan the project and rebuild.
    pub fn rebuild(&self) {
        self.inner.rebuild();
    }

    /// Recentre at zoom 1 and restart the layout.
    pub fn reset_camera(&self) {
        self.inner.reset_camera();
    }

    /// Zoom and pan so the whole graph is visible.
    pub fn fit_camera(&self) {
        self.inner.fit_camera();
    }

    /// Move the camera to a file's marker.
    pub fn focus_file(&self, path: &Path) -> bool {
        self.inner.focus_file(path)
    }

    /// Play the project history (global graph only).
    pub fn replay_history(&self) {
        self.inner.start_replay();
    }

    pub fn replaying(&self) -> bool {
        self.inner.replay.borrow().is_some()
    }

    // ---- appearance -------------------------------------------------------

    /// Applies the theme's UI colours and the accent.
    pub fn set_theme(&self, ui: &impro_core::themes::UiColors, accent: &str) {
        self.inner.set_theme(ui, accent);
    }

    /// The UI font family used for labels (`None`: the default sans font).
    pub fn set_font(&self, family: Option<&str>) {
        self.inner.set_font(family);
    }

    /// The application's Reduce motion preference.
    pub fn set_reduce_motion(&self, reduce: bool) {
        self.inner.set_reduce_motion(reduce);
    }

    // ---- preferences ------------------------------------------------------

    /// The current preferences.
    pub fn prefs(&self) -> GraphPrefs {
        self.inner.prefs.borrow().clone()
    }

    /// Applies preferences changed elsewhere (settings dialog, another view);
    /// the blob is normalised and not saved again.
    pub fn apply_prefs(&self, value: &Value) {
        self.inner.apply_external_prefs(prefs::normalize(value));
    }

    /// Writes pending changes now (call at shutdown).
    pub fn flush_prefs(&self) {
        self.inner.flush_prefs();
    }

    /// True if the last save failed.
    pub fn save_failed(&self) -> bool {
        self.inner.save_failed.get()
    }

    // ---- lifecycle --------------------------------------------------------

    /// Stops the worker threads and timers; safe to call twice. A dock calls
    /// this when the pane closes: see [`GraphView::dock_shutdown`].
    pub fn shutdown(&self) {
        self.inner.shutdown();
    }

    /// A closure for `add_custom_pane(kind, widget, shutdown)`.
    pub fn dock_shutdown(&self) -> Box<dyn FnOnce()> {
        let view = self.clone();
        Box::new(move || view.shutdown())
    }

    // ---- inspection ---------------------------------------------------------

    /// Nodes and lines currently shown.
    pub fn summary(&self) -> GraphSummary {
        self.inner.summary.borrow().clone()
    }

    pub fn frame_stats(&self) -> FrameStats {
        self.inner.stats.get()
    }

    /// The status text over the canvas, and whether it is shown.
    pub fn status_text(&self) -> (String, bool) {
        (
            self.inner.status.text().to_string(),
            self.inner.status.is_visible(),
        )
    }

    /// The truncation notice, if shown.
    pub fn notice_text(&self) -> Option<String> {
        self.inner
            .notice
            .is_visible()
            .then(|| self.inner.notice.text().to_string())
    }

    pub fn panel_open(&self) -> bool {
        self.inner.panel.is_open()
    }

    pub fn set_panel_open(&self, open: bool) {
        self.inner.set_panel_open(open, true);
    }
}

/// The file-search query for a tag (what the "search-tag" host runs).
pub fn tag_query(tag: &str) -> String {
    format!("tag:#{}", tag.trim_start_matches('#'))
}
