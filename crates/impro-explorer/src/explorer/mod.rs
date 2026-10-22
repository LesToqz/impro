//! Lazy filesystem explorer with theme-driven navigation and removal preferences.
//!
//! The panel is a vertical `GtkBox`: a header toolbar, then a vertical paned holding the
//! search panel (whose idle content is the file tree) and the collapsible Notes section.
//! All disk access, Git and process work happens on the worker pool; the panel only owns
//! GTK models and applies results in time-sliced steps.
mod actions;
mod gitdeco;
mod notes;
mod tree;
mod watch;

pub use actions::{
    ConfirmHook, ConfirmRequest, DeleteMode, DialogHooks, ErrorHook, NameAction, NameDialogHook,
    OpCallback, RemoveHook,
};
pub use gitdeco::set_git_collector;
pub use notes::{note_edited_text, NotesHandles};
pub use tree::{scan_count, scan_dir, set_scan_counting, Entry};

use crate::gitstatus::Snapshot;
use crate::searchpanel::{PathTextProvider, SearchPanel};
use crate::util;
use glib::subclass::Signal;
use gtk::{prelude::*, subclass::prelude::*};
use impro_core::fsops::FsOpError;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::OnceLock;

type OpJob = Box<dyn FnOnce() -> Result<PathBuf, FsOpError> + Send>;
type OpDone = Box<dyn FnOnce(Result<PathBuf, FsOpError>)>;
pub(crate) type PendingOp = (OpJob, OpDone);

/// Worker -> main-thread messages.
pub(crate) enum Msg {
    Dir {
        generation: u64,
        dir: PathBuf,
        entries: Vec<tree::Entry>,
    },
    Git {
        generation: u64,
        snapshot: Snapshot,
        /// Some decoration differs from the snapshot the worker started from.
        changed: bool,
    },
    Notes {
        generation: u64,
        listing: notes::Listing,
    },
    Op {
        id: u64,
        result: Result<PathBuf, FsOpError>,
    },
}

pub(crate) struct NotesWidgets {
    pub section: gtk::Box,
    pub header: gtk::Box,
    pub collapse: gtk::ToggleButton,
    pub chevron: gtk::Image,
    pub chevron_css: gtk::CssProvider,
    pub count: gtk::Label,
    pub new_button: gtk::Button,
    pub store: gtk::ListStore,
    pub tree: gtk::TreeView,
    pub hover_css: gtk::CssProvider,
    pub cell: gtk::CellRendererText,
    pub marker: gtk::CellRendererText,
    pub edited_cell: gtk::CellRendererText,
    pub empty: gtk::Box,
    pub empty_new: gtk::Button,
    pub scroll: gtk::ScrolledWindow,
    pub stack: gtk::Stack,
    pub body: gtk::Revealer,
}

pub(crate) struct Widgets {
    pub toolbar: gtk::Box,
    pub actions: gtk::Box,
    pub notes_button: gtk::ToggleButton,
    pub header_buttons: Vec<(&'static str, gtk::Button)>,
    pub store: gtk::TreeStore,
    pub tree: gtk::TreeView,
    pub text_cell: gtk::CellRendererText,
    pub scroll: gtk::ScrolledWindow,
    pub search: SearchPanel,
    pub notes_split: gtk::Paned,
    pub notes: NotesWidgets,
}

mod imp {
    use super::*;

