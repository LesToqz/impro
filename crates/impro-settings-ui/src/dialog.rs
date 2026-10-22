//! The settings window: chrome, model, search and keyboard handling. Page contents
//! live in `pages.rs`; the model (theme, sizes, values, graph) is independent of the
//! widgets so pages can be built lazily and synced from it on first visit.
use crate::{
    graph_prefs::{graph_values_equal, normalize_graph},
    writer,
};
use atk::prelude::*;
use gtk::prelude::*;
use impro_core::{
    settings::{self, Values},
    themes,
};
use impro_ui::widgets::{CenteredColumn, NavList, SearchField, SettingCard, SettingRow};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    ops::Deref,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
};

/// `(id, title, icon)` of every page, in navigation order.
pub const PAGES: [(&str, &str, &str); 9] = [
    (
        "appearance",
        "Appearance",
        "preferences-desktop-appearance-symbolic",
    ),
    ("editor", "Editor", "accessories-text-editor-symbolic"),
    ("files", "File explorer", "folder-symbolic"),
    ("notes", "Notes", "text-x-generic-symbolic"),
    ("terminal", "Terminal", "utilities-terminal-symbolic"),
    ("graph", "Graph view", "view-grid-symbolic"),
    ("gitgraph", "Git graph", "view-list-symbolic"),
    ("hotkeys", "Hotkeys", "input-keyboard-symbolic"),
    ("about", "About", "help-about-symbolic"),
];
/// Navigation pane width: fraction of the window, minimum, maximum.
pub const NAV_WIDTH: (f64, i32, i32) = (0.25, 200, 250);
/// Below this column width rows stack their controls (spec 4.15).
pub const NARROW_CONTENT: i32 = 400;
/// Default list of the "Ignored folders" row (the folders Search in Files skips).
pub const SKIP_DIRS: [&str; 12] = [
    ".git",
    ".hg",
    ".svn",
    "__pycache__",
    "node_modules",
    ".venv",
    "venv",
    ".mypy_cache",
    ".pytest_cache",
    ".tox",
    ".idea",
    ".ceo",
];

/// Optional construction inputs besides the parent window.
#[derive(Clone, Debug, Default)]
pub struct DialogOptions {
    /// Theme shown initially (the stored theme when `None`).
    pub theme_id: Option<String>,
    /// Font sizes shown initially, over the defaults (the stored sizes when `None`).
    pub sizes: Option<BTreeMap<String, i32>>,
    /// Names listed by the read-only "Ignored folders" row ([`SKIP_DIRS`] when `None`).
    pub ignored_folders: Option<Vec<String>>,
    /// Project root used to show the notes folder (`<project>` when `None`).
    pub project_root: Option<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct Group {
    pub widget: gtk::Box,
    pub card: SettingCard,
}

#[derive(Clone)]
pub(crate) struct Page {
    pub scroll: gtk::ScrolledWindow,
    pub body: gtk::Box,
    pub column: CenteredColumn,
    pub groups: Vec<Group>,
}

#[derive(Clone)]
pub(crate) struct Results {
    pub scroll: gtk::ScrolledWindow,
    pub content: gtk::Box,
    pub sections: Vec<gtk::Box>,
    pub empty_label: gtk::Label,
}

pub(crate) type Setter = Rc<dyn Fn(&Value)>;

#[derive(Default)]
pub(crate) struct Registry {
    pub rows: HashMap<String, SettingRow>,
    pub controls: HashMap<String, gtk::Widget>,
    pub setters: HashMap<String, Setter>,
    pub scales: BTreeMap<String, gtk::Scale>,
    pub value_labels: HashMap<String, gtk::Label>,
    pub keys: Vec<Vec<String>>,
    /// `(page index, feature key)` of every page with an Enable switch.
    pub features: Vec<(usize, String)>,
}

/// Single widgets of particular pages.
#[derive(Default)]
pub(crate) struct Extras {
    pub theme_combo: Option<gtk::ComboBoxText>,
    pub theme_preview: Option<gtk::Box>,
    pub preview_providers: Vec<gtk::CssProvider>,
    pub accent_picker: Option<impro_ui::widgets::SwatchPicker>,
    pub shell_hint: Option<gtk::Label>,
    pub hotkey_filter: Option<SearchField>,
    pub hotkey_box: Option<gtk::Box>,
    pub hotkeys_empty: Option<gtk::Label>,
    pub copy_button: Option<gtk::Button>,
    pub settings_path_label: Option<gtk::Label>,
    pub notes_folder: Option<SettingRow>,
}

type ThemeCb = Rc<dyn Fn(&str)>;
type SizeCb = Rc<dyn Fn(&str, i32)>;
type SettingCb = Rc<dyn Fn(&str, &Value)>;

pub(crate) struct Inner {
    pub window: gtk::Window,
    pub stack: gtk::Stack,
    pub nav_pane: gtk::Box,
    pub search_entry: SearchField,
    pub group_title: gtk::Label,
    pub nav_list: NavList,
    pub updating: Cell<bool>,
    pub searching: Cell<bool>,
    pub narrow: Cell<bool>,
    pub theme_id: RefCell<String>,
    pub values: RefCell<Values>,
    pub sizes: RefCell<BTreeMap<String, i32>>,
    pub graph: RefCell<Values>,
    pub saved_graph: RefCell<Values>,
    pub pages: RefCell<Vec<Option<Page>>>,
    pub reg: RefCell<Registry>,
    pub extras: RefCell<Extras>,
    pub results: RefCell<Option<Results>>,
    pub nav_counts: RefCell<Option<[usize; 9]>>,
    pub hotkeys: RefCell<Vec<(String, String)>>,
    pub ignored_folders: Vec<String>,
    pub project_root: RefCell<Option<PathBuf>>,
    pub on_theme: RefCell<Vec<ThemeCb>>,
    pub on_size: RefCell<Vec<SizeCb>>,
    pub on_setting: RefCell<Vec<SettingCb>>,
    pub prebuild: Cell<bool>,
    pub building: Cell<usize>,
    pub search_query: RefCell<String>,
}

/// The settings window; see the crate documentation for the whole surface.
#[derive(Clone)]
pub struct SettingsDialog {
    pub(crate) inner: Rc<Inner>,
}

impl Deref for SettingsDialog {
    type Target = gtk::Window;
    fn deref(&self) -> &gtk::Window {
        &self.inner.window
    }
}

pub(crate) fn class<W: IsA<gtk::Widget>>(widget: &W, name: &str) -> W {
    widget
        .style_context()
        .add_class(&format!("impro-settings-{name}"));
    widget.clone()
}

pub(crate) fn page_index(id: &str) -> Option<usize> {
    PAGES.iter().position(|(page, _, _)| *page == id)
}

/// The page that owns a settings key (graph keys are `graph.<name>`).
pub(crate) fn page_of(key: &str) -> Option<&'static str> {
    if key.starts_with("graph.") || key == "feature_graph" || key == "graph_default_type" {
        return Some("graph");
    }
    if key.starts_with("gitgraph_") || key == "feature_gitgraph" {
        return Some("gitgraph");
    }
    if key.starts_with("terminal_") || key == "feature_terminal" {
        return Some("terminal");
    }
    if key.starts_with("notes_") || key.starts_with("md_") || key == "feature_notes" {
        return Some("notes");
    }
    Some(match key {
        "theme" | "accent" | "reduce_motion" | "ui_font" => "appearance",
        "editor_font_size" | "editor_font" | "line_numbers" | "highlight_line" | "indent_tabs"
        | "indent_width" | "auto_pair" => "editor",
        "feature_explorer"
        | "explorer_git_colors"
        | "explorer_folders_first"
        | "explorer_compact"
        | "explorer_font_size"
        | "show_hidden"
        | "confirm_delete"
        | "delete_mode"
        | "ignored_folders" => "files",
        "settings_file" => "about",
        _ => return None,
    })
}

