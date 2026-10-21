//! Search-in-files panel: an entry row over a stack of the explorer content and the results.
//!
//! Queries run on the worker pool and stream their results back in batches; the panel
//! renders them incrementally (a few milliseconds per idle step) so a thousand hits keep
//! the list at 60 fps. Every new query cancels the previous run within one file.
use crate::pool::{self, CancelToken, Lane};
use crate::search::{search_stream, FileGroup, SearchError, SearchOptions, SearchResults};
use crate::util;
use glib::subclass::Signal;
use gtk::{prelude::*, subclass::prelude::*};
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Live (possibly unsaved) text for a path, or `None` to read the disk. Always called on
/// the GTK main thread, in chunks, while a search is running.
pub type PathTextProvider = Rc<dyn Fn(&Path) -> Option<String>>;

/// Delay between the last keystroke and the start of a search.
const DEBOUNCE: Duration = Duration::from_millis(200);
/// Minimum interval between result batches posted by the worker.
const BATCH_INTERVAL: Duration = Duration::from_millis(50);
/// File-name rows appended in order before the rest are inserted back to front.
const NAME_HEAD: usize = 300;
/// Main-thread time spent rendering rows per step.
const RENDER_BUDGET: Duration = Duration::from_millis(3);

/// CSS of the search panel for the default theme (the original's `SEARCH_CSS`).
pub fn search_css() -> String {
    crate::search_css_for(&impro_core::themes::get_theme(None).ui)
}

struct Batch {
    generation: u64,
    reset: bool,
    groups: Vec<FileGroup>,
    names: Vec<PathBuf>,
    truncated: bool,
    total_matches: usize,
    total_files: usize,
    error: Option<SearchError>,
    done: bool,
}

enum Msg {
    Batch(Batch),
    Overlay {
        generation: u64,
        paths: Vec<PathBuf>,
        reply: mpsc::Sender<Vec<Option<String>>>,
    },
}

/// Rows still to be inserted. Big items are resumable so a step never exceeds its budget.
enum Item {
    /// A file group whose header row is not inserted yet.
    Group(FileGroup),
    /// The remaining match rows of an inserted group header.
    Rows {
        parent: gtk::TreeRowReference,
        path: String,
        matches: Vec<crate::search::Match>,
        next: usize,
    },
    /// The file-name group (all names; rendered when the search is complete).
    Names(Vec<PathBuf>),
    /// Remaining file-name rows (head in order, then tail back to front after `anchor`).
    NameRows(NameJob),
}

struct NameJob {
    parent: gtk::TreeRowReference,
    paths: Vec<PathBuf>,
    pos: usize,
    rev: usize,
    anchor: Option<gtk::TreeIter>,
}

struct Widgets {
    entry: gtk::SearchEntry,
    case_button: gtk::ToggleButton,
    regex_button: gtk::ToggleButton,
    stack: gtk::Stack,
    summary: gtk::Label,
    tree: gtk::TreeView,
    store: gtk::TreeStore,
    cells: [gtk::CellRendererText; 2],
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct SearchPanel {
        pub(super) w: OnceCell<Widgets>,
        pub(super) root: RefCell<PathBuf>,
        pub(super) current_path: RefCell<Option<PathBuf>>,
        pub(super) provider: RefCell<Option<PathTextProvider>>,
        pub(super) results: RefCell<SearchResults>,
        pub(super) generation: Cell<u64>,
        pub(super) timeout: RefCell<Option<glib::SourceId>>,
        pub(super) cancel: RefCell<CancelToken>,
        pub(super) destroyed: Cell<bool>,
        pub(super) searching: Cell<bool>,
        pub(super) font_size: Cell<f64>,
        pub(super) sender: RefCell<Option<glib::Sender<Msg>>>,
        pub(super) receiver_source: RefCell<Option<glib::SourceId>>,
        pub(super) queue: RefCell<VecDeque<Item>>,
        pub(super) render_source: RefCell<Option<glib::SourceId>>,
        pub(super) has_names_group: Cell<bool>,
        pub(super) focus_first: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SearchPanel {
        const NAME: &'static str = "ImproSearchPanel";
        type Type = super::SearchPanel;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for SearchPanel {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // absolute path, zero-based line and column, match length in characters
                    Signal::builder("result-activated")
                        .param_types([
                            String::static_type(),
                            i32::static_type(),
                            i32::static_type(),
                            i32::static_type(),
                        ])
                        .run_last()
                        .build(),
                    Signal::builder("escaped").run_last().build(),
                ]
            })
        }
    }
    impl WidgetImpl for SearchPanel {}
    impl ContainerImpl for SearchPanel {}
    impl BoxImpl for SearchPanel {}
}