    pub struct ExplorerPanel {
        pub(crate) w: OnceCell<Widgets>,
        pub(crate) destroyed: Cell<bool>,
        pub(crate) root: RefCell<PathBuf>,
        pub(crate) show_hidden: Cell<bool>,
        pub(crate) confirm_delete: Cell<bool>,
        pub(crate) delete_mode: Cell<DeleteMode>,
        pub(crate) open_after_create: Cell<bool>,
        pub(crate) folders_first: Cell<bool>,
        pub(crate) compact: Cell<bool>,
        pub(crate) git_colors: Cell<bool>,
        pub(crate) git_enabled: Cell<bool>,
        pub(crate) font_size: Cell<f64>,
        pub(crate) active_file: RefCell<Option<PathBuf>>,
        // tree model bookkeeping
        pub(crate) loaded: RefCell<HashSet<PathBuf>>,
        pub(crate) generation: Cell<u64>,
        pub(crate) pending_scans: Cell<usize>,
        pub(crate) scan_cancel: RefCell<crate::pool::CancelToken>,
        pub(crate) tasks: RefCell<VecDeque<tree::Apply>>,
        pub(crate) apply_source: RefCell<Option<glib::SourceId>>,
        pub(crate) want_expanded: RefCell<HashSet<PathBuf>>,
        pub(crate) want_select: RefCell<Option<PathBuf>>,
        // worker plumbing
        pub(crate) sender: RefCell<Option<glib::Sender<Msg>>>,
        pub(crate) receiver_source: RefCell<Option<glib::SourceId>>,
        pub(crate) next_op: Cell<u64>,
        pub(crate) op_running: Cell<bool>,
        pub(crate) op_queue: RefCell<VecDeque<PendingOp>>,
        pub(crate) ops: RefCell<HashMap<u64, OpDone>>,
        // git decorations
        pub(crate) git_snapshot: RefCell<Arc<Snapshot>>,
        pub(crate) git_generation: Cell<u64>,
        pub(crate) git_timeout: RefCell<Option<glib::SourceId>>,
        pub(crate) git_running: Cell<bool>,
        pub(crate) git_closed: Cell<bool>,
        pub(crate) git_cancel: RefCell<crate::pool::CancelToken>,
        pub(crate) git_monitor: RefCell<Option<gio::FileMonitor>>,
        pub(crate) git_dir: RefCell<Option<PathBuf>>,
        pub(crate) file_monitor: RefCell<Option<gio::FileMonitor>>,
        // notes section
        pub(crate) notes_enabled: Cell<bool>,
        pub(crate) notes_visible: Cell<bool>,
        pub(crate) notes_requested: Cell<Option<bool>>,
        pub(crate) notes_sort: RefCell<String>,
        pub(crate) notes_collapsed: Cell<bool>,
        pub(crate) notes_height: Cell<Option<i32>>,
        pub(crate) notes_layout: Cell<bool>,
        pub(crate) notes_monitors: RefCell<HashMap<PathBuf, gio::FileMonitor>>,
        pub(crate) notes_refresh: RefCell<Option<glib::SourceId>>,
        pub(crate) notes_generation: Cell<u64>,
        pub(crate) notes_running: Cell<bool>,
        pub(crate) notes_dirty: Cell<bool>,
        pub(crate) notes_dir_exists: Cell<bool>,
        pub(crate) notes_pending_select: RefCell<Option<PathBuf>>,
        pub(crate) notes_edited: RefCell<HashMap<PathBuf, String>>,
        pub(crate) watch_folders: Cell<bool>,
        pub(crate) dir_monitors: RefCell<HashMap<PathBuf, gio::FileMonitor>>,
        pub(crate) dir_dirty: RefCell<HashSet<PathBuf>>,
        pub(crate) dir_timeout: RefCell<Option<glib::SourceId>>,
        pub(crate) dir_sync: RefCell<Option<glib::SourceId>>,
        pub(crate) notes_task: RefCell<Option<notes::NotesTask>>,
        pub(crate) notes_apply_source: RefCell<Option<glib::SourceId>>,
        pub(crate) notes_hover: RefCell<HashMap<String, f64>>,
        pub(crate) notes_hover_path: RefCell<Option<String>>,
        pub(crate) notes_angle: Cell<f64>,
        pub(crate) notes_window:
            RefCell<Option<(glib::WeakRef<gtk::Window>, glib::SignalHandlerId)>>,
        // dialogs
        pub(crate) hooks: RefCell<DialogHooks>,
        pub(crate) dialog: RefCell<Option<gtk::Dialog>>,
    }