fn scrolled(child: &impl IsA<gtk::Widget>) -> (gtk::ScrolledWindow, CenteredColumn) {
    let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let column = CenteredColumn::default();
    column.set_focus_vadjustment(&scroll.vadjustment());
    column.add(child);
    scroll.add(&column);
    (scroll, column)
}

fn prop(handled: bool) -> glib::Propagation {
    if handled {
        glib::Propagation::Stop
    } else {
        glib::Propagation::Proceed
    }
}

impl SettingsDialog {
    /// Build the window chrome. `theme_id` / `sizes` default to the stored values.
    pub fn new(
        parent: Option<&gtk::Window>,
        theme_id: Option<&str>,
        sizes: Option<&BTreeMap<String, i32>>,
    ) -> Self {
        Self::with_options(
            parent,
            DialogOptions {
                theme_id: theme_id.map(str::to_owned),
                sizes: sizes.cloned(),
                ..Default::default()
            },
        )
    }

    pub fn with_options(parent: Option<&gtk::Window>, options: DialogOptions) -> Self {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Settings");
        window.set_transient_for(parent);
        window.set_destroy_with_parent(true);
        window.style_context().add_class("impro-settings");
        window.set_default_size(900, 700);
        window.set_size_request(600, 400);
        let nav_pane = class(&gtk::Box::new(gtk::Orientation::Vertical, 0), "nav");
        let search_entry = SearchField::default();
        let group_title = class(&label("Options", 0.0), "group-title");
        let nav_list = NavList::new();
        let stack = class(&gtk::Stack::new(), "content");
        let stored = settings::get_values();
        let mut sizes = settings::default_font_sizes()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<BTreeMap<_, _>>();
        let initial_sizes = match options.sizes {
            Some(given) => Some(given),
            None => Some(settings::get_font_sizes()),
        };
        for (key, value) in initial_sizes.into_iter().flatten() {
            if sizes.contains_key(&key) {
                sizes.insert(key.clone(), normalize_size(&key, Value::from(value)));
            }
        }
        let theme = themes::get_theme(
            options
                .theme_id
                .as_deref()
                .or(stored.get("theme").and_then(Value::as_str)),
        );
        let mut values = Values::new();
        for key in settings::new_keys() {
            values.insert(key.to_owned(), stored[key].clone());
        }
        let graph = normalize_graph(&settings::get_graph());
        let inner = Rc::new(Inner {
            window,
            stack,
            nav_pane,
            search_entry,
            group_title,
            nav_list,
            updating: Cell::new(true),
            searching: Cell::new(false),
            narrow: Cell::new(false),
            theme_id: RefCell::new(theme.id.clone()),
            values: RefCell::new(values),
            sizes: RefCell::new(sizes),
            saved_graph: RefCell::new(graph.clone()),
            graph: RefCell::new(graph),
            pages: RefCell::new(vec![None; PAGES.len()]),
            reg: RefCell::new(Registry {
                keys: vec![Vec::new(); PAGES.len()],
                ..Default::default()
            }),
            extras: RefCell::default(),
            results: RefCell::new(None),
            nav_counts: RefCell::new(None),
            hotkeys: RefCell::new(crate::hotkeys::default_hotkeys()),
            ignored_folders: options.ignored_folders.unwrap_or_else(|| {
                let mut names: Vec<String> = SKIP_DIRS.iter().map(|s| s.to_string()).collect();
                names.sort();
                names
            }),
            project_root: RefCell::new(options.project_root),
            on_theme: RefCell::default(),
            on_size: RefCell::default(),
            on_setting: RefCell::default(),
            prebuild: Cell::new(false),
            building: Cell::new(0),
            search_query: RefCell::default(),
        });
        inner.build_chrome();
        inner.updating.set(false);
        Self { inner }
    }

    // Signals ---------------------------------------------------------------

    /// Called with the theme id when the user picks a theme (not persisted here).
    pub fn connect_theme_selected(&self, f: impl Fn(&str) + 'static) {
        self.inner.on_theme.borrow_mut().push(Rc::new(f));
    }

    /// Called with `(key, points)` when a font-size slider moves (not persisted here).
    pub fn connect_font_size_changed(&self, f: impl Fn(&str, i32) + 'static) {
        self.inner.on_size.borrow_mut().push(Rc::new(f));
    }

    /// Called after a validated change of any other key, or `"graph"` with the full
    /// validated object; the save has been queued already.
    pub fn connect_setting_changed(&self, f: impl Fn(&str, &Value) + 'static) {
        self.inner.on_setting.borrow_mut().push(Rc::new(f));
    }