glib::wrapper! {
    /// Wraps explorer content; emits absolute paths and zero-based match locations.
    ///
    /// Signals: `result-activated(path, line, column, length)` and `escaped()`.
    pub struct SearchPanel(ObjectSubclass<imp::SearchPanel>)
        @extends gtk::Box, gtk::Container, gtk::Widget, @implements gtk::Buildable, gtk::Orientable;
}

fn escape(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}

fn char_slice(text: &str, from: usize, to: usize) -> &str {
    let byte = |index: usize| {
        text.char_indices()
            .nth(index)
            .map_or(text.len(), |(i, _)| i)
    };
    let (a, b) = (byte(from), byte(to));
    &text[a..b.max(a)]
}

fn hex(colour: &gdk::RGBA) -> String {
    let channel = |value: f64| (value * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(colour.red()),
        channel(colour.green()),
        channel(colour.blue())
    )
}

impl SearchPanel {
    /// `content` is shown while the query is empty (the explorer tree).
    pub fn new(content: &impl IsA<gtk::Widget>, root: &Path) -> Self {
        let panel: Self = glib::Object::builder()
            .property("orientation", gtk::Orientation::Vertical)
            .build();
        panel.build(content.as_ref(), root);
        panel
    }

    fn w(&self) -> &Widgets {
        self.imp().w.get().expect("search panel is built")
    }