    impl Default for ExplorerPanel {
        fn default() -> Self {
            Self {
                w: OnceCell::new(),
                destroyed: Cell::new(false),
                root: RefCell::new(PathBuf::new()),
                show_hidden: Cell::new(true),
                confirm_delete: Cell::new(true),
                delete_mode: Cell::new(DeleteMode::Delete),
                open_after_create: Cell::new(true),
                folders_first: Cell::new(true),
                compact: Cell::new(false),
                git_colors: Cell::new(true),
                git_enabled: Cell::new(true),
                font_size: Cell::new(10.0),
                active_file: RefCell::new(None),
                loaded: RefCell::default(),
                generation: Cell::new(0),
                pending_scans: Cell::new(0),
                scan_cancel: RefCell::default(),
                tasks: RefCell::default(),
                apply_source: RefCell::new(None),
                want_expanded: RefCell::default(),
                want_select: RefCell::new(None),
                sender: RefCell::new(None),
                receiver_source: RefCell::new(None),
                next_op: Cell::new(1),
                op_running: Cell::new(false),
                op_queue: RefCell::default(),
                ops: RefCell::default(),
                git_snapshot: RefCell::default(),
                git_generation: Cell::new(0),
                git_timeout: RefCell::new(None),
                git_running: Cell::new(false),
                git_closed: Cell::new(false),
                git_cancel: RefCell::default(),
                git_monitor: RefCell::new(None),
                git_dir: RefCell::new(None),
                file_monitor: RefCell::new(None),
                notes_enabled: Cell::new(true),
                notes_visible: Cell::new(false),
                notes_requested: Cell::new(None),
                notes_sort: RefCell::new("name".into()),
                notes_collapsed: Cell::new(false),
                notes_height: Cell::new(None),
                notes_layout: Cell::new(false),
                notes_monitors: RefCell::default(),
                notes_refresh: RefCell::new(None),
                notes_generation: Cell::new(0),
                notes_running: Cell::new(false),
                notes_dirty: Cell::new(false),
                notes_dir_exists: Cell::new(false),
                notes_pending_select: RefCell::new(None),
                notes_edited: RefCell::default(),
                watch_folders: Cell::new(true),
                dir_monitors: RefCell::default(),
                dir_dirty: RefCell::default(),
                dir_timeout: RefCell::new(None),
                dir_sync: RefCell::new(None),
                notes_task: RefCell::new(None),
                notes_apply_source: RefCell::new(None),
                notes_hover: RefCell::default(),
                notes_hover_path: RefCell::new(None),
                notes_angle: Cell::new(0.0),
                notes_window: RefCell::new(None),
                hooks: RefCell::default(),
                dialog: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ExplorerPanel {
        const NAME: &'static str = "ImproExplorerPanel";
        type Type = super::ExplorerPanel;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for ExplorerPanel {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("file-activated")
                        .param_types([String::static_type()])
                        .run_last()
                        .build(),
                    Signal::builder("path-renamed")
                        .param_types([String::static_type(), String::static_type()])
                        .run_last()
                        .build(),
                    Signal::builder("path-deleted")
                        .param_types([String::static_type()])
                        .run_last()
                        .build(),
                    // absolute path, zero-based line and column, match length in characters
                    Signal::builder("search-result-activated")
                        .param_types([
                            String::static_type(),
                            i32::static_type(),
                            i32::static_type(),
                            i32::static_type(),
                        ])
                        .run_last()
                        .build(),
                    Signal::builder("search-escaped").run_last().build(),
                ]
            })
        }
    }
    impl WidgetImpl for ExplorerPanel {}
    impl ContainerImpl for ExplorerPanel {}
    impl BoxImpl for ExplorerPanel {}
}

glib::wrapper! {
    /// The file explorer panel.
    ///
    /// Signals: `file-activated(path)`, `path-renamed(old, new)`, `path-deleted(path)`,
    /// `search-result-activated(path, line, column, length)` and `search-escaped()`.
    pub struct ExplorerPanel(ObjectSubclass<imp::ExplorerPanel>)
        @extends gtk::Box, gtk::Container, gtk::Widget, @implements gtk::Buildable, gtk::Orientable;
}

impl ExplorerPanel {
    pub fn new(root: &Path) -> Self {
        let panel: Self = glib::Object::builder()
            .property("orientation", gtk::Orientation::Vertical)
            .build();
        panel.build(root);
        panel
    }

    pub(crate) fn w(&self) -> &Widgets {
        self.imp().w.get().expect("explorer panel is built")
    }

