use gtk::prelude::*;
use impro_chrome::{Command, CommandPalette, OverlayHost, QuickSwitcher, TitleBar};
use impro_core::{identity, notes, settings, themes};
use impro_editor::{EditorPanel, EditorTab};
use impro_explorer::ExplorerPanel;
use impro_gitviews::{CommitView, GitGraphView};
use impro_graph::GraphView;
use impro_settings_ui::SettingsDialog;
use impro_terminal::TerminalPanel;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap, HashSet},
    ops::Deref,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
};

/// Cloneable application-window owner; callbacks hold weak references to it.
#[derive(Clone)]
pub struct MainWindow(pub(crate) Rc<WindowWidgets>);

/// Public GTK composition and private application state.
pub struct WindowWidgets {
    pub window: gtk::ApplicationWindow,
    pub explorer: ExplorerPanel,
    pub editor: EditorPanel,
    pub terminal_bottom: TerminalPanel,
    pub terminal_right: TerminalPanel,
    pub dock_bottom: gtk::Paned,
    pub dock: gtk::Paned,
    pub sidebar: gtk::Paned,
    pub titlebar: TitleBar,
    pub menu_bar: gtk::MenuBar,
    pub overlay: OverlayHost,
    pub(crate) root: RefCell<PathBuf>,
    root_revision: Cell<u64>,
    pub(crate) values: RefCell<settings::Values>,
    pub(crate) commands: RefCell<Vec<Command>>,
    pub(crate) settings_dialog: RefCell<Option<SettingsDialog>>,
    pub(crate) palette: RefCell<Option<CommandPalette>>,
    pub(crate) palette_cache_idle: RefCell<Option<glib::SourceId>>,
    pub(crate) switcher: RefCell<Option<QuickSwitcher>>,
    pub(crate) recent: RefCell<Vec<PathBuf>>,
    pub(crate) switcher_epoch: Cell<u64>,
    pub(crate) graphs: RefCell<Vec<(TerminalPanel, GraphView)>>,
    pub(crate) gitgraphs: RefCell<Vec<(TerminalPanel, GitGraphView)>>,
    pub(crate) commits: RefCell<BTreeMap<String, CommitView>>,
    pub(crate) commit_pending: RefCell<HashSet<String>>,
    pub(crate) epoch: Cell<u64>,
    pub(crate) errors: RefCell<HashSet<(String, String)>>,
    pub(crate) full_notice: Cell<bool>,
    pub(crate) disabled: RefCell<HashMap<(bool, String), usize>>,
    pub(crate) explorer_was_visible: Cell<bool>,
    pub(crate) fullscreen: RefCell<Option<crate::docks::SavedLayout>>,
    pub(crate) size_pending: [Cell<bool>; 2],
    pub(crate) dock_size_idle: [RefCell<Option<glib::SourceId>>; 2],
    pub(crate) toolbars: RefCell<HashMap<usize, impro_ui::notes::NoteToolbar>>,
    pub(crate) connected_tabs: RefCell<HashSet<usize>>,
    pub(crate) tree_refresh: RefCell<Option<glib::SourceId>>,
    pub(crate) closed: Cell<bool>,
    pub(crate) quitting: Cell<bool>,
    pub(crate) allocation_us: Cell<u128>,
    pub(crate) startup_shell_pending: Cell<bool>,
}