    /// Report failed writes of the settings file; see [`writer::set_save_failed_handler`].
    pub fn connect_save_failed(&self, f: impl Fn(&Path) + 'static) {
        writer::set_save_failed_handler(f);
    }

    // Public API --------------------------------------------------------------

    /// Show the window, optionally on a page, focus the navigation row and raise it.
    pub fn present(&self, page: Option<&str>) {
        self.inner.window.show_all();
        let id = page.filter(|p| page_index(p).is_some());
        if let Some(id) = id {
            self.inner.nav_list.select_page(id);
        }
        let current = id
            .map(str::to_owned)
            .or_else(|| self.inner.nav_list.page_id());
        if let Some(row) = current.and_then(|id| self.inner.nav_list.rows().get(&id).cloned()) {
            if id.is_some() {
                self.inner.window.set_focus(Some(row.widget()));
            }
        }
        self.inner.window.present();
        self.inner.schedule_prebuild();
    }

    /// Select a page by id (see [`PAGES`]); false for an unknown id.
    pub fn select_page(&self, id: &str) -> bool {
        self.inner.nav_list.select_page(id)
    }

    /// The id of the page being shown.
    pub fn page_id(&self) -> Option<String> {
        self.inner.nav_list.page_id()
    }

    /// Compatibility with the former dialog API: any response hides.
    pub fn response(&self) {
        self.inner.window.hide();
    }

    pub fn theme_id(&self) -> String {
        self.inner.theme_id.borrow().clone()
    }

    /// The three font sizes as shown (a copy).
    pub fn sizes(&self) -> BTreeMap<String, i32> {
        self.inner.sizes.borrow().clone()
    }

    /// Every settings key as currently shown (a copy): `theme`, sizes, the other keys
    /// and the validated `graph` object.
    pub fn values(&self) -> Values {
        let mut out = Values::new();
        out.insert("theme".into(), Value::from(self.theme_id()));
        for (key, size) in self.inner.sizes.borrow().iter() {
            out.insert(key.clone(), Value::from(*size));
        }
        for (key, value) in self.inner.values.borrow().iter() {
            out.insert(key.clone(), value.clone());
        }
        out.insert(
            "graph".into(),
            Value::Object(self.inner.graph.borrow().clone()),
        );
        out
    }

    /// Select without emission; unknown ids fall back to the default theme.
    pub fn set_theme_id(&self, theme_id: Option<&str>) {
        self.inner.set_theme_id(theme_id);
    }

    /// Update supplied known keys silently; invalid values use defaults.
    pub fn set_sizes(&self, sizes: &BTreeMap<String, i32>) {
        let given = sizes
            .iter()
            .map(|(k, v)| (k.clone(), Value::from(*v)))
            .collect();
        self.inner.set_sizes(&given);
    }

    /// Show any known keys silently (validated); unknown keys are ignored.
    pub fn set_values(&self, values: &Values) {
        self.inner.set_values(values);
    }

    /// Replace the Hotkeys page list with `(command label, accelerator text)` pairs.
    /// An unchanged list is a no-op.
    pub fn set_hotkeys<L: AsRef<str>, A: AsRef<str>>(&self, hotkeys: &[(L, A)]) {
        let list = hotkeys
            .iter()
            .map(|(l, a)| (l.as_ref().to_owned(), a.as_ref().to_owned()))
            .collect();
        self.inner.set_hotkeys(list);
    }

    /// The project root shown by the notes folder row.
    pub fn set_project_root(&self, root: Option<&Path>) {
        *self.inner.project_root.borrow_mut() = root.map(Path::to_path_buf);
        self.inner.sync_notes_folder();
    }

    /// Filter rows across pages (name, description, aliases), grouped by page; the
    /// hit count while a query is active.
    pub fn search(&self, query: &str) -> Option<usize> {
        self.inner.search(query)
    }

    /// Focus the first focusable row of the current view; false when there is none.
    pub fn focus_content(&self) -> bool {
        self.inner.focus_content()
    }

    /// Focusable rows of the current view, in order.
    pub fn visible_rows(&self) -> Vec<SettingRow> {
        self.inner.visible_rows()
    }

    /// Build every page now (they are otherwise built on first visit).
    pub fn build_all_pages(&self) {
        self.inner.ensure_all_pages();
    }

    /// Whether the page has been built.
    pub fn page_built(&self, id: &str) -> bool {
        page_index(id).is_some_and(|i| self.inner.pages.borrow()[i].is_some())
    }

    // Parts ---------------------------------------------------------------