    fn build(&self, root: &Path) {
        self.style_context().add_class("impro-explorer");
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        toolbar.style_context().add_class("impro-explorer-header");
        toolbar.set_tooltip_text(Some("EXPLORER"));
        set_accessible_name(&toolbar, "EXPLORER");
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        actions.set_halign(gtk::Align::Center);
        toolbar.pack_start(&actions, true, true, 0);
        // This toggle controls the section below the file tree.
        let notes_button = gtk::ToggleButton::with_label("Notes");
        notes_button.set_relief(gtk::ReliefStyle::None);
        set_accessible_name(&notes_button, "Notes");
        notes_button.set_tooltip_text(Some("Show project notes"));
        actions.pack_start(&notes_button, false, false, 0);
        let mut header_buttons = Vec::new();
        for (icon, tooltip) in [
            ("document-new-symbolic", "New File"),
            ("folder-new-symbolic", "New Folder"),
            ("view-refresh-symbolic", "Refresh"),
        ] {
            let button = gtk::Button::from_icon_name(Some(icon), gtk::IconSize::Menu);
            button.set_relief(gtk::ReliefStyle::None);
            button.set_tooltip_text(Some(tooltip));
            set_accessible_name(&button, tooltip);
            actions.pack_start(&button, false, false, 0);
            header_buttons.push((tooltip, button));
        }
        self.pack_start(&toolbar, false, false, 0);

        let store = gtk::TreeStore::new(&[
            String::static_type(),
            String::static_type(),
            String::static_type(),
        ]);
        let tree = gtk::TreeView::with_model(&store);
        tree.set_headers_visible(false);
        tree.set_grid_lines(gtk::TreeViewGridLines::None);
        tree.set_margin_start(12);
        tree.set_margin_end(12);
        tree.set_margin_top(4);
        tree.set_margin_bottom(24);
        let column = gtk::TreeViewColumn::new();
        let icon = crate::icons::IconCell::new();
        util::pack(&column, &icon, false);
        // Preserve model icon names (also used to identify directories), but render their
        // system symbolic variants in the row's muted colour.
        util::data_func(&column, &icon, |cell, model, iter| {
            let name = util::opt_string(model, iter, 0).filter(|name| !name.is_empty());
            cell.set_property("icon-name", name.map(|name| format!("{name}-symbolic")));
        });
        let text_cell = gtk::CellRendererText::new();
        text_cell.set_ellipsize(pango::EllipsizeMode::End);
        util::pack(&column, &text_cell, true);
        util::attribute(&column, &text_cell, "text", 1);
        // Rows share one height, so GTK need not measure all of them (50 000 entries).
        column.set_sizing(gtk::TreeViewColumnSizing::Fixed);
        column.set_expand(true);
        tree.append_column(&column);
        tree.set_fixed_height_mode(true);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .build();
        scroll.add(&tree);
        // The search row sits directly under the header; the tree is its idle content.
        let search = SearchPanel::new(&scroll, root);
        search.set_size_request(-1, 120);
        let notes_split = gtk::Paned::new(gtk::Orientation::Vertical);
        notes_split.style_context().add_class("impro-notes-split");
        notes_split.set_wide_handle(true);
        notes_split.pack1(&search, true, false);
        let notes = self.build_notes_section();
        notes.section.set_no_show_all(true);
        notes_split.pack2(&notes.section, false, false);
        self.pack_start(&notes_split, true, true, 0);
        let widgets = Widgets {
            toolbar,
            actions,
            notes_button,
            header_buttons,
            store,
            tree,
            text_cell,
            scroll,
            search,
            notes_split,
            notes,
        };
        assert!(
            self.imp().w.set(widgets).is_ok(),
            "explorer panel built once"
        );
        let w = self.w();
        // The text cell reads git decorations at paint time.
        let weak = self.downgrade();
        util::data_func(&column, &w.text_cell, move |cell, model, iter| {
            if let Some(panel) = weak.upgrade() {
                panel.git_cell_data(cell, model, iter);
            }
        });
        self.connect_signals();
        self.attach_channel();
        self.set_font_size(10.0);
        self.set_root(root);
    }