impl Deref for MainWindow {
    type Target = WindowWidgets;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl MainWindow {
    /// Load settings before construction. Applications may pass their startup snapshot instead.
    pub fn new(app: &gtk::Application, root: impl AsRef<Path>) -> Self {
        Self::with_values(app, root, crate::preferences::load_values())
    }

    /// Build the complete workspace synchronously; expensive views remain lazy.
    pub fn with_values(
        app: &gtk::Application,
        root: impl AsRef<Path>,
        mut values: settings::Values,
    ) -> Self {
        let started = std::time::Instant::now();
        let trace = |phase: &str| {
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            if std::env::var_os("IMPRO_QA_TIMING").is_some() {
                println!("window-{phase}-ms={elapsed:.3}");
            } else if std::env::var_os("IMPRO_QA_TRACE").is_some() {
                eprintln!("window-{phase}-ms={elapsed:.3}");
            }
        };
        crate::style::init();
        for (key, default) in settings::defaults() {
            let value = settings::normalize(key, values.get(key).unwrap_or(default))
                .unwrap_or_else(|_| default.clone());
            values.insert(key.clone(), value);
        }
        let style_error = impro_ui::apply_theme(
            themes::get_theme(values["theme"].as_str()),
            values["accent"].as_str(),
            values["ui_font"].as_str(),
            values["editor_font"].as_str(),
        )
        .err();
        trace("style");
        let root = root.as_ref().to_path_buf();
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(identity::WINDOW_TITLE);
        window.set_default_size(1200, 750);
        trace("window");
        let explorer = ExplorerPanel::new(&root);
        trace("explorer");
        let editor = EditorPanel::new(Some(&root));
        trace("editor");
        let size = values["terminal_font_size"].as_i64().unwrap_or(11) as i32;
        let terminal_bottom = TerminalPanel::new(&root, false, false, size);
        let terminal_right = TerminalPanel::new(&root, false, true, size);
        trace("docks");
        terminal_right.set_no_show_all(true);
        let autostart = values["terminal_autostart"] == true && values["feature_terminal"] == true;
        terminal_bottom.set_no_show_all(!autostart);
        let dock_bottom = gtk::Paned::new(gtk::Orientation::Vertical);
        dock_bottom.pack1(&editor, true, true);
        dock_bottom.pack2(&terminal_bottom, false, true);
        let dock = gtk::Paned::new(gtk::Orientation::Horizontal);
        dock.pack1(&dock_bottom, true, true);
        dock.pack2(&terminal_right, false, true);
        let sidebar = gtk::Paned::new(gtk::Orientation::Horizontal);
        sidebar.pack1(&explorer, false, false);
        sidebar.pack2(&dock, true, false);
        sidebar.set_position(300);
        let menu_bar = gtk::MenuBar::new();
        let titlebar = TitleBar::new(&menu_bar);
        window.set_titlebar(Some(&titlebar));
        let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
        layout.pack_start(&sidebar, true, true, 0);
        let overlay = OverlayHost::new(Some(layout.upcast_ref()));
        window.add(overlay.widget());
        let this = Self(Rc::new(WindowWidgets {
            window,
            explorer,
            editor,
            terminal_bottom,
            terminal_right,
            dock_bottom,
            dock,
            sidebar,
            titlebar,
            menu_bar,
            overlay,
            root: RefCell::new(root),
            root_revision: Cell::new(0),
            values: RefCell::new(values),
            commands: RefCell::new(Vec::new()),
            settings_dialog: RefCell::new(None),
            palette: RefCell::new(None),
            palette_cache_idle: RefCell::new(None),
            switcher: RefCell::new(None),
            recent: RefCell::new(Vec::new()),
            switcher_epoch: Cell::new(0),
            graphs: RefCell::new(Vec::new()),
            gitgraphs: RefCell::new(Vec::new()),
            commits: RefCell::new(BTreeMap::new()),
            commit_pending: RefCell::new(HashSet::new()),
            epoch: Cell::new(0),
            errors: RefCell::new(HashSet::new()),
            full_notice: Cell::new(false),
            disabled: RefCell::new(HashMap::new()),
            explorer_was_visible: Cell::new(true),
            fullscreen: RefCell::new(None),
            size_pending: [Cell::new(true), Cell::new(true)],
            dock_size_idle: [RefCell::new(None), RefCell::new(None)],
            toolbars: RefCell::new(HashMap::new()),
            connected_tabs: RefCell::new(HashSet::new()),
            tree_refresh: RefCell::new(None),
            closed: Cell::new(false),
            quitting: Cell::new(false),
            allocation_us: Cell::new(0),
            startup_shell_pending: Cell::new(false),
        }));
        trace("skeleton");
        this.build_commands();
        trace("commands");
        this.connect_panels();
        this.connect_docks();
        trace("signals");
        this.apply_initial_preferences();
        trace("preferences");
        if let Some(error) = style_error {
            this.notify(&error.to_string(), "error");
        }
        // Keep the owner alive until native destruction, then break the ownership cycle.
        let keep = RefCell::new(Some(this.clone()));
        this.window.connect_destroy(move |_| {
            if let Some(owner) = keep.borrow_mut().take() {
                owner.shutdown();
            }
        });
        let weak = this.weak();
        this.window.connect_delete_event(move |_, _| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.request_quit();
            }
            glib::Propagation::Stop
        });
        let weak = this.weak();
        this.window.connect_key_press_event(move |_, event| {
            weak.upgrade()
                .map(Self)
                .is_some_and(|this| this.handle_key(event))
                .into()
        });
        let weak = this.weak();
        impro_settings_ui::set_save_failed_handler(move |path| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.notify(
                    &format!("Could not save settings to {}", path.display()),
                    "error",
                );
            }
        });
        // Default shell chrome is immediate; custom shells are checked on a worker first.
        this.configure_shell(autostart);
        trace("terminal-pane");
        impro_ui::icons::prepare_widget(&this.explorer);
        crate::startup_icons::prepare(&this);
        trace("icons");
        let weak = this.weak();
        let painted = Cell::new(false);
        this.window.connect_draw(move |_, _| {
            if let Some(this) = weak.upgrade().map(Self) {
                if this.layout_ready() && !painted.replace(true) {
                    // LOW runs after GDK's paint and the QA capture idle.
                    this.stage_command_palette_cache();
                }
            }
            glib::Propagation::Proceed
        });
        this.window.set_title(identity::WINDOW_TITLE);
        this
    }

    pub(crate) fn weak(&self) -> Weak<WindowWidgets> {
        Rc::downgrade(&self.0)
    }
    pub(crate) fn callback(&self, f: impl Fn(&Self) + 'static) -> impl Fn() + 'static {
        let weak = self.weak();
        move || {
            if let Some(this) = weak.upgrade().map(Self) {
                if !this.closed.get() {
                    f(&this);
                }
            }
        }
    }
    pub fn root(&self) -> PathBuf {
        self.root.borrow().clone()
    }
    pub fn values(&self) -> settings::Values {
        self.values.borrow().clone()
    }
    pub fn show(&self) {
        self.window.show_all();
        self.window.present();
    }
    pub fn notify(&self, text: &str, level: &str) {
        let _ = self.overlay.notify_default(text, level);
    }
    pub fn feature_enabled(&self, name: &str) -> bool {
        self.values
            .borrow()
            .get(&format!("feature_{name}"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }
    pub(crate) fn require_feature(&self, name: &str) -> bool {
        if self.feature_enabled(name) {
            return true;
        }
        let label = match name {
            "explorer" => "File explorer",
            "terminal" => "Terminal",
            "graph" => "Graph view",
            "gitgraph" => "Git graph",
            _ => "Notes",
        };
        self.notify(&format!("{label} is turned off in Settings."), "info");
        false
    }

    fn connect_panels(&self) {
        let weak = self.weak();
        self.explorer.connect_file_activated(move |_, path| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.editor.open_file(path);
            }
        });
        let weak = self.weak();
        self.explorer
            .connect_search_result_activated(move |_, path, line, column, length| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.restore_terminal_fullscreen();
                    this.open_file_at(path, line, column, length);
                }
            });
        let weak = self.weak();
        self.explorer.connect_search_escaped(move |_| {
            if let Some(this) = weak.upgrade().map(Self) {
                if !this.editor.focus_current() {
                    this.explorer.focus_tree();
                }
            }
        });
        let weak = self.weak();
        self.explorer.set_text_provider(Some(Rc::new(move |path| {
            weak.upgrade()
                .map(Self)
                .and_then(|this| this.editor.buffer_text(path))
        })));
        let weak = self.weak();
        self.explorer.connect_path_renamed(move |_, old, new| {
            if let Some(this) = weak.upgrade().map(Self) {
                if !this.editor.on_file_renamed(old, new) {
                    this.editor.on_path_renamed(old, new);
                }
                for view in this.graph_views() {
                    view.path_renamed(old, new);
                }
                this.sync_note_toolbars();
            }
        });
        let weak = self.weak();
        self.explorer.connect_path_deleted(move |_, path| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.editor.on_path_deleted(path);
                for view in this.graph_views() {
                    view.path_deleted(path);
                }
                this.sync_note_toolbars();
            }
        });
        let weak = self.weak();
        self.editor.connect_current_file_changed(move |_, path| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.current_file_changed(path);
            }
        });
        let weak = self.weak();
        self.editor.connect_file_saved(move |_, path| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.explorer.refresh_git();
                this.explorer.refresh_notes();
                for view in this.graph_views() {
                    view.file_saved(path);
                }
                for view in this.gitgraph_views() {
                    view.refresh();
                }
            }
        });
        let weak = self.weak();
        self.editor.connect_tab_closed(move |_, path| {
            if let Some(this) = weak.upgrade().map(Self) {
                for view in this.graph_views() {
                    view.buffer_changed(path, None);
                    view.file_saved(path);
                }
                this.sync_note_toolbars();
            }
        });
        let weak = self.weak();
        self.editor.connect_view_tab_closed(move |_, key| {
            if let Some(this) = weak.upgrade().map(Self) {
                let view = this.commits.borrow_mut().remove(key);
                if let Some(view) = view {
                    view.shutdown();
                }
                this.sync_git_selection();
            }
        });
        let weak = self.weak();
        self.editor.notebook().connect_switch_page(move |_, _, _| {
            let weak = weak.clone();
            glib::idle_add_local_once(move || {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.sync_git_selection();
                }
            });
        });
        let weak = self.weak();
        self.editor.connect_font_size_changed(move |_, size| {
            if let Some(this) = weak.upgrade().map(Self) {
                for view in this.commit_views() {
                    view.set_font_size(size);
                }
            }
        });
        let weak = self.weak();
        self.editor.connect_error(move |_, error| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.notify(error, "error");
            }
        });
        let weak = self.weak();
        self.window.connect_focus_in_event(move |_, _| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.explorer.refresh_git();
                for view in this.gitgraph_views() {
                    view.refresh();
                }
            }
            glib::Propagation::Proceed
        });
        let weak = self.weak();
        self.explorer.store().connect_row_inserted(move |_, _, _| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.schedule_tree_refresh();
            }
        });
        let weak = self.weak();
        self.explorer.store().connect_row_deleted(move |_, _| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.schedule_tree_refresh();
            }
        });
    }

    fn current_file_changed(&self, path: Option<PathBuf>) {
        self.explorer.set_current_file(path.as_deref());
        self.explorer.refresh_git();
        for view in self.graph_views() {
            view.set_active_file(path.as_deref());
        }
        for tab in self.editor.tabs() {
            let id = Rc::as_ptr(&tab) as usize;
            if self.connected_tabs.borrow_mut().insert(id) {
                let weak = self.weak();
                let t = Rc::downgrade(&tab);
                tab.buffer.connect_changed(move |_| {
                    if let (Some(this), Some(tab)) = (weak.upgrade().map(Self), t.upgrade()) {
                        if !tab.deleted() && !tab.is_closed() {
                            for view in this.graph_views() {
                                view.buffer_changed(&tab.path(), Some(tab.text()));
                            }
                        }
                    }
                });
            }
        }
        if let Some(path) = path {
            let mut recent = self.recent.borrow_mut();
            recent.retain(|p| *p != path);
            recent.insert(0, path);
            recent.truncate(50);
        }
        self.sync_note_toolbars();
        self.sync_git_selection();
    }

    pub fn show_notes(&self) {
        if !self.require_feature("notes") || !self.require_feature("explorer") {
            return;
        }
        self.restore_terminal_fullscreen();
        Self::set_visible(&self.explorer, true);
        self.explorer.focus_notes();
    }
    pub fn new_note(&self) {
        if !self.require_feature("notes") || !self.require_feature("explorer") {
            return;
        }
        self.show_notes();
        self.explorer.new_note_dialog();
    }
    pub fn apply_note_command(&self, id: &str) {
        self.apply_note_command_to(id, self.editor.current_tab());
    }
    fn apply_note_command_to(&self, id: &str, tab: Option<Rc<EditorTab>>) {
        if !self.require_feature("notes") {
            return;
        }
        let Some(tab) = tab.filter(|t| t.markdown_mode().is_some()) else {
            self.notify("Open a note or Markdown file to use note commands.", "info");
            return;
        };
        if !tab.view.is_editable() {
            return;
        }
        match impro_ui::notes::apply_to_buffer(id, &tab.buffer) {
            Ok(edit) if id == "table" && tab.markdown_mode() == Some("preview") => {
                let offset = edit.start + edit.text.chars().count();
                tab.buffer
                    .place_cursor(&tab.buffer.iter_at_offset(offset as i32));
            }
            Err(error) => self.notify(&error.to_string(), "error"),
            _ => {}
        }
        if let Some(mark) = tab.buffer.get_insert() {
            tab.view.scroll_mark_onscreen(&mark);
        }
        tab.view.grab_focus();
    }
    pub fn note_toolbar(&self, tab: &Rc<EditorTab>) -> Option<impro_ui::notes::NoteToolbar> {
        self.toolbars
            .borrow()
            .get(&(Rc::as_ptr(tab) as usize))
            .cloned()
    }
    pub(crate) fn sync_note_toolbars(&self) {
        let values = self.values();
        let enabled = self.feature_enabled("notes") && values["notes_toolbar"] == true;
        self.editor
            .set_selection_toolbar(enabled && values["notes_selection_toolbar"] == true);
        let tabs = self.editor.tabs();
        let ids: HashSet<_> = tabs.iter().map(|t| Rc::as_ptr(t) as usize).collect();
        let stale: Vec<_> = self
            .toolbars
            .borrow()
            .keys()
            .copied()
            .filter(|id| !ids.contains(id))
            .collect();
        self.connected_tabs
            .borrow_mut()
            .retain(|id| ids.contains(id));
        for id in stale {
            self.toolbars.borrow_mut().remove(&id);
            self.connected_tabs.borrow_mut().remove(&id);
        }
        for tab in tabs {
            let id = Rc::as_ptr(&tab) as usize;
            let wanted =
                enabled && !tab.deleted() && notes::is_note(self.root(), Some(&tab.path()));
            let toolbar = self.toolbars.borrow().get(&id).cloned();
            if wanted && toolbar.is_none() {
                let toolbar = impro_ui::notes::NoteToolbar::new();
                let weak = self.weak();
                let t = Rc::downgrade(&tab);
                toolbar.connect_command(move |_, command| {
                    if let Some(this) = weak.upgrade().map(Self) {
                        this.apply_note_command_to(command, t.upgrade());
                    }
                });
                let scroll = tab.source_scroll.downgrade();
                toolbar.connect_size_allocate(move |_, rect| {
                    if let Some(scroll) = scroll.upgrade() {
                        if scroll.margin_top() != rect.height() {
                            scroll.set_margin_top(rect.height());
                        }
                    }
                });
                tab.page.add_overlay(&toolbar);
                tab.page.reorder_overlay(&toolbar, 0);
                tab.page.set_overlay_pass_through(&toolbar, false);
                toolbar.show_all();
                tab.source_scroll
                    .set_margin_top(toolbar.preferred_height().1);
                self.toolbars.borrow_mut().insert(id, toolbar);
            } else if !wanted {
                let toolbar = self.toolbars.borrow_mut().remove(&id);
                if let Some(toolbar) = toolbar {
                    unsafe {
                        toolbar.destroy();
                    }
                    tab.source_scroll.set_margin_top(0);
                }
            }
        }
    }

    pub fn toggle_markdown_view(&self) {
        if let Some(mode) = self.editor.markdown_mode(None) {
            let _ = self.editor.set_markdown_mode(
                if mode == "source" {
                    "preview"
                } else {
                    "source"
                },
                None,
            );
        }
    }
    pub fn search_in_files(&self) {
        if !self.require_feature("explorer") {
            return;
        }
        self.restore_terminal_fullscreen();
        Self::set_visible(&self.explorer, true);
        let seed = self.editor.selected_text();
        let seed = if seed.trim().is_empty()
            || seed.contains(['\n', '\r'])
            || seed.chars().count() > 200
        {
            ""
        } else {
            &seed
        };
        self.explorer.focus_search(Some(seed));
    }
    /// Open a search/location target and centre it after its first allocated draw.
    pub fn open_file_at(&self, path: &Path, line: i32, column: i32, length: i32) {
        self.editor.open_file_at(path, line, column, length);
        self.editor.open_file_with(path, move |tab| {
            let Some(tab) = tab else {
                return;
            };
            let weak = Rc::downgrade(&tab);
            let handler = Rc::new(RefCell::new(None));
            let connection = handler.clone();
            let armed = Cell::new(false);
            let id = tab.view.connect_local("draw", true, move |_| {
                if !armed.replace(true) {
                    let weak = weak.clone();
                    let connection = connection.clone();
                    glib::idle_add_local_once(move || {
                        if let Some(tab) = weak.upgrade().filter(|t| !t.is_closed()) {
                            if let Some(id) = connection.borrow_mut().take() {
                                tab.view.disconnect(id);
                            }
                            let mut at = tab.buffer.iter_at_offset(tab.buffer.cursor_position());
                            tab.view.scroll_to_iter(&mut at, 0.0, true, 0.0, 0.5);
                        }
                    });
                }
                Some(false.to_value())
            });
            *handler.borrow_mut() = Some(id);
            tab.view.queue_draw();
        });
    }
    pub fn set_root(&self, path: impl AsRef<Path>) {
        let revision = self.root_revision.get() + 1;
        self.root_revision.set(revision);
        let path = path.as_ref().to_path_buf();
        let weak = self.weak();
        self.worker(
            move || {
                path.canonicalize().and_then(|p| {
                    if p.is_dir() {
                        Ok(p)
                    } else {
                        Err(std::io::Error::other("not a directory"))
                    }
                })
            },
            move |result| {
                if let Some(this) = weak.upgrade().map(Self) {
                    if this.root_revision.get() != revision {
                        return;
                    }
                    match result {
                        Ok(root) => this.apply_root(root),
                        Err(error) => this.notify(&error.to_string(), "error"),
                    }
                }
            },
        );
    }
    fn apply_root(&self, root: PathBuf) {
        self.restore_terminal_fullscreen();
        self.close_commit_tabs();
        self.switcher_epoch.set(self.switcher_epoch.get() + 1);
        *self.root.borrow_mut() = root.clone();
        self.recent.borrow_mut().clear();
        self.editor.set_root(Some(&root));
        self.explorer.set_root(&root);
        self.terminal_bottom.set_cwd(&root);
        self.terminal_right.set_cwd(&root);
        for view in self.graph_views() {
            view.set_root(&root);
            view.set_active_file(self.editor.current_path().as_deref());
        }
        for view in self.gitgraph_views() {
            view.set_root(&root);
        }
        if let Some(dialog) = self.settings_dialog() {
            dialog.set_project_root(Some(&root));
        }
        let switcher = self.switcher.borrow().clone();
        if let Some(switcher) = switcher {
            switcher.set_root(&root);
            switcher.set_files(Vec::new(), Vec::new());
        }
        self.sync_note_toolbars();
        self.window.set_title(identity::WINDOW_TITLE);
    }
    pub fn open_folder(&self) {
        self.restore_terminal_fullscreen();
        let dialog = gtk::FileChooserDialog::new(
            Some("Open Folder"),
            Some(&self.window),
            gtk::FileChooserAction::SelectFolder,
        );
        dialog.add_buttons(&[
            ("Cancel", gtk::ResponseType::Cancel),
            ("Open", gtk::ResponseType::Ok),
        ]);
        dialog.set_current_folder(self.root());
        let weak = self.weak();
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Ok {
                if let (Some(this), Some(path)) = (weak.upgrade().map(Self), dialog.filename()) {
                    this.set_root(path);
                }
            }
            unsafe {
                dialog.destroy();
            }
        });
        dialog.show();
    }
    pub fn request_quit(&self) {
        if self.quitting.replace(true) {
            return;
        }
        let weak = self.weak();
        self.editor.request_quit(move |ok| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.quitting.set(false);
                if ok {
                    impro_settings_ui::flush();
                    unsafe {
                        this.window.destroy();
                    }
                }
            }
        });
    }
    pub fn shutdown(&self) {
        if self.closed.replace(true) {
            return;
        }
        if let Some(source) = self.palette_cache_idle.borrow_mut().take() {
            source.remove();
        }
        for source in &self.dock_size_idle {
            if let Some(source) = source.borrow_mut().take() {
                source.remove();
            }
        }
        self.epoch.set(self.epoch.get() + 1);
        self.switcher_epoch.set(self.switcher_epoch.get() + 1);
        if let Some(source) = self.tree_refresh.borrow_mut().take() {
            source.remove();
        }
        for view in self.graph_views() {
            view.shutdown();
        }
        for view in self.gitgraph_views() {
            view.shutdown();
        }
        self.close_commit_tabs();
        self.terminal_bottom.shutdown();
        self.terminal_right.shutdown();
        self.editor.shutdown();
        if let Some(dialog) = self.settings_dialog() {
            unsafe {
                dialog.destroy();
            }
        }
        self.overlay.clear_notices();
        impro_settings_ui::flush();
    }
    #[allow(deprecated)]
    pub(crate) fn worker<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(T) + 'static,
    ) {
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        let weak = self.weak();
        let mut done = Some(done);
        rx.attach(None, move |value| {
            if weak.upgrade().is_some_and(|this| !this.closed.get()) {
                if let Some(done) = done.take() {
                    done(value);
                }
            }
            glib::ControlFlow::Break
        });
        impro_explorer::pool::spawn(impro_explorer::pool::Lane::Background, move || {
            let _ = tx.send(work());
        });
    }
}