    pub fn nav_list(&self) -> NavList {
        self.inner.nav_list.clone()
    }
    pub fn nav_pane(&self) -> gtk::Box {
        self.inner.nav_pane.clone()
    }
    pub fn search_entry(&self) -> SearchField {
        self.inner.search_entry.clone()
    }
    pub fn stack(&self) -> gtk::Stack {
        self.inner.stack.clone()
    }
    pub fn group_title(&self) -> gtk::Label {
        self.inner.group_title.clone()
    }
    /// The row of a settings key (`graph.<name>` for graph rows, `theme`,
    /// `ignored_folders`, `notes_folder`, `settings_file`); builds its page.
    pub fn row(&self, key: &str) -> Option<SettingRow> {
        self.inner.ensure_for_key(key);
        self.inner.reg.borrow().rows.get(key).cloned()
    }
    /// The control of a settings key's row; builds its page.
    pub fn control(&self, key: &str) -> Option<gtk::Widget> {
        self.inner.ensure_for_key(key);
        self.inner.reg.borrow().controls.get(key).cloned()
    }
    /// The Restore-default button of a row, if it has one.
    pub fn reset_button(&self, key: &str) -> Option<impro_ui::widgets::IconButton> {
        self.row(key).and_then(|row| row.reset_button.clone())
    }
    /// The font-size slider of a font-size key.
    pub fn size_scale(&self, key: &str) -> Option<gtk::Scale> {
        self.inner.ensure_for_key(key);
        self.inner.reg.borrow().scales.get(key).cloned()
    }
    /// The numeric label next to a slider.
    pub fn value_label(&self, key: &str) -> Option<gtk::Label> {
        self.inner.ensure_for_key(key);
        self.inner.reg.borrow().value_labels.get(key).cloned()
    }
    /// The scrolled window of a page (built on demand).
    pub fn page_scroll(&self, id: &str) -> Option<gtk::ScrolledWindow> {
        self.inner.page(id).map(|p| p.scroll)
    }
    /// The body box of a page (built on demand).
    pub fn page_body(&self, id: &str) -> Option<gtk::Box> {
        self.inner.page(id).map(|p| p.body)
    }
    /// The centred column of a page (built on demand).
    pub fn page_column(&self, id: &str) -> Option<CenteredColumn> {
        self.inner.page(id).map(|p| p.column)
    }
    /// `(group box, card)` pairs of a page (built on demand).
    pub fn groups(&self, id: &str) -> Vec<(gtk::Box, SettingCard)> {
        self.inner
            .page(id)
            .map(|p| p.groups.into_iter().map(|g| (g.widget, g.card)).collect())
            .unwrap_or_default()
    }
    /// The search-results section of a page, once a search has run.
    pub fn section(&self, id: &str) -> Option<gtk::Box> {
        let results = self.inner.results.borrow().clone()?;
        results.sections.get(page_index(id)?).cloned()
    }
    pub fn results_scroll(&self) -> Option<gtk::ScrolledWindow> {
        self.inner
            .results
            .borrow()
            .as_ref()
            .map(|r| r.scroll.clone())
    }
    pub fn empty_label(&self) -> Option<gtk::Label> {
        self.inner
            .results
            .borrow()
            .as_ref()
            .map(|r| r.empty_label.clone())
    }
    pub fn theme_combo(&self) -> gtk::ComboBoxText {
        self.inner.ensure_page(0);
        self.inner
            .extras
            .borrow()
            .theme_combo
            .clone()
            .expect("appearance page")
    }
    pub fn theme_preview(&self) -> gtk::Box {
        self.inner.ensure_page(0);
        self.inner
            .extras
            .borrow()
            .theme_preview
            .clone()
            .expect("appearance page")
    }
    pub fn accent_picker(&self) -> impro_ui::widgets::SwatchPicker {
        self.inner.ensure_page(0);
        self.inner
            .extras
            .borrow()
            .accent_picker
            .clone()
            .expect("appearance page")
    }
    pub fn shell_hint(&self) -> gtk::Label {
        self.inner.ensure_for_key("terminal_shell");
        self.inner
            .extras
            .borrow()
            .shell_hint
            .clone()
            .expect("terminal page")
    }
    pub fn hotkey_filter(&self) -> SearchField {
        self.inner.ensure_hotkeys();
        self.inner
            .extras
            .borrow()
            .hotkey_filter
            .clone()
            .expect("hotkeys page")
    }
    pub fn hotkey_box(&self) -> gtk::Box {
        self.inner.ensure_hotkeys();
        self.inner
            .extras
            .borrow()
            .hotkey_box
            .clone()
            .expect("hotkeys page")
    }
    pub fn hotkeys_empty(&self) -> gtk::Label {
        self.inner.ensure_hotkeys();
        self.inner
            .extras
            .borrow()
            .hotkeys_empty
            .clone()
            .expect("hotkeys page")
    }
    /// The card holding the shortcut rows (replaced by `set_hotkeys`; fetch it again).
    pub fn hotkey_card(&self) -> SettingCard {
        self.inner.ensure_hotkeys();
        let page = self.inner.page("hotkeys").expect("hotkeys page");
        page.groups.last().expect("hotkey card").card.clone()
    }
    pub fn copy_button(&self) -> gtk::Button {
        self.inner.ensure_page(8);
        self.inner
            .extras
            .borrow()
            .copy_button
            .clone()
            .expect("about page")
    }
    pub fn settings_path_label(&self) -> gtk::Label {
        self.inner.ensure_page(8);
        self.inner
            .extras
            .borrow()
            .settings_path_label
            .clone()
            .expect("about page")
    }
}

pub(crate) fn label(text: &str, xalign: f32) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(xalign);
    label
}

pub(crate) fn normalize_size(key: &str, value: Value) -> i32 {
    settings::normalize(key, &value)
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(11) as i32
}

impl Inner {
    fn build_chrome(self: &Rc<Self>) {
        let window = &self.window;
        let weak = Rc::downgrade(self);
        window.connect_delete_event(move |w, _| {
            w.hide();
            glib::Propagation::Stop
        });
        window.connect_key_press_event({
            let weak = weak.clone();
            move |_, event| prop(upgrade(&weak, |i| i.on_key(event)).unwrap_or(false))
        });
        window.connect_configure_event({
            let weak = weak.clone();
            move |_, event| {
                let width = event.size().0 as i32;
                upgrade(&weak, |i| i.update_nav_width(width));
                false
            }
        });

        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        window.add(&root);
        // Navigation pane.
        let pane = &self.nav_pane;
        for side in ["start", "end", "top"] {
            match side {
                "start" => self.search_entry.set_margin_start(12),
                "end" => self.search_entry.set_margin_end(12),
                _ => self.search_entry.set_margin_top(12),
            }
        }
        self.search_entry.connect_changed({
            let weak = weak.clone();
            move |entry| {
                let text = entry.text().to_string();
                upgrade(&weak, |i| i.search(&text));
            }
        });
        self.search_entry.connect_key_press_event({
            let weak = weak.clone();
            move |_, event| prop(upgrade(&weak, |i| i.search_key(event, true)).unwrap_or(false))
        });
        pane.pack_start(self.search_entry.widget(), false, false, 0);
        self.group_title.set_margin_top(12 + 8);
        self.group_title.set_margin_bottom(8);
        self.group_title.set_margin_start(12 + 8);
        pane.pack_start(&self.group_title, false, false, 0);
        let nav = &self.nav_list;
        nav.set_margin_start(12);
        nav.set_margin_end(12);
        nav.set_margin_bottom(12);
        nav.set_filter_func(Some(Box::new({
            let weak = weak.clone();
            move |row| upgrade(&weak, |i| i.nav_filter(row)).unwrap_or(true)
        })));
        nav.connect_row_selected({
            let weak = weak.clone();
            move |_, row| {
                let index = row.map(|r| r.index());
                upgrade(&weak, |i| i.nav_selected(index));
            }
        });
        nav.connect_key_press_event({
            let weak = weak.clone();
            move |_, event| prop(upgrade(&weak, |i| i.nav_key(event)).unwrap_or(false))
        });
        if let Some(accessible) = nav.accessible() {
            accessible.set_name("Settings pages");
        }
        let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.add(nav.widget());
        pane.pack_start(&scroll, true, true, 0);
        root.pack_start(pane, false, false, 0);
        self.stack
            .set_transition_type(gtk::StackTransitionType::None);
        self.stack.set_hexpand(true);
        root.pack_start(&self.stack, true, true, 0);
        for (id, title, icon) in PAGES {
            self.nav_list.add_page(id, title, icon);
        }
        self.update_nav_width(900);
        root.show_all();
        self.nav_list.select_page("appearance");
        if let Some(row) = self.nav_list.rows().get("appearance") {
            self.window.set_focus(Some(row.widget()));
        }
    }