    fn connect_signals(&self) {
        let w = self.w();
        let weak = self.downgrade();
        w.notes_button.connect_toggled(move |button| {
            if let Some(panel) = weak.upgrade() {
                panel.show_notes_section(button.is_active());
            }
        });
        for (tooltip, button) in &w.header_buttons {
            let weak = self.downgrade();
            let tooltip = *tooltip;
            button.connect_clicked(move |_| {
                let Some(panel) = weak.upgrade() else { return };
                match tooltip {
                    "New File" => panel.new_dialog(false),
                    "New Folder" => panel.new_dialog(true),
                    _ => panel.refresh(),
                }
            });
        }
        let weak = self.downgrade();
        w.tree.connect_row_expanded(move |_, iter, _| {
            if let Some(panel) = weak.upgrade() {
                panel.on_expanded(iter);
                panel.schedule_watch_sync();
            }
        });
        let weak = self.downgrade();
        w.tree.connect_row_collapsed(move |_, _, _| {
            if let Some(panel) = weak.upgrade() {
                panel.schedule_watch_sync();
            }
        });
        let weak = self.downgrade();
        w.tree.connect_row_activated(move |_, path, _| {
            if let Some(panel) = weak.upgrade() {
                panel.on_activated(path);
            }
        });
        let weak = self.downgrade();
        w.tree.connect_button_press_event(move |tree, event| {
            weak.upgrade().map_or(glib::Propagation::Proceed, |panel| {
                panel.on_button(tree, event)
            })
        });
        let weak = self.downgrade();
        w.tree.connect_popup_menu(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.popup_menu(None);
            }
            true
        });
        let weak = self.downgrade();
        self.connect_key_press_event(move |_, event| {
            if weak.upgrade().is_some_and(|panel| panel.on_key(event)) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        let weak = self.downgrade();
        w.search
            .connect_result_activated(move |_, path, line, column, length| {
                if let Some(panel) = weak.upgrade() {
                    panel.emit_by_name::<()>(
                        "search-result-activated",
                        &[&model_path(path), &line, &column, &length],
                    );
                }
            });
        let weak = self.downgrade();
        w.search.connect_escaped(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.emit_by_name::<()>("search-escaped", &[]);
            }
        });
        let weak = self.downgrade();
        w.notes_split
            .connect_notify_local(Some("position"), move |_, _| {
                if let Some(panel) = weak.upgrade() {
                    panel.notes_position_changed();
                }
            });
        let weak = self.downgrade();
        w.notes_split.connect_size_allocate(move |_, _| {
            if let Some(panel) = weak.upgrade() {
                panel.layout_notes();
            }
        });
        let weak = self.downgrade();
        self.connect_hierarchy_changed(move |_, _| {
            if let Some(panel) = weak.upgrade() {
                panel.notes_toplevel_changed();
            }
        });
        let weak = self.downgrade();
        self.connect_destroy(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.shutdown();
            }
        });
    }

    #[allow(deprecated)]
    fn attach_channel(&self) {
        let (sender, receiver) = glib::MainContext::channel::<Msg>(glib::Priority::HIGH_IDLE);
        let weak = self.downgrade();
        let source = receiver.attach(None, move |message| match weak.upgrade() {
            Some(panel) => {
                panel.receive(message);
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        *self.imp().sender.borrow_mut() = Some(sender);
        *self.imp().receiver_source.borrow_mut() = Some(source);
    }

    pub(crate) fn sender(&self) -> Option<glib::Sender<Msg>> {
        self.imp().sender.borrow().clone()
    }

    fn receive(&self, message: Msg) {
        match message {
            Msg::Dir {
                generation,
                dir,
                entries,
            } => self.on_dir_listed(generation, dir, entries),
            Msg::Git {
                generation,
                snapshot,
                changed,
            } => self.deliver_git(generation, snapshot, changed),
            Msg::Notes {
                generation,
                listing,
            } => self.deliver_notes(generation, listing),
            Msg::Op { id, result } => {
                let callback = self.imp().ops.borrow_mut().remove(&id);
                self.imp().op_running.set(false);
                if let Some(callback) = callback {
                    callback(result);
                }
                self.pump_ops();
            }
        }
    }

    /// Run `job` on the worker pool and hand its result to `done` on the main thread.
    /// Jobs of one panel run strictly in submission order.
    pub(crate) fn run_op(
        &self,
        job: impl FnOnce() -> Result<PathBuf, FsOpError> + Send + 'static,
        done: impl FnOnce(Result<PathBuf, FsOpError>) + 'static,
    ) {
        self.imp()
            .op_queue
            .borrow_mut()
            .push_back((Box::new(job), Box::new(done)));
        self.pump_ops();
    }

    fn pump_ops(&self) {
        let imp = self.imp();
        if imp.op_running.get() {
            return;
        }
        let Some(sender) = self.sender() else { return };
        let Some((job, done)) = imp.op_queue.borrow_mut().pop_front() else {
            return;
        };
        imp.op_running.set(true);
        let id = imp.next_op.get();
        imp.next_op.set(id + 1);
        imp.ops.borrow_mut().insert(id, done);
        crate::pool::spawn(crate::pool::Lane::Interactive, move || {
            let _ = sender.send(Msg::Op { id, result: job() });
        });
    }

    fn shutdown(&self) {
        self.imp().destroyed.set(true);
        self.stop_git();
        self.stop_watches();
        let imp = self.imp();
        if let Some(source) = imp.apply_source.take() {
            source.remove();
        }
        imp.tasks.borrow_mut().clear();
        imp.scan_cancel.borrow().cancel();
        imp.generation.set(imp.generation.get() + 1);
        if let Some(source) = imp.receiver_source.take() {
            source.remove();
        }
        imp.sender.take();
        imp.ops.borrow_mut().clear();
        imp.op_queue.borrow_mut().clear();
    }

    // ---- public surface ---------------------------------------------------------------

    /// The panel was destroyed; its methods then do nothing.
    pub fn is_destroyed(&self) -> bool {
        self.imp().destroyed.get()
    }

    /// The project root (absolute).
    pub fn root(&self) -> PathBuf {
        self.imp().root.borrow().clone()
    }

    /// The search panel wrapped around the file tree.
    pub fn search(&self) -> SearchPanel {
        self.w().search.clone()
    }

    /// The file-tree model (columns: icon name, file name, absolute path). Connect to its
    /// `row-inserted` / `row-deleted` signals to learn about tree changes.
    pub fn store(&self) -> gtk::TreeStore {
        self.w().store.clone()
    }

    pub fn tree(&self) -> gtk::TreeView {
        self.w().tree.clone()
    }

    /// The header toolbar and the box of its buttons (Notes, New File, New Folder, Refresh).
    pub fn header(&self) -> (gtk::Box, gtk::Box) {
        (self.w().toolbar.clone(), self.w().actions.clone())
    }

    /// A header button by its tooltip: "New File", "New Folder" or "Refresh".
    pub fn header_button(&self, tooltip: &str) -> Option<gtk::Button> {
        self.w()
            .header_buttons
            .iter()
            .find(|(name, _)| *name == tooltip)
            .map(|(_, button)| button.clone())
    }

    pub fn show_hidden(&self) -> bool {
        self.imp().show_hidden.get()
    }

    pub fn confirm_delete(&self) -> bool {
        self.imp().confirm_delete.get()
    }

    pub fn delete_mode(&self) -> DeleteMode {
        self.imp().delete_mode.get()
    }

    pub fn open_after_create(&self) -> bool {
        self.imp().open_after_create.get()
    }

    /// Open freshly created notes in the editor (default `true`).
    pub fn set_open_after_create(&self, open: bool) {
        self.imp().open_after_create.set(open);
    }

    pub fn font_size(&self) -> f64 {
        self.imp().font_size.get()
    }

    /// Resize file names and reserve enough height for their Pango metrics.
    pub fn set_font_size(&self, size: f64) {
        let imp = self.imp();
        let w = self.w();
        imp.font_size.set(size);
        let context = w.tree.pango_context();
        let mut description = context.font_description().unwrap_or_default();
        description.set_size((size * f64::from(pango::SCALE)) as i32);
        let metrics = context.metrics(Some(&description), None);
        let height = f64::from(metrics.ascent() + metrics.descent()) / f64::from(pango::SCALE);
        let compact = imp.compact.get();
        let whole = (height + 0.999) as i32;
        for (cell, tree) in [(&w.text_cell, &w.tree), (&w.notes.cell, &w.notes.tree)] {
            cell.set_size_points(size);
            cell.set_fixed_size(
                -1,
                if compact { 16 } else { 20 }.max(whole + if compact { 0 } else { 4 }),
            );
            tree.columns_autosize();
            tree.queue_resize();
        }
        w.notes.cell.set_fixed_size(-1, 32.max(whole + 8));
        w.search.set_font_size(size);
    }

    /// Provide live (possibly unsaved) text for a path, or `None` to read the disk.
    pub fn set_text_provider(&self, provider: Option<PathTextProvider>) {
        self.w().search.set_text_provider(provider);
    }

    /// Focus the search entry; a given query replaces its text and runs at once.
    pub fn focus_search(&self, query: Option<&str>) {
        let search = &self.w().search;
        search.focus_search();
        if let Some(query) = query.filter(|q| !q.is_empty()) {
            search.search_now(Some(query));
            search.entry().set_position(-1);
        }
    }

    pub fn clear_search(&self) {
        self.w().search.clear();
    }

    pub fn focus_tree(&self) {
        self.w().tree.grab_focus();
    }

    /// Restyle for compact rows (a CSS class and shorter row heights).
    pub fn set_compact(&self, enabled: bool) {
        let imp = self.imp();
        imp.compact.set(enabled);
        let context = self.style_context();
        if enabled {
            context.add_class("impro-explorer-compact");
        } else {
            context.remove_class("impro-explorer-compact");
        }
        self.set_font_size(imp.font_size.get());
    }

    pub fn compact(&self) -> bool {
        self.imp().compact.get()
    }

    // ---- typed signal helpers ---------------------------------------------------------

    pub fn connect_file_activated<F: Fn(&Self, &Path) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("file-activated", false, move |args| {
            let panel = args[0].get::<ExplorerPanel>().ok()?;
            let path: String = args[1].get().ok()?;
            f(&panel, &util::decode_path(&path));
            None
        })
    }

    pub fn connect_path_renamed<F: Fn(&Self, &Path, &Path) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("path-renamed", false, move |args| {
            let panel = args[0].get::<ExplorerPanel>().ok()?;
            let (old, new): (String, String) = (args[1].get().ok()?, args[2].get().ok()?);
            f(&panel, &util::decode_path(&old), &util::decode_path(&new));
            None
        })
    }

    pub fn connect_path_deleted<F: Fn(&Self, &Path) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("path-deleted", false, move |args| {
            let panel = args[0].get::<ExplorerPanel>().ok()?;
            let path: String = args[1].get().ok()?;
            f(&panel, &util::decode_path(&path));
            None
        })
    }

    /// `f(panel, path, line, column, length)` with zero-based line/column (characters).
    pub fn connect_search_result_activated<F: Fn(&Self, &Path, i32, i32, i32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("search-result-activated", false, move |args| {
            let panel = args[0].get::<ExplorerPanel>().ok()?;
            let path: String = args[1].get().ok()?;
            f(
                &panel,
                &util::decode_path(&path),
                args[2].get().ok()?,
                args[3].get().ok()?,
                args[4].get().ok()?,
            );
            None
        })
    }

    pub fn connect_search_escaped<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("search-escaped", false, move |args| {
            f(&args[0].get::<ExplorerPanel>().ok()?);
            None
        })
    }

    /// Activate a file as if it was opened from the tree (emits `file-activated`).
    pub(crate) fn emit_file_activated(&self, path: &Path) {
        self.emit_by_name::<()>("file-activated", &[&model_path(path)]);
    }
}

pub(crate) fn set_accessible_name(widget: &impl IsA<gtk::Widget>, name: &str) {
    if let Some(accessible) = widget.accessible() {
        atk::prelude::AtkObjectExt::set_name(&accessible, name);
    }
}

/// Convert a path to the string stored in the tree model.
pub(crate) fn model_path(path: &Path) -> String {
    util::encode_path(path)
}

pub(crate) fn css(ui: &impro_core::themes::UiColors) -> String {
    impro_ui::render_css(include_str!("../explorer.css"), ui)
}