    fn build(&self, content: &gtk::Widget, root: &Path) {
        let imp = self.imp();
        self.style_context().add_class("impro-search");
        *imp.root.borrow_mut() = absolute_or_empty(root);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        row.style_context().add_class("impro-search-row");
        let entry = gtk::SearchEntry::new();
        entry.set_placeholder_text(Some("Search"));
        // GtkSearchEntry provides the search icon and a secondary clear icon only while
        // text is present, including keyboard/accessibility support.
        entry.set_width_chars(6);
        entry.style_context().add_class("impro-search-entry");
        row.pack_start(&entry, true, true, 0);
        let case_button = gtk::ToggleButton::with_label("Aa");
        case_button.set_tooltip_text(Some("Match case"));
        let regex_button = gtk::ToggleButton::with_label(".*");
        regex_button.set_tooltip_text(Some("Use regular expression"));
        for button in [&case_button, &regex_button] {
            button.set_valign(gtk::Align::Center);
            if let Some(accessible) = button.accessible() {
                atk::prelude::AtkObjectExt::set_name(
                    &accessible,
                    &button.tooltip_text().unwrap_or_default(),
                );
            }
            button.style_context().add_class("impro-search-toggle");
            row.pack_start(button, false, false, 0);
        }
        self.pack_start(&row, false, false, 0);
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::None);
        stack.add_named(content, "content");
        let results_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let summary = gtk::Label::new(None);
        summary.set_xalign(0.0);
        summary.set_ellipsize(pango::EllipsizeMode::End);
        summary.style_context().add_class("impro-search-summary");
        results_box.pack_start(&summary, false, false, 0);
        // icon, line-number markup, preview markup, path, line, column, length, kind
        let store = gtk::TreeStore::new(&[
            String::static_type(),
            String::static_type(),
            String::static_type(),
            String::static_type(),
            i32::static_type(),
            i32::static_type(),
            i32::static_type(),
            String::static_type(),
        ]);
        let tree = gtk::TreeView::with_model(&store);
        tree.set_headers_visible(false);
        tree.style_context().add_class("impro-search-results");
        tree.set_enable_search(false);
        tree.set_activate_on_single_click(true);
        tree.set_margin_start(12);
        tree.set_margin_end(12);
        tree.set_margin_bottom(24);
        let column = gtk::TreeViewColumn::new();
        let icon = gtk::CellRendererPixbuf::new();
        util::pack(&column, &icon, false);
        util::attribute(&column, &icon, "icon-name", 0);
        let number = gtk::CellRendererText::new();
        number.set_xalign(1.0);
        number.set_ypad(2);
        util::pack(&column, &number, false);
        util::attribute(&column, &number, "markup", 1);
        let label = gtk::CellRendererText::new();
        label.set_ypad(2);
        label.set_ellipsize(pango::EllipsizeMode::End);
        util::pack(&column, &label, true);
        util::attribute(&column, &label, "markup", 2);
        let tree_weak = tree.downgrade();
        util::data_func(&column, &label, move |cell, model, iter| {
            if let Some(tree) = tree_weak.upgrade() {
                style_result(&tree, cell, model, iter);
            }
        });
        // All rows share one height, so GTK need not measure them individually.
        column.set_sizing(gtk::TreeViewColumnSizing::Fixed);
        column.set_expand(true);
        tree.append_column(&column);
        tree.set_fixed_height_mode(true);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .build();
        scroll.add(&tree);
        results_box.pack_start(&scroll, true, true, 0);
        stack.add_named(&results_box, "results");
        self.pack_start(&stack, true, true, 0);
        let widgets = Widgets {
            entry,
            case_button,
            regex_button,
            stack,
            summary,
            tree,
            store,
            cells: [number, label],
        };
        assert!(imp.w.set(widgets).is_ok(), "search panel built once");
        self.set_font_size(10.0);
        self.connect_signals();
        self.attach_channel();
        self.show_all();
        self.w().stack.set_visible_child_name("content");
    }

    fn connect_signals(&self) {
        let w = self.w();
        let weak = self.downgrade();
        let changed = move |_: &gtk::SearchEntry| {
            if let Some(panel) = weak.upgrade() {
                panel.changed();
            }
        };
        w.entry.connect_changed(changed);
        for button in [&w.case_button, &w.regex_button] {
            let weak = self.downgrade();
            button.connect_toggled(move |_| {
                if let Some(panel) = weak.upgrade() {
                    panel.changed();
                }
            });
        }
        let weak = self.downgrade();
        w.entry.connect_stop_search(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.clear();
            }
        });
        let weak = self.downgrade();
        w.entry.connect_activate(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.entry_activate();
            }
        });
        let weak = self.downgrade();
        w.entry.connect_key_press_event(move |entry, event| {
            weak.upgrade().map_or(glib::Propagation::Proceed, |panel| {
                panel.key(entry.upcast_ref(), event)
            })
        });
        let weak = self.downgrade();
        w.tree.connect_key_press_event(move |tree, event| {
            weak.upgrade().map_or(glib::Propagation::Proceed, |panel| {
                panel.key(tree.upcast_ref(), event)
            })
        });
        let weak = self.downgrade();
        w.tree.connect_row_activated(move |_, path, _| {
            if let Some(panel) = weak.upgrade() {
                panel.activate(path);
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
        let (sender, receiver) = glib::MainContext::channel::<Msg>(glib::Priority::DEFAULT_IDLE);
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

    fn shutdown(&self) {
        let imp = self.imp();
        imp.destroyed.set(true);
        self.invalidate();
        if let Some(source) = imp.render_source.take() {
            source.remove();
        }
        if let Some(source) = imp.receiver_source.take() {
            source.remove();
        }
        imp.sender.take();
        imp.queue.borrow_mut().clear();
    }

    // ---- public surface -------------------------------------------------------------

    pub fn entry(&self) -> gtk::SearchEntry {
        self.w().entry.clone()
    }
    pub fn case_button(&self) -> gtk::ToggleButton {
        self.w().case_button.clone()
    }
    pub fn regex_button(&self) -> gtk::ToggleButton {
        self.w().regex_button.clone()
    }
    pub fn stack(&self) -> gtk::Stack {
        self.w().stack.clone()
    }
    pub fn summary(&self) -> gtk::Label {
        self.w().summary.clone()
    }
    pub fn tree(&self) -> gtk::TreeView {
        self.w().tree.clone()
    }
    pub fn store(&self) -> gtk::TreeStore {
        self.w().store.clone()
    }
    /// The number and preview cell renderers of the results column.
    pub fn cells(&self) -> [gtk::CellRendererText; 2] {
        self.w().cells.clone()
    }
    pub fn query(&self) -> String {
        self.w().entry.text().to_string()
    }
    pub fn root(&self) -> PathBuf {
        self.imp().root.borrow().clone()
    }
    pub fn current_path(&self) -> Option<PathBuf> {
        self.imp().current_path.borrow().clone()
    }
    pub fn font_size(&self) -> f64 {
        self.imp().font_size.get()
    }
    /// Everything shown so far (a clone; may still be growing while a search runs).
    pub fn results(&self) -> SearchResults {
        self.imp().results.borrow().clone()
    }
    /// A search is running or its rows are still being rendered.
    pub fn is_searching(&self) -> bool {
        self.imp().searching.get() || !self.imp().queue.borrow().is_empty()
    }
    /// A debounced start is pending.
    pub fn timeout_pending(&self) -> bool {
        self.imp().timeout.borrow().is_some()
    }
    pub fn generation(&self) -> u64 {
        self.imp().generation.get()
    }

    pub fn set_root(&self, path: &Path) {
        *self.imp().root.borrow_mut() = absolute_or_empty(path);
        self.changed();
    }

    /// List the active editor file's hits first.
    pub fn set_current_file(&self, path: Option<&Path>) {
        let current = path
            .filter(|p| !p.as_os_str().is_empty())
            .map(impro_core::fsops::absolute);
        *self.imp().current_path.borrow_mut() = current.clone();
        // Reorder the already visible results immediately, then refresh buffers.
        {
            let mut results = self.imp().results.borrow_mut();
            for group in &mut results.groups {
                group.is_current = Some(&group.path) == current.as_ref();
            }
            results.groups.sort_by_cached_key(|g| {
                (
                    !g.is_current,
                    impro_core::text::casefold(&g.relative_path),
                    g.relative_path.clone(),
                )
            });
        }
        if !self.query().is_empty() {
            self.rebuild();
            self.changed();
        }
    }

    /// Set the path -> live text provider; it is always invoked on the GTK thread.
    pub fn set_text_provider(&self, provider: Option<PathTextProvider>) {
        *self.imp().provider.borrow_mut() = provider;
        self.changed();
    }

    /// Size the result rows (in points).
    pub fn set_font_size(&self, size: f64) {
        self.imp().font_size.set(size);
        let w = self.w();
        let context = w.tree.pango_context();
        let mut description = context.font_description().unwrap_or_default();
        description.set_size((size * f64::from(pango::SCALE)) as i32);
        let metrics = context.metrics(Some(&description), None);
        let height = f64::from(metrics.ascent() + metrics.descent()) / f64::from(pango::SCALE);
        for cell in &w.cells {
            cell.set_size_points(size);
            cell.set_fixed_size(-1, 20.max(((height + 0.999) as i32) + 4));
        }
        w.tree.columns_autosize();
        w.tree.queue_resize();
    }

    pub fn focus_search(&self) {
        let entry = &self.w().entry;
        entry.grab_focus();
        entry.select_region(0, -1);
    }

    pub fn clear(&self) {
        self.w().entry.set_text("");
        self.changed();
    }

    /// Cancel pending work and search at once (no typing delay); with `Some(query)` the
    /// entry text is replaced first. Results stream in; wait for [`Self::is_searching`].
    pub fn search_now(&self, query: Option<&str>) {
        if let Some(query) = query {
            self.w().entry.set_text(query);
        }
        self.invalidate();
        let query = self.query();
        self.w().stack.set_visible_child_name(if query.is_empty() {
            "content"
        } else {
            "results"
        });
        if query.is_empty() {
            self.reset_results();
            return;
        }
        self.start();
    }

    // ---- query lifecycle -------------------------------------------------------------

    fn invalidate(&self) {
        let imp = self.imp();
        imp.cancel.borrow().cancel();
        imp.generation.set(imp.generation.get() + 1);
        if let Some(source) = imp.timeout.take() {
            source.remove();
        }
        imp.searching.set(false);
        imp.focus_first.set(false);
    }

    fn changed(&self) {
        let imp = self.imp();
        if imp.destroyed.get() {
            return;
        }
        self.invalidate();
        let query = self.query();
        self.w().stack.set_visible_child_name(if query.is_empty() {
            "content"
        } else {
            "results"
        });
        if query.is_empty() {
            self.reset_results();
        } else {
            let weak = self.downgrade();
            let source = glib::timeout_add_local_once(DEBOUNCE, move || {
                if let Some(panel) = weak.upgrade() {
                    panel.imp().timeout.take();
                    panel.start();
                }
            });
            *imp.timeout.borrow_mut() = Some(source);
        }
    }

    fn reset_results(&self) {
        let imp = self.imp();
        *imp.results.borrow_mut() = SearchResults::default();
        imp.queue.borrow_mut().clear();
        imp.has_names_group.set(false);
        self.w().store.clear();
        self.set_summary("No results");
    }

    fn set_summary(&self, text: &str) {
        let summary = &self.w().summary;
        if summary.text() != text {
            summary.set_text(text);
        }
        if summary.tooltip_text().as_deref() != Some(text) {
            summary.set_tooltip_text(Some(text));
        }
    }

    fn show_error(&self, error: &SearchError) {
        let imp = self.imp();
        *imp.results.borrow_mut() = SearchResults::default();
        imp.queue.borrow_mut().clear();
        imp.has_names_group.set(false);
        self.w().store.clear();
        self.set_summary(&error.0);
        imp.searching.set(false);
    }

    /// Start a worker run for the current query, root, toggles and live-text provider.
    fn start(&self) {
        let imp = self.imp();
        if imp.destroyed.get() {
            return;
        }
        let generation = imp.generation.get();
        let cancel = CancelToken::new();
        *imp.cancel.borrow_mut() = cancel.clone();
        let Some(sender) = imp.sender.borrow().clone() else {
            return;
        };
        let w = self.w();
        let query = self.query();
        let root = imp.root.borrow().clone();
        let options = SearchOptions {
            case_sensitive: w.case_button.is_active(),
            regex: w.regex_button.is_active(),
            current_path: imp.current_path.borrow().clone(),
            ..SearchOptions::default()
        };
        let has_provider = imp.provider.borrow().is_some();
        imp.searching.set(true);
        pool::spawn(Lane::Background, move || {
            run_search(
                generation,
                root,
                query,
                options,
                has_provider,
                cancel,
                sender,
            );
        });
    }

    // ---- worker messages ---------------------------------------------------------------

    fn receive(&self, message: Msg) {
        match message {
            Msg::Batch(batch) => {
                self.deliver(batch);
            }
            Msg::Overlay {
                generation,
                paths,
                reply,
            } => {
                // The editor may have thousands of paths to look up. Keep the single
                // worker round trip, but yield between small main-thread slices.
                self.overlay_step(generation, paths.into_iter(), reply, Vec::new());
            }
        }
    }

    fn overlay_step(
        &self,
        generation: u64,
        mut paths: std::vec::IntoIter<PathBuf>,
        reply: mpsc::Sender<Vec<Option<String>>>,
        mut answer: Vec<Option<String>>,
    ) {
        let imp = self.imp();
        if generation != imp.generation.get() || imp.destroyed.get() {
            return;
        }
        // Clone before calling user code: a provider can replace itself or the query.
        let provider = imp.provider.borrow().clone();
        let Some(provider) = provider else {
            let _ = reply.send(answer);
            return;
        };
        let deadline = Instant::now() + Duration::from_millis(2);
        while let Some(path) = paths.next() {
            answer.push(provider(&path));
            if generation != imp.generation.get() || imp.destroyed.get() {
                return;
            }
            if Instant::now() >= deadline && paths.len() > 0 {
                let weak = self.downgrade();
                glib::idle_add_local_once(move || {
                    if let Some(panel) = weak.upgrade() {
                        panel.overlay_step(generation, paths, reply, answer);
                    }
                });
                return;
            }
        }
        let _ = reply.send(answer);
    }

    /// Apply a batch unless it belongs to a superseded run; `false` if it was dropped.
    fn deliver(&self, batch: Batch) -> bool {
        let imp = self.imp();
        if imp.destroyed.get() || batch.generation != imp.generation.get() {
            return false;
        }
        if let Some(error) = &batch.error {
            self.show_error(error);
            return true;
        }
        if batch.reset {
            imp.queue.borrow_mut().clear();
            imp.has_names_group.set(false);
            self.w().store.clear();
            imp.results.borrow_mut().groups.clear();
            imp.results.borrow_mut().file_names.clear();
        }
        {
            let mut results = imp.results.borrow_mut();
            let mut queue = imp.queue.borrow_mut();
            for group in batch.groups {
                results.groups.push(group.clone());
                queue.push_back(Item::Group(group));
            }
            results.file_names.extend(batch.names);
            if batch.done && !results.file_names.is_empty() {
                // One insertion pass for all names (see `step_names`).
                queue.push_back(Item::Names(results.file_names.clone()));
            }
            results.truncated = batch.truncated;
            results.total_matches = batch.total_matches;
            results.total_files = batch.total_files;
        }
        imp.searching.set(!batch.done);
        self.render_step();
        true
    }

    /// Deliver a complete snapshot as if a worker had produced it (stale ones are dropped).
    #[doc(hidden)]
    pub fn deliver_snapshot(&self, generation: u64, results: SearchResults, done: bool) -> bool {
        self.deliver(Batch {
            generation,
            reset: true,
            groups: results.groups,
            names: results.file_names,
            truncated: results.truncated,
            total_matches: results.total_matches,
            total_files: results.total_files,
            error: None,
            done,
        })
    }

    // ---- incremental rendering -----------------------------------------------------------

    fn rebuild(&self) {
        let imp = self.imp();
        let results = imp.results.borrow().clone();
        let mut queue = imp.queue.borrow_mut();
        queue.clear();
        queue.extend(results.groups.into_iter().map(Item::Group));
        if !imp.searching.get() && !results.file_names.is_empty() {
            queue.push_back(Item::Names(results.file_names));
        }
        drop(queue);
        imp.has_names_group.set(false);
        self.w().store.clear();
        self.render_step();
    }

    /// Insert queued rows for about [`RENDER_BUDGET`]; continue from an idle source.
    fn render_step(&self) {
        let imp = self.imp();
        if let Some(source) = imp.render_source.take() {
            source.remove();
        }
        let deadline = Instant::now() + RENDER_BUDGET;
        while Instant::now() < deadline {
            let item = imp.queue.borrow_mut().pop_front();
            let Some(item) = item else { break };
            let rest = match item {
                Item::Group(group) => self.insert_group(&group),
                Item::Rows {
                    parent,
                    path,
                    matches,
                    next,
                } => self.step_rows(parent, path, matches, next, deadline),
                Item::Names(paths) => self.start_names(paths),
                Item::NameRows(job) => self.step_names(job, deadline),
            };
            if let Some(rest) = rest {
                imp.queue.borrow_mut().push_front(rest);
            }
        }
        self.update_summary();
        if !imp.queue.borrow().is_empty() {
            let weak = self.downgrade();
            let source = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
                if let Some(panel) = weak.upgrade() {
                    panel.imp().render_source.take();
                    panel.render_step();
                }
                glib::ControlFlow::Break
            });
            *imp.render_source.borrow_mut() = Some(source);
        } else if imp.focus_first.get() && !imp.searching.get() {
            imp.focus_first.set(false);
        }
    }

    fn focus_first_row(&self) {
        let w = self.w();
        if w.store.iter_first().is_some() {
            w.tree.grab_focus();
            w.tree.set_cursor(
                &gtk::TreePath::new_first(),
                None::<&gtk::TreeViewColumn>,
                false,
            );
        }
    }

    /// Insert a group's header row; its match rows follow as a resumable item.
    fn insert_group(&self, group: &FileGroup) -> Option<Item> {
        let w = self.w();
        let directory = Path::new(&group.relative_path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tag = if group.is_current {
            "  · current file"
        } else {
            ""
        };
        let name = group
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Count and tag first: a narrow sidebar clips the long directory, not these.
        let markup = format!(
            "<b>{}</b> <span alpha=\"60%\">{}</span><i>{}</i><span alpha=\"60%\">  {}</span>",
            escape(&name),
            group.matches.len(),
            tag,
            escape(&directory)
        );
        let path = group.path.to_string_lossy().into_owned();
        let parent = w.store.insert_with_values(
            None,
            None,
            &[
                (0, &"text-x-generic-symbolic"),
                (1, &""),
                (2, &markup),
                (3, &path),
                (4, &0i32),
                (5, &0i32),
                (6, &0i32),
                (7, &"group"),
            ],
        );
        let reference = gtk::TreeRowReference::new(&w.store, &util::path_of(&w.store, &parent))?;
        Some(Item::Rows {
            parent: reference,
            path,
            matches: group.matches.clone(),
            next: 0,
        })
    }

    /// Insert match rows below a group until the deadline; expand the group when complete.
    fn step_rows(
        &self,
        parent: gtk::TreeRowReference,
        path: String,
        matches: Vec<crate::search::Match>,
        mut next: usize,
        deadline: Instant,
    ) -> Option<Item> {
        let w = self.w();
        let tree_path = parent.path()?;
        let parent_iter = w.store.iter(&tree_path)?;
        while next < matches.len() {
            if next % 4 == 3 && Instant::now() >= deadline {
                return Some(Item::Rows {
                    parent,
                    path,
                    matches,
                    next,
                });
            }
            let m = &matches[next];
            next += 1;
            let preview = &m.line_text;
            let length = preview.chars().count();
            let start = m.column.saturating_sub(m.preview_offset);
            let end = length.min(start + m.length);
            // The ellipsis is decoration, outside the preview's source coordinates.
            let prefix = if m.preview_offset > 0 { "…" } else { "" };
            let markup = format!(
                "{}{}<b><u>{}</u></b>{}",
                prefix,
                escape(char_slice(preview, 0, start)),
                escape(char_slice(preview, start, end)),
                escape(char_slice(preview, end, length))
            );
            w.store.insert_with_values(
                Some(&parent_iter),
                None,
                &[
                    (0, &None::<&str>),
                    (1, &format!("<span alpha=\"60%\">{}</span>", m.line + 1)),
                    (2, &markup),
                    (3, &path),
                    (4, &(m.line as i32)),
                    (5, &(m.column as i32)),
                    (6, &(m.length as i32)),
                    (7, &"match"),
                ],
            );
        }
        w.tree.expand_row(&tree_path, false);
        self.after_row_inserted();
        None
    }

    /// Insert the "File names" header; its rows follow as a resumable item.
    fn start_names(&self, paths: Vec<PathBuf>) -> Option<Item> {
        let w = self.w();
        self.imp().has_names_group.set(true);
        let parent = w.store.insert_with_values(
            None,
            None,
            &[
                (0, &"folder-saved-search-symbolic"),
                (1, &""),
                (2, &"<b>File names</b>"),
                (3, &""),
                (4, &0i32),
                (5, &0i32),
                (6, &0i32),
                (7, &"group"),
            ],
        );
        let reference = gtk::TreeRowReference::new(&w.store, &util::path_of(&w.store, &parent))?;
        Some(Item::NameRows(NameJob {
            parent: reference,
            rev: paths.len(),
            paths,
            pos: 0,
            anchor: None,
        }))
    }

    /// File-name rows. `GtkTreeStore` costs O(position) per insertion, so a long list is
    /// built as: the first [`NAME_HEAD`] rows in order, then the rest from last to first
    /// right after that head, which keeps every insertion O(head) and the order exact.
    fn step_names(&self, mut job: NameJob, deadline: Instant) -> Option<Item> {
        let w = self.w();
        let tree_path = job.parent.path()?;
        let parent = w.store.iter(&tree_path)?;
        let root = self.imp().root.borrow().clone();
        let head = job.paths.len().min(NAME_HEAD);
        let mut counter = 0u32;
        loop {
            counter += 1;
            if counter & 3 == 0 && Instant::now() >= deadline {
                return Some(Item::NameRows(job));
            }
            let index = if job.pos < head {
                job.pos += 1;
                job.pos - 1
            } else if job.rev > job.pos {
                job.rev -= 1;
                job.rev
            } else {
                break;
            };
            let path = &job.paths[index];
            let relative = crate::search::relative_to(path, &root);
            let values: [(u32, &dyn ToValue); 8] = [
                (0, &"text-x-generic-symbolic"),
                (1, &""),
                (2, &escape(&relative)),
                (3, &path.to_string_lossy().into_owned()),
                (4, &0i32),
                (5, &0i32),
                (6, &0i32),
                (7, &"file"),
            ];
            let row = if index < head {
                let row = w.store.insert_with_values(Some(&parent), None, &values);
                job.anchor = Some(row);
                row
            } else {
                let row = w.store.insert_after(Some(&parent), job.anchor.as_ref());
                w.store.set(&row, &values);
                row
            };
            let _ = row;
        }
        w.tree.expand_row(&tree_path, false);
        self.after_row_inserted();
        None
    }

    fn after_row_inserted(&self) {
        if self.imp().focus_first.replace(false) {
            self.focus_first_row();
        }
    }

    fn update_summary(&self) {
        let imp = self.imp();
        let results = imp.results.borrow();
        let summary = if results.truncated {
            format!("Showing the first {} results", results.total_matches)
        } else if results.total_matches > 0 {
            format!(
                "{} results in {} files",
                results.total_matches, results.total_files
            )
        } else if !results.file_names.is_empty() {
            format!("{} file name results", results.file_names.len())
        } else if imp.searching.get() {
            "Searching…".to_owned()
        } else {
            "No results".to_owned()
        };
        drop(results);
        self.set_summary(&summary);
    }

    // ---- activation and keys --------------------------------------------------------------

    /// Toggle a group header, or emit `result-activated` for a match or file-name row.
    #[doc(hidden)]
    pub fn activate(&self, path: &gtk::TreePath) {
        let w = self.w();
        let Some(iter) = w.store.iter(path) else {
            return;
        };
        let kind = util::string(&w.store, &iter, 7);
        if kind == "group" {
            if w.tree.row_expanded(path) {
                w.tree.collapse_row(path);
            } else {
                w.tree.expand_row(path, false);
            }
        } else {
            let file = util::string(&w.store, &iter, 3);
            let (line, column, length): (i32, i32, i32) = (
                util::int(&w.store, &iter, 4),
                util::int(&w.store, &iter, 5),
                util::int(&w.store, &iter, 6),
            );
            self.emit_by_name::<()>("result-activated", &[&file, &line, &column, &length]);
        }
    }

    fn entry_activate(&self) {
        self.search_now(None);
        let w = self.w();
        if w.store.iter_first().is_some() {
            self.focus_first_row();
        } else {
            self.imp().focus_first.set(true);
        }
    }

    fn key(&self, widget: &gtk::Widget, event: &gdk::EventKey) -> glib::Propagation {
        let w = self.w();
        let key = event.keyval();
        if key == gdk::keys::constants::Escape {
            self.clear();
            w.entry.grab_focus();
            self.emit_by_name::<()>("escaped", &[]);
            return glib::Propagation::Stop;
        }
        if widget == w.entry.upcast_ref::<gtk::Widget>() && key == gdk::keys::constants::Down {
            w.tree.grab_focus();
            self.focus_first_row();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    // ---- typed signal helpers ----------------------------------------------------------------

    /// `f(panel, path, line, column, length)` for a clicked or activated result.
    pub fn connect_result_activated<F: Fn(&Self, &Path, i32, i32, i32) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("result-activated", false, move |args| {
            let panel = args[0].get::<SearchPanel>().ok()?;
            let path: String = args[1].get().ok()?;
            f(
                &panel,
                Path::new(&path),
                args[2].get().ok()?,
                args[3].get().ok()?,
                args[4].get().ok()?,
            );
            None
        })
    }

    pub fn connect_escaped<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("escaped", false, move |args| {
            f(&args[0].get::<SearchPanel>().ok()?);
            None
        })
    }
}

fn absolute_or_empty(path: &Path) -> PathBuf {
    if path.as_os_str().is_empty() {
        PathBuf::new()
    } else {
        impro_core::fsops::absolute(path)
    }
}

/// Resolve Pango colours at paint time so live theme changes keep hits fresh.
fn style_result(
    tree: &gtk::TreeView,
    cell: &gtk::CellRenderer,
    model: &gtk::TreeModel,
    iter: &gtk::TreeIter,
) {
    let kind = util::string(model, iter, 7);
    let group = kind == "group";
    let context = tree.style_context();
    let colour = context.lookup_color(if group {
        "impro_search_group"
    } else {
        "impro_search_match"
    });
    let mut markup = util::string(model, iter, 2);
    if let Some(colour) = colour {
        let hex = hex(&colour);
        if group {
            markup = format!("<span foreground=\"{hex}\" weight=\"600\">{markup}</span>");
        } else if kind == "match" {
            markup = markup
                .replacen("<b><u>", &format!("<span foreground=\"{hex}\"><b><u>"), 1)
                .replacen("</u></b>", "</u></b></span>", 1);
        }
    }
    cell.set_property("markup", markup);
    if group {
        let attributes: Option<pango::AttrList> = cell.property("attributes");
        let attributes = attributes.unwrap_or_default();
        attributes.change(pango::AttrSize::new_size_absolute(12 * pango::SCALE));
        attributes.change(pango::AttrInt::new_weight(pango::Weight::Semibold));
        cell.set_property("attributes", &attributes);
    }
}

/// Worker body: stream a search to the panel in time-spaced batches.
fn run_search(
    generation: u64,
    root: PathBuf,
    query: String,
    options: SearchOptions,
    has_provider: bool,
    cancel: CancelToken,
    sender: glib::Sender<Msg>,
) {
    struct Flusher<'a> {
        generation: u64,
        sender: &'a glib::Sender<Msg>,
        sent_groups: usize,
        sent_names: usize,
        started: bool,
        last: Instant,
    }
    impl Flusher<'_> {
        fn flush(&mut self, results: &SearchResults, done: bool) {
            let batch = Batch {
                generation: self.generation,
                reset: !self.started,
                groups: results.groups[self.sent_groups..].to_vec(),
                names: results.file_names[self.sent_names..].to_vec(),
                truncated: results.truncated,
                total_matches: results.total_matches,
                total_files: results.total_files,
                error: None,
                done,
            };
            self.started = true;
            self.sent_groups = results.groups.len();
            self.sent_names = results.file_names.len();
            self.last = Instant::now();
            let _ = self.sender.send(Msg::Batch(batch));
        }
        fn progress(&mut self, results: &SearchResults) {
            // The first hit goes out at once; later ones in time-spaced batches.
            let first_hit = self.sent_groups == 0 && !results.groups.is_empty();
            if !self.started || first_hit || self.last.elapsed() >= BATCH_INTERVAL {
                self.flush(results, false);
            }
        }
    }
    let mut flusher = Flusher {
        generation,
        sender: &sender,
        sent_groups: 0,
        sent_names: 0,
        started: false,
        last: Instant::now(),
    };
    let overlay_sender = sender.clone();
    let overlay_cancel = cancel.clone();
    let mut overlay = move |paths: &[PathBuf]| -> Vec<Option<String>> {
        let none = || vec![None; paths.len()];
        let (reply, answer) = mpsc::channel();
        let request = Msg::Overlay {
            generation,
            paths: paths.to_vec(),
            reply,
        };
        if overlay_sender.send(request).is_err() {
            return none();
        }
        loop {
            match answer.recv_timeout(Duration::from_millis(20)) {
                Ok(texts) => return texts,
                Err(mpsc::RecvTimeoutError::Timeout) if !overlay_cancel.is_cancelled() => {}
                Err(_) => return none(),
            }
        }
    };
    let outcome = search_stream(
        &root,
        &query,
        &options,
        has_provider.then_some(&mut overlay as &mut dyn FnMut(&[PathBuf]) -> Vec<Option<String>>),
        &cancel,
        &mut |results| flusher.progress(results),
    );
    match outcome {
        Ok(results) => {
            if !cancel.is_cancelled() {
                flusher.flush(&results, true);
            }
        }
        Err(error) => {
            let _ = sender.send(Msg::Batch(Batch {
                generation,
                reset: true,
                groups: Vec::new(),
                names: Vec::new(),
                truncated: false,
                total_matches: 0,
                total_files: 0,
                error: Some(error),
                done: true,
            }));
        }
    }
}