    // Pages ------------------------------------------------------------------

    pub(crate) fn page(self: &Rc<Self>, id: &str) -> Option<Page> {
        let index = page_index(id)?;
        self.ensure_page(index);
        self.pages.borrow()[index].clone()
    }

    pub(crate) fn ensure_for_key(self: &Rc<Self>, key: &str) {
        if let Some(index) = page_of(key).and_then(page_index) {
            self.ensure_page(index);
        }
    }

    pub(crate) fn ensure_hotkeys(self: &Rc<Self>) {
        self.ensure_page(7);
    }

    pub(crate) fn ensure_all_pages(self: &Rc<Self>) {
        for index in 0..PAGES.len() {
            self.ensure_page(index);
        }
    }

    fn schedule_prebuild(self: &Rc<Self>) {
        if self.prebuild.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            let Some(inner) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let next = inner.pages.borrow().iter().position(Option::is_none);
            match next {
                Some(index) => {
                    inner.ensure_page(index);
                    glib::ControlFlow::Continue
                }
                None => {
                    inner.prebuild.set(false);
                    glib::ControlFlow::Break
                }
            }
        });
    }

    pub(crate) fn all_rows(&self) -> Vec<SettingRow> {
        let pages = self.pages.borrow().clone();
        pages
            .into_iter()
            .flatten()
            .flat_map(|p| p.groups.into_iter().flat_map(|g| g.card.rows()))
            .collect()
    }

    fn update_nav_width(&self, width: i32) {
        let (fraction, low, high) = NAV_WIDTH;
        let target = ((width as f64 * fraction).round_ties_even() as i32).clamp(low, high);
        if self.nav_pane.size_request().0 != target {
            self.nav_pane.set_size_request(target, -1);
        }
        let column = 700.min(width - target - 1 - 2 * 32);
        let narrow = column < NARROW_CONTENT;
        if narrow != self.narrow.get() {
            self.narrow.set(narrow);
            for row in self.all_rows() {
                row.set_narrow(narrow);
            }
        }
    }

    // Model ------------------------------------------------------------------

    pub(crate) fn model_value(&self, key: &str) -> Option<Value> {
        if let Some(name) = key.strip_prefix("graph.") {
            return self.graph.borrow().get(name).cloned();
        }
        if let Some(size) = self.sizes.borrow().get(key) {
            return Some(Value::from(*size));
        }
        self.values.borrow().get(key).cloned()
    }

    /// Push the model into the widgets of a freshly built page.
    pub(crate) fn sync_page(self: &Rc<Self>, index: usize) {
        let keys = self.reg.borrow().keys[index].clone();
        let previous = self.updating.replace(true);
        for key in keys {
            let setter = self.reg.borrow().setters.get(&key).cloned();
            if let (Some(setter), Some(value)) = (setter, self.model_value(&key)) {
                setter(&value);
            }
            self.update_reset(&key);
        }
        if index == 0 {
            let combo = self.extras.borrow().theme_combo.clone();
            if let Some(combo) = combo {
                let id = self.theme_id.borrow().clone();
                combo.set_active_id(Some(&id));
            }
        }
        self.updating.set(previous);
        self.sync_features();
    }

    pub(crate) fn sync_features(&self) {
        let features = self.reg.borrow().features.clone();
        for (index, feature) in features {
            let enabled = self
                .values
                .borrow()
                .get(&feature)
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let page = self.pages.borrow()[index].clone();
            for group in page.into_iter().flat_map(|p| p.groups) {
                for row in group.card.rows() {
                    if row.key.as_deref() != Some(feature.as_str()) {
                        row.set_sensitive(enabled);
                        row.set_opacity(if enabled { 1.0 } else { 0.48 });
                    }
                }
            }
        }
    }

    pub(crate) fn update_reset(&self, key: &str) {
        let button = self
            .reg
            .borrow()
            .rows
            .get(key)
            .and_then(|row| row.reset_button.clone());
        let Some(button) = button else { return };
        let (current, default) = if let Some(name) = key.strip_prefix("graph.") {
            (
                self.graph.borrow().get(name).cloned(),
                crate::graph_prefs::graph_defaults().get(name).cloned(),
            )
        } else {
            (
                self.model_value(key),
                settings::defaults().get(key).cloned(),
            )
        };
        let same = match (&current, &default) {
            (Some(a), Some(b)) => graph_values_equal(a, b),
            (a, b) => a == b,
        };
        button.set_sensitive(!same);
    }

    pub(crate) fn emit_setting(&self, key: &str, value: &Value) {
        let callbacks = self.on_setting.borrow().clone();
        for callback in callbacks {
            callback(key, value);
        }
    }

    pub(crate) fn changed(self: &Rc<Self>, key: &str, value: Value) {
        if self.updating.get() {
            return;
        }
        let Ok(value) = settings::normalize(key, &value) else {
            return;
        };
        if settings::FONT_KEYS.contains(&key) {
            let scale = self.reg.borrow().scales.get(key).cloned();
            if let (Some(scale), Some(size)) = (scale, value.as_f64()) {
                scale.set_value(size);
            }
            return;
        }
        if self.values.borrow().get(key) == Some(&value) {
            return;
        }
        self.values
            .borrow_mut()
            .insert(key.to_owned(), value.clone());
        let setter = self.reg.borrow().setters.get(key).cloned();
        if let Some(setter) = setter {
            let previous = self.updating.replace(true);
            setter(&value);
            self.updating.set(previous);
        }
        self.update_reset(key);
        self.sync_features();
        writer::save_value_async(key, value.clone());
        self.emit_setting(key, &value);
    }

    pub(crate) fn graph_changed(self: &Rc<Self>, key: &str, value: Value) {
        if self.updating.get() {
            return;
        }
        // The persisted preferences (including a change still queued) are the base,
        // so edits retain what an open graph pane has saved meanwhile.
        let path = settings::settings_path();
        let base = writer::unsaved_value(&path, "graph").unwrap_or_else(settings::get_graph);
        let mut merged = normalize_graph(&base);
        {
            let (graph, saved) = (self.graph.borrow(), self.saved_graph.borrow());
            for (name, current) in graph.iter() {
                if !saved
                    .get(name)
                    .is_some_and(|old| graph_values_equal(old, current))
                {
                    merged.insert(name.clone(), current.clone());
                }
            }
        }
        merged.insert(key.to_owned(), value);
        let graph = normalize_graph(&Value::Object(merged));
        let unchanged = {
            let current = self.graph.borrow();
            graph.len() == current.len()
                && graph
                    .iter()
                    .all(|(k, v)| current.get(k).is_some_and(|c| graph_values_equal(c, v)))
        };
        if unchanged {
            return;
        }
        let mut shown = Values::new();
        shown.insert("graph".into(), Value::Object(graph.clone()));
        self.set_values(&shown);
        writer::save_value_async("graph", Value::Object(graph.clone()));
        *self.saved_graph.borrow_mut() = graph.clone();
        self.emit_setting("graph", &Value::Object(graph));
    }

    pub(crate) fn set_theme_id(self: &Rc<Self>, theme_id: Option<&str>) {
        let theme = themes::get_theme(theme_id);
        *self.theme_id.borrow_mut() = theme.id.clone();
        let combo = self.extras.borrow().theme_combo.clone();
        if let Some(combo) = combo {
            let previous = self.updating.replace(true);
            combo.set_active_id(Some(&theme.id));
            self.updating.set(previous);
        }
    }

    pub(crate) fn set_sizes(self: &Rc<Self>, given: &BTreeMap<String, Value>) {
        for (key, value) in given {
            if settings::FONT_KEYS.contains(&key.as_str()) {
                let size = normalize_size(key, value.clone());
                self.sizes.borrow_mut().insert(key.clone(), size);
            }
        }
        let previous = self.updating.replace(true);
        let scales = self.reg.borrow().scales.clone();
        for (key, scale) in scales {
            let size = self.sizes.borrow().get(&key).copied();
            if let Some(size) = size {
                scale.set_value(size as f64);
            }
        }
        self.updating.set(previous);
        for key in settings::FONT_KEYS {
            self.update_reset(key);
        }
    }

    pub(crate) fn set_values(self: &Rc<Self>, values: &Values) {
        if let Some(theme) = values.get("theme") {
            self.set_theme_id(theme.as_str());
        }
        let sizes: BTreeMap<String, Value> = settings::FONT_KEYS
            .iter()
            .filter_map(|k| values.get(*k).map(|v| (k.to_string(), v.clone())))
            .collect();
        if !sizes.is_empty() {
            self.set_sizes(&sizes);
        }
        let previous = self.updating.replace(true);
        for key in settings::new_keys() {
            let Some(raw) = values.get(key) else { continue };
            let Ok(value) = settings::normalize(key, raw) else {
                continue;
            };
            self.values
                .borrow_mut()
                .insert(key.to_owned(), value.clone());
            let setter = self.reg.borrow().setters.get(key).cloned();
            if let Some(setter) = setter {
                setter(&value);
            }
            self.update_reset(key);
        }
        if let Some(raw) = values.get("graph") {
            let graph = normalize_graph(raw);
            *self.graph.borrow_mut() = graph.clone();
            for name in crate::graph_prefs::graph_defaults().keys() {
                let row_key = format!("graph.{name}");
                let setter = self.reg.borrow().setters.get(&row_key).cloned();
                if let Some(setter) = setter {
                    setter(&graph[name]);
                    self.update_reset(&row_key);
                }
            }
        }
        self.sync_features();
        self.updating.set(previous);
    }

    pub(crate) fn set_hotkeys(self: &Rc<Self>, list: Vec<(String, String)>) {
        if *self.hotkeys.borrow() == list {
            return;
        }
        *self.hotkeys.borrow_mut() = list;
        if self.pages.borrow()[7].is_some() {
            self.fill_hotkeys();
            self.filter_hotkeys();
            if self.searching.get() {
                let text = self.search_entry.text().to_string();
                self.search(&text);
            }
        }
    }

    // Handlers ----------------------------------------------------------------

    pub(crate) fn emit_theme(&self, id: &str) {
        let callbacks = self.on_theme.borrow().clone();
        for callback in callbacks {
            callback(id);
        }
    }

    pub(crate) fn emit_size(&self, key: &str, size: i32) {
        let callbacks = self.on_size.borrow().clone();
        for callback in callbacks {
            callback(key, size);
        }
    }

    pub(crate) fn theme_changed(&self, combo: &gtk::ComboBoxText) {
        let Some(id) = combo.active_id() else { return };
        let theme = themes::get_theme(Some(id.as_str()));
        let mut colors = vec![theme.ui["bg"].clone(), theme.ui["editor_fg"].clone()];
        for kind in ["keyword", "string", "function", "comment"] {
            colors.push(theme.syntax[kind].foreground.clone());
        }
        let (providers, preview) = {
            let extras = self.extras.borrow();
            (
                extras.preview_providers.clone(),
                extras.theme_preview.clone(),
            )
        };
        for (provider, color) in providers.iter().zip(colors) {
            let _ = provider.load_from_data(format!("* {{ background: {color}; }}").as_bytes());
        }
        if let Some(preview) = preview {
            preview.set_tooltip_text(Some(&format!(
                "{} ({})",
                theme.name,
                if theme.dark { "dark" } else { "light" }
            )));
        }
        let changed = *self.theme_id.borrow() != theme.id;
        *self.theme_id.borrow_mut() = theme.id.clone();
        if changed && !self.updating.get() {
            self.emit_theme(&theme.id);
        }
    }

    pub(crate) fn size_changed(&self, key: &str, scale: &gtk::Scale) {
        let size = scale.value().round_ties_even() as i32;
        self.sizes.borrow_mut().insert(key.to_owned(), size);
        self.update_reset(key);
        if !self.updating.get() {
            self.emit_size(key, size);
        }
    }

    pub(crate) fn nav_filter(&self, row: &gtk::ListBoxRow) -> bool {
        match *self.nav_counts.borrow() {
            None => true,
            Some(counts) => counts
                .get(row.index().max(0) as usize)
                .is_some_and(|&n| n > 0),
        }
    }

    fn nav_selected(self: &Rc<Self>, index: Option<i32>) {
        let Some(index) = index.filter(|i| *i >= 0).map(|i| i as usize) else {
            return;
        };
        let Some((id, _, _)) = PAGES.get(index) else {
            return;
        };
        if self.searching.get() {
            let results = self.results.borrow().clone();
            if let Some(results) = results {
                if let Some((_, y)) =
                    results.sections[index].translate_coordinates(&results.content, 0, 0)
                {
                    results.scroll.vadjustment().set_value(y as f64);
                }
            }
        } else {
            self.ensure_page(index);
            self.stack.set_visible_child_name(id);
            if let Some(page) = self.pages.borrow()[index].clone() {
                page.scroll.vadjustment().set_value(0.0);
            }
        }
    }

    // Search --------------------------------------------------------------------

    fn ensure_results(self: &Rc<Self>) {
        if self.results.borrow().is_some() {
            return;
        }
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let (scroll, _column) = scrolled(&content);
        self.stack.add_named(&scroll, "results");
        let mut sections = Vec::new();
        for (_, title, _) in PAGES {
            let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let heading = class(&label(title, 0.0), "page-title");
            heading.set_margin_bottom(16);
            heading.set_margin_start(16);
            section.set_margin_bottom(32);
            section.pack_start(&heading, false, false, 0);
            content.pack_start(&section, false, false, 0);
            sections.push(section);
        }
        let empty_label = class(&label("", 0.5), "empty");
        empty_label.set_line_wrap(true);
        empty_label.set_margin_top(48);
        content.pack_start(&empty_label, false, false, 0);
        scroll.show_all();
        for section in &sections {
            section.set_no_show_all(true);
        }
        empty_label.set_no_show_all(true);
        empty_label.hide();
        *self.results.borrow_mut() = Some(Results {
            scroll,
            content,
            sections,
            empty_label,
        });
    }

    fn move_bodies(&self, into_sections: bool) {
        let results = self.results.borrow().clone();
        let pages = self.pages.borrow().clone();
        let Some(results) = results else { return };
        for (index, page) in pages.into_iter().enumerate() {
            let Some(page) = page else { continue };
            if into_sections {
                page.column.remove(&page.body);
                results.sections[index].pack_start(&page.body, false, false, 0);
            } else {
                results.sections[index].remove(&page.body);
                page.column.add(&page.body);
            }
        }
    }

    pub(crate) fn search(self: &Rc<Self>, query: &str) -> Option<usize> {
        let query = query.trim();
        if query.is_empty() && !self.searching.get() && self.results.borrow().is_none() {
            return None;
        }
        *self.search_query.borrow_mut() = query.to_owned();
        if !query.is_empty() && !self.searching.get() {
            self.ensure_all_pages();
            self.ensure_results();
            self.searching.set(true);
            let (hotkey_box, empty) = {
                let extras = self.extras.borrow();
                (extras.hotkey_box.clone(), extras.hotkeys_empty.clone())
            };
            if let Some(widget) = hotkey_box {
                widget.hide();
            }
            if let Some(widget) = empty {
                widget.hide();
            }
            self.move_bodies(true);
            let scroll = self.results.borrow().as_ref().map(|r| r.scroll.clone());
            if let Some(scroll) = scroll {
                self.stack.set_visible_child(&scroll);
            }
        } else if query.is_empty() && self.searching.get() {
            self.searching.set(false);
            self.move_bodies(false);
            if let Some(widget) = self.extras.borrow().hotkey_box.clone() {
                widget.show();
            }
            let id = self
                .nav_list
                .page_id()
                .unwrap_or_else(|| "appearance".into());
            self.stack.set_visible_child_name(&id);
        }
        let results = self.results.borrow().clone();
        let pages = self.pages.borrow().clone();
        let mut counts = [0usize; 9];
        for (index, page) in pages.iter().enumerate() {
            let Some(page) = page else { continue };
            let mut total = 0;
            for group in &page.groups {
                group.card.set_query(query);
                let visible = card_visible_rows(&group.card, query).len();
                group.widget.set_visible(query.is_empty() || visible > 0);
                total += visible;
            }
            counts[index] = total;
            if let Some(results) = &results {
                results.sections[index].set_visible(total > 0);
            }
        }
        if query.is_empty() {
            self.filter_hotkeys();
        }
        let active = !query.is_empty();
        *self.nav_counts.borrow_mut() = active.then_some(counts);
        for (index, (id, _, _)) in PAGES.iter().enumerate() {
            if let Some(row) = self.nav_list.rows().get(*id) {
                row.set_count(active.then_some(counts[index]));
            }
        }
        self.nav_list.invalidate_filter();
        let hits: usize = counts.iter().sum();
        if let Some(results) = &results {
            results
                .empty_label
                .set_text(&format!("No settings match “{query}”."));
            results.empty_label.set_visible(active && hits == 0);
        }
        active.then_some(hits)
    }

    pub(crate) fn filter_hotkeys(&self) {
        if self.searching.get() {
            return;
        }
        let (filter, empty) = {
            let extras = self.extras.borrow();
            (extras.hotkey_filter.clone(), extras.hotkeys_empty.clone())
        };
        let (Some(filter), Some(empty)) = (filter, empty) else {
            return;
        };
        let page = self.pages.borrow()[7].clone();
        let Some(card) = page.and_then(|p| p.groups.last().map(|g| g.card.clone())) else {
            return;
        };
        let text = filter.text().to_string();
        card.set_query(&text);
        empty.set_visible(!text.trim().is_empty() && card_visible_rows(&card, &text).is_empty());
    }

    // Keyboard --------------------------------------------------------------------

    pub(crate) fn visible_rows(&self) -> Vec<SettingRow> {
        let ids: Vec<usize> = if self.searching.get() {
            let results = self.results.borrow().clone();
            (0..PAGES.len())
                .filter(|i| {
                    results
                        .as_ref()
                        .is_some_and(|r| r.sections[*i].get_visible())
                })
                .collect()
        } else {
            let id = self
                .nav_list
                .page_id()
                .unwrap_or_else(|| "appearance".into());
            page_index(&id).into_iter().collect()
        };
        let pages = self.pages.borrow().clone();
        let mut rows = Vec::new();
        for index in ids {
            let Some(Some(page)) = pages.get(index) else {
                continue;
            };
            for group in &page.groups {
                if group.widget.get_visible() {
                    rows.extend(
                        card_visible_rows(&group.card, &self.query_for(index))
                            .into_iter()
                            .filter(|r| r.is_sensitive()),
                    );
                }
            }
        }
        rows
    }

    /// The text the cards of page `index` are currently filtered by.
    fn query_for(&self, index: usize) -> String {
        if self.searching.get() {
            self.search_query.borrow().clone()
        } else if index == 7 {
            let filter = self.extras.borrow().hotkey_filter.clone();
            filter
                .map(|f| f.text().trim().to_owned())
                .unwrap_or_default()
        } else {
            String::new()
        }
    }

    pub(crate) fn focus_content(&self) -> bool {
        let rows = self.visible_rows();
        if let Some(first) = rows.first() {
            first.grab_focus();
        }
        !rows.is_empty()
    }

    fn nav_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as key;
        let (keyval, state) = (
            event.keyval(),
            event.state() & gtk::accelerator_get_default_mod_mask(),
        );
        if keyval == key::ISO_Left_Tab
            || (keyval == key::Tab && state.contains(gdk::ModifierType::SHIFT_MASK))
        {
            self.search_entry.grab_focus();
            return true;
        }
        if !(state - gdk::ModifierType::SHIFT_MASK).is_empty() {
            return false;
        }
        if [key::Return, key::KP_Enter, key::Right, key::l, key::Tab].contains(&keyval) {
            return self.focus_content() || keyval != key::Tab;
        }
        false
    }

    pub(crate) fn row_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as key;
        let Some(focus) = self.window.focused_widget() else {
            return false;
        };
        let rows = self.all_rows_and_hotkeys();
        let Some(row) = rows
            .iter()
            .find(|r| r.widget().upcast_ref::<gtk::Widget>() == &focus)
            .cloned()
        else {
            return false;
        };
        if !(event.state() & gtk::accelerator_get_default_mod_mask()).is_empty() {
            return false;
        }
        let keyval = event.keyval();
        if [key::Return, key::KP_Enter, key::space].contains(&keyval) {
            row.activate_control();
            return true;
        }
        if [key::Left, key::h].contains(&keyval) {
            if let Some(selected) = self.nav_list.selected_row() {
                selected.grab_focus();
            }
            return true;
        }
        if [key::Down, key::j, key::Up, key::k].contains(&keyval) {
            let rows = self.visible_rows();
            let position = rows
                .iter()
                .position(|r| r.widget().upcast_ref::<gtk::Widget>() == &focus);
            if let Some(position) = position {
                let step: i64 = if [key::Down, key::j].contains(&keyval) {
                    1
                } else {
                    -1
                };
                let target = (position as i64 + step).clamp(0, rows.len() as i64 - 1);
                rows[target as usize].grab_focus();
            }
            return true;
        }
        false
    }

    fn all_rows_and_hotkeys(&self) -> Vec<SettingRow> {
        self.all_rows()
    }

    pub(crate) fn search_key(self: &Rc<Self>, event: &gdk::EventKey, main: bool) -> bool {
        use gdk::keys::constants as key;
        let keyval = event.keyval();
        if [key::Down, key::Return, key::KP_Enter].contains(&keyval) {
            if main && !self.searching.get() {
                if let Some(selected) = self.nav_list.selected_row() {
                    if keyval == key::Down {
                        selected.grab_focus();
                        return true;
                    }
                }
                return false;
            }
            return self.focus_content();
        }
        false
    }

    fn on_key(self: &Rc<Self>, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as key;
        let keyval = event.keyval();
        let state = event.state() & gtk::accelerator_get_default_mod_mask();
        if keyval == key::Escape {
            if !self.search_entry.text().is_empty() {
                self.search_entry.set_text("");
                self.search_entry.grab_focus();
                return true;
            }
            let filter = self.extras.borrow().hotkey_filter.clone();
            match filter {
                Some(filter) if !filter.text().is_empty() && !self.searching.get() => {
                    filter.set_text("");
                    filter.grab_focus();
                }
                _ => self.window.hide(),
            }
            return true;
        }
        if state == gdk::ModifierType::CONTROL_MASK && (keyval == key::f || keyval == key::F) {
            self.search_entry.grab_focus();
            return true;
        }
        if keyval == key::slash
            && state.is_empty()
            && !self
                .window
                .focused_widget()
                .is_some_and(|w| w.dynamic_cast_ref::<gtk::Editable>().is_some())
        {
            self.search_entry.grab_focus();
            return true;
        }
        false
    }
}

/// Rows of `card` that are not hidden and match `query`. (`SettingCard::visible_rows`
/// tests ancestor visibility too, which a hidden group would defeat.)
pub(crate) fn card_visible_rows(card: &SettingCard, query: &str) -> Vec<SettingRow> {
    card.rows()
        .into_iter()
        .filter(|r| r.get_visible() && r.matches(query))
        .collect()
}

pub(crate) fn upgrade<R>(weak: &Weak<Inner>, f: impl FnOnce(&Rc<Inner>) -> R) -> Option<R> {
    weak.upgrade().map(|inner| f(&inner))
}
