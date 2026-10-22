use crate::{
    file::{self, Document},
    tab::{EditorTab, TabHeader},
};
use gtk::{prelude::*, subclass::prelude::*};
use impro_core::themes::{SyntaxColors, Theme, UiColors};
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::SystemTime,
};

pub const DEFAULT_FONT_SIZE: i32 = 11;
pub const MIN_FONT_SIZE: i32 = 7;
pub const MAX_FONT_SIZE: i32 = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    Save,
    Discard,
    Cancel,
    Overwrite,
}
#[derive(Clone, Debug)]
pub enum Prompt {
    Close(PathBuf),
    Overwrite(PathBuf),
    Quit,
}
pub type DialogHook = Rc<dyn Fn(&EditorPanel, &Prompt) -> Response>;
type Done = Box<dyn FnOnce(bool)>;
type OpenDone = Box<dyn FnOnce(Option<Rc<EditorTab>>)>;
struct PendingOpen {
    cancel: Arc<AtomicBool>,
    source: glib::SourceId,
    callbacks: Vec<OpenDone>,
}
/// Arbitrary read-only page; shutdown runs once before removal or parent destruction.
pub struct ViewTab {
    pub key: String,
    pub page: gtk::Widget,
    pub header: TabHeader,
    shutdown: RefCell<Option<Box<dyn FnOnce()>>>,
}
impl ViewTab {
    fn shutdown(&self) {
        let f = self.shutdown.take();
        if let Some(f) = f {
            f();
        }
    }
}
#[derive(Clone)]
struct Config {
    root: Option<PathBuf>,
    font_family: Option<String>,
    font: i32,
    default_font: i32,
    line_numbers: bool,
    highlight_line: bool,
    indent_tabs: bool,
    indent_width: i32,
    auto_pair: bool,
    md_live_preview: bool,
    md_readable_width: bool,
    slash_commands: bool,
    syntax: SyntaxColors,
    chrome: UiColors,
}
impl Default for Config {
    fn default() -> Self {
        let t = impro_core::themes::get_theme(Some("dark-plus"));
        Self {
            root: None,
            font_family: None,
            font: 11,
            default_font: 11,
            line_numbers: true,
            highlight_line: true,
            indent_tabs: false,
            indent_width: 4,
            auto_pair: false,
            md_live_preview: true,
            md_readable_width: true,
            slash_commands: true,
            syntax: t.syntax.clone(),
            chrome: t.chrome.clone(),
        }
    }
}
pub(crate) struct Widgets {
    pub notebook: gtk::Notebook,
    pub stack: gtk::Stack,
    pub overlay: gtk::Overlay,
    pub search_bar: gtk::SearchBar,
    pub search_entry: gtk::SearchEntry,
    pub status: StatusBar,
}
mod imp {
    use super::*;
    #[derive(Default)]
    pub struct EditorPanel {
        pub(super) widgets: OnceCell<Widgets>,
        pub(super) config: RefCell<Config>,
        pub tabs: RefCell<Vec<Rc<EditorTab>>>,
        pub views: RefCell<BTreeMap<String, Rc<ViewTab>>>,
        pub(super) pending: RefCell<BTreeMap<PathBuf, PendingOpen>>,
        pub(super) jobs: RefCell<BTreeMap<u64, (Arc<AtomicBool>, glib::SourceId)>>,
        pub next: Cell<u64>,
        pub announced: RefCell<Option<PathBuf>>,
        pub dirty: Cell<bool>,
        pub closed: Cell<bool>,
        pub assembling: Cell<bool>,
        pub root_revision: Cell<u64>,
        pub dialog: RefCell<Option<DialogHook>>,
        pub last_error: RefCell<Option<String>>,
    }
    #[glib::object_subclass]
    impl ObjectSubclass for EditorPanel {
        const NAME: &'static str = "ImproEditorPanel";
        type Type = super::EditorPanel;
        type ParentType = gtk::Box;
    }
    impl ObjectImpl for EditorPanel {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build();
        }
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
                std::sync::OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("current-file-changed")
                        .param_types([String::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("view-tab-closed")
                        .param_types([String::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("tab-closed")
                        .param_types([String::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("file-saved")
                        .param_types([String::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("dirty-changed")
                        .param_types([bool::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("font-size-changed")
                        .param_types([i32::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("error")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }
    }
    impl WidgetImpl for EditorPanel {
        fn destroy(&self) {
            self.obj().shutdown();
            self.parent_destroy();
        }
    }
    impl ContainerImpl for EditorPanel {}
    impl BoxImpl for EditorPanel {}
}
glib::wrapper! {pub struct EditorPanel(ObjectSubclass<imp::EditorPanel>) @extends gtk::Widget,gtk::Container,gtk::Box,@implements gtk::Buildable,gtk::Orientable;}
impl Default for EditorPanel {
    fn default() -> Self {
        Self::new(None)
    }
}
macro_rules! config_get {($($method:ident:$field:ident->$ty:ty),*$(,)?)=>{$(pub fn $method(&self)->$ty{self.imp().config.borrow().$field.clone()})*};}
impl EditorPanel {
    pub fn new(root: Option<&Path>) -> Self {
        let this: Self = glib::Object::new();
        if root.is_some() {
            this.set_root(root);
        }
        this
    }
    fn widgets(&self) -> &Widgets {
        self.imp()
            .widgets
            .get()
            .expect("constructed editor widgets")
    }
    fn build(&self) {
        self.set_orientation(gtk::Orientation::Vertical);
        self.style_context().add_class("impro-editor");
        let stack = gtk::Stack::new();
        let placeholder = gtk::Label::new(Some("impro  -  open a file from the explorer"));
        placeholder.style_context().add_class("impro-empty");
        stack.add_named(&placeholder, "empty");
        let notebook = gtk::Notebook::new();
        notebook.set_scrollable(true);
        stack.add_named(&notebook, "editor");
        let overlay = gtk::Overlay::new();
        overlay.add(&stack);
        self.pack_start(&overlay, true, true, 0);
        let search_bar = gtk::SearchBar::new();
        search_bar.style_context().add_class("impro-find-bar");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let search_entry = gtk::SearchEntry::new();
        row.pack_start(&search_entry, true, true, 0);
        for (label, back) in [("Previous", true), ("Next", false)] {
            let button = gtk::Button::with_label(label);
            let weak = self.downgrade();
            button.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade() {
                    p.find(back, false);
                }
            });
            row.pack_start(&button, false, false, 0);
        }
        search_bar.add(&row);
        search_bar.connect_entry(&search_entry);
        self.pack_start(&search_bar, false, false, 0);
        let status = StatusBar::new();
        overlay.add_overlay(&status.widget);
        let weak = stack.downgrade();
        status.widget.connect_size_allocate(move |_, rect| {
            if let Some(s) = weak.upgrade() {
                s.set_margin_bottom(rect.height());
            }
        });
        stack.set_margin_bottom(status.widget.preferred_height().1);
        let weak = self.downgrade();
        status.targets[3].set_tooltip_text(Some("Markdown: Toggle source mode (Ctrl+Shift+M)"));
        status.targets[3].connect_button_press_event(move |_, e| {
            if e.button() == 1 {
                if let Some(p) = weak.upgrade() {
                    if let Some(t) = p.current_tab() {
                        if let Some(mode) = t.markdown_mode() {
                            t.set_markdown_mode(if mode == "preview" {
                                "source"
                            } else {
                                "preview"
                            });
                            return glib::Propagation::Stop;
                        }
                    }
                }
            }
            glib::Propagation::Proceed
        });
        self.imp()
            .widgets
            .set(Widgets {
                notebook: notebook.clone(),
                stack: stack.clone(),
                overlay,
                search_bar,
                search_entry: search_entry.clone(),
                status,
            })
            .ok();
        // Use an after handler so external observers see the selected page immediately.
        let weak = self.downgrade();
        notebook.connect_local("switch-page", true, move |_| {
            if let Some(p) = weak.upgrade() {
                p.switched();
            }
            None
        });
        let weak = self.downgrade();
        notebook.connect_page_reordered(move |_, _, _| {
            if let Some(p) = weak.upgrade() {
                p.update_tabs();
            }
        });
        let weak = self.downgrade();
        notebook.connect_size_allocate(move |_, _| {
            if let Some(p) = weak.upgrade() {
                p.size_tabs();
            }
        });
        let weak = self.downgrade();
        search_entry.connect_search_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.find(false, true);
            }
        });
        let weak = self.downgrade();
        search_entry.connect_activate(move |_| {
            if let Some(p) = weak.upgrade() {
                p.find(false, false);
            }
        });
        let weak = self.downgrade();
        self.connect_key_press_event(move |_, e| {
            if let Some(p) = weak.upgrade() {
                if p.current_view_key().is_some() && p.tab_shortcut(e) {
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        self.show_all();
        stack.set_visible_child_name("empty");
        self.update_status();
    }
    config_get! {root:root->Option<PathBuf>,font_size:font->i32,default_font_size:default_font->i32,line_numbers:line_numbers->bool,highlight_line:highlight_line->bool,indent_tabs:indent_tabs->bool,indent_width:indent_width->i32,auto_pair:auto_pair->bool,md_live_preview:md_live_preview->bool,md_readable_width:md_readable_width->bool,slash_commands:slash_commands->bool,syntax_style:syntax->SyntaxColors,chrome:chrome->UiColors}
    pub fn notebook(&self) -> gtk::Notebook {
        self.widgets().notebook.clone()
    }
    pub fn stack(&self) -> gtk::Stack {
        self.widgets().stack.clone()
    }
    pub fn overlay(&self) -> gtk::Overlay {
        self.widgets().overlay.clone()
    }
    pub fn search_bar(&self) -> gtk::SearchBar {
        self.widgets().search_bar.clone()
    }
    pub fn search_entry(&self) -> gtk::SearchEntry {
        self.widgets().search_entry.clone()
    }
    pub fn status(&self) -> StatusBar {
        self.widgets().status.clone()
    }
    pub fn tabs(&self) -> Vec<Rc<EditorTab>> {
        self.imp().tabs.borrow().clone()
    }
    pub fn view_tabs(&self) -> Vec<Rc<ViewTab>> {
        self.imp().views.borrow().values().cloned().collect()
    }
    pub fn current_tab(&self) -> Option<Rc<EditorTab>> {
        let n = self.notebook();
        let page = n.nth_page(n.current_page())?;
        self.tabs()
            .into_iter()
            .find(|t| t.page.upcast_ref::<gtk::Widget>() == &page)
    }
    pub fn current_path(&self) -> Option<PathBuf> {
        self.current_tab().map(|t| t.path())
    }
    pub fn current_view_key(&self) -> Option<String> {
        let n = self.notebook();
        let page = n.nth_page(n.current_page())?;
        self.view_tabs()
            .into_iter()
            .find(|t| t.page == page)
            .map(|t| t.key.clone())
    }
    pub fn tab_for_path(&self, path: impl AsRef<Path>) -> Option<Rc<EditorTab>> {
        let path = file::absolute(path.as_ref());
        self.tabs().into_iter().find(|t| t.path() == path)
    }
    pub fn buffer_text(&self, path: impl AsRef<Path>) -> Option<String> {
        self.tab_for_path(path)
            .filter(|t| !t.deleted())
            .map(|t| t.text())
    }
    pub fn selected_text(&self) -> String {
        self.current_tab()
            .and_then(|t| {
                t.buffer
                    .selection_bounds()
                    .map(|(a, b)| t.buffer.text(&a, &b, true).unwrap_or_default().to_string())
            })
            .unwrap_or_default()
    }
    pub fn has_unsaved(&self) -> bool {
        self.tabs()
            .iter()
            .any(|t| t.buffer.is_modified() || t.deleted())
    }
    pub fn is_busy(&self) -> bool {
        !self.imp().pending.borrow().is_empty() || !self.imp().jobs.borrow().is_empty()
    }
    pub fn last_error(&self) -> Option<String> {
        self.imp().last_error.borrow().clone()
    }
    pub fn set_dialog_hook(&self, hook: Option<DialogHook>) {
        self.imp().dialog.replace(hook);
    }
    fn ask(&self, prompt: Prompt) -> Response {
        let hook = self.imp().dialog.borrow().clone();
        if let Some(h) = hook {
            return h(self, &prompt);
        }
        let (text, choices) = match &prompt {
            Prompt::Close(p) => (
                format!(
                    "Save changes to {}?",
                    p.file_name().unwrap_or_default().to_string_lossy()
                ),
                vec![
                    ("Save", Response::Save),
                    ("Discard", Response::Discard),
                    ("Cancel", Response::Cancel),
                ],
            ),
            Prompt::Overwrite(_) => (
                "File changed on disk. Overwrite?".into(),
                vec![
                    ("Overwrite", Response::Overwrite),
                    ("Cancel", Response::Cancel),
                ],
            ),
            Prompt::Quit => (
                "Save changes before quitting?".into(),
                vec![
                    ("Save All", Response::Save),
                    ("Discard", Response::Discard),
                    ("Cancel", Response::Cancel),
                ],
            ),
        };
        let parent = self
            .toplevel()
            .and_then(|w| w.downcast::<gtk::Window>().ok());
        let dialog = gtk::MessageDialog::new(
            parent.as_ref(),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Question,
            gtk::ButtonsType::None,
            &text,
        );
        if matches!(prompt, Prompt::Quit) {
            dialog.set_message_type(gtk::MessageType::Warning);
            dialog.set_secondary_text(Some("Unsaved changes will be lost if you discard them."));
            dialog.set_default_response(gtk::ResponseType::Other(2));
        }
        for (i, (label, _)) in choices.iter().enumerate() {
            dialog.add_button(label, gtk::ResponseType::Other(i as u16));
        }
        let response = dialog.run();
        unsafe {
            dialog.destroy();
        }
        if let gtk::ResponseType::Other(index) = response {
            choices
                .get(index as usize)
                .map(|x| x.1)
                .unwrap_or(Response::Cancel)
        } else {
            Response::Cancel
        }
    }
    fn error(&self, message: String) {
        self.imp().last_error.replace(Some(message.clone()));
        self.emit_by_name::<()>("error", &[&message]);
    }
    pub fn open_file(&self, path: impl AsRef<Path>) {
        self.open_file_with(path, |_| {});
    }
    #[allow(deprecated)]
    pub fn open_file_with(
        &self,
        path: impl AsRef<Path>,
        done: impl FnOnce(Option<Rc<EditorTab>>) + 'static,
    ) {
        if self.imp().closed.get() {
            done(None);
            return;
        }
        let path = file::absolute(path.as_ref());
        if let Some(t) = self.tab_for_path(&path) {
            let n = self.notebook();
            n.set_current_page(n.page_num(&t.page));
            t.view.grab_focus();
            self.announce_current();
            done(Some(t));
            return;
        }
        {
            let mut pending = self.imp().pending.borrow_mut();
            if let Some(p) = pending.get_mut(&path) {
                p.callbacks.push(Box::new(done));
                return;
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker_path = path.clone();
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        std::thread::spawn(move || {
            let result = file::read_file(&worker_path);
            if !worker_cancel.load(Ordering::Relaxed) {
                let _ = tx.send(result);
            }
        });
        let weak = self.downgrade();
        let key = path.clone();
        let source = rx.attach(None, move |result: std::io::Result<Document>| {
            if let Some(p) = weak.upgrade() {
                let pending = p.imp().pending.borrow_mut().remove(&key);
                if let Some(pending) = pending {
                    let tab = if p.imp().closed.get() {
                        None
                    } else {
                        match result {
                            Ok(doc) => Some(p.insert_document(key.clone(), doc)),
                            Err(e) => {
                                p.error(e.to_string());
                                None
                            }
                        }
                    };
                    for callback in pending.callbacks {
                        callback(tab.clone());
                    }
                }
            }
            glib::ControlFlow::Break
        });
        self.imp().pending.borrow_mut().insert(
            path,
            PendingOpen {
                cancel,
                source,
                callbacks: vec![Box::new(done)],
            },
        );
    }
    fn insert_document(&self, path: PathBuf, doc: Document) -> Rc<EditorTab> {
        if let Some(tab) = self.tab_for_path(&path) {
            return tab;
        }
        let trace_start = std::time::Instant::now();
        let trace = |label: &str| {
            if std::env::var_os("IMPRO_EDITOR_TRACE").is_some() {
                eprintln!("panel {label}: {:?}", trace_start.elapsed());
            }
        };
        let tab = EditorTab::new(self, path, doc);
        trace("tab");
        self.imp().tabs.borrow_mut().push(tab.clone());
        let n = self.notebook();
        self.imp().assembling.set(true);
        n.append_page(&tab.page, Some(&tab.header.widget));
        self.imp().assembling.set(false);
        n.set_tab_reorderable(&tab.page, true);
        trace("append");
        tab.page.show_all();
        tab.header.widget.show_all();
        self.stack().set_visible_child_name("editor");
        n.set_current_page(n.page_num(&tab.page));
        trace("show");
        self.switched();
        trace("switch");
        self.focus_current();
        trace("focus");
        tab
    }
    pub fn open_file_at(&self, path: impl AsRef<Path>, line: i32, column: i32, length: i32) {
        self.open_file_with(path, move |tab| {
            if let Some(t) = tab {
                if t.markdown_mode() == Some("preview") {
                    t.set_markdown_mode("source");
                }
                let b = &t.buffer;
                let mut start = b.iter_at_line(line.clamp(0, b.line_count() - 1));
                let mut limit = start;
                if !limit.ends_line() {
                    limit.forward_to_line_end();
                }
                start.set_line_offset(column.clamp(0, limit.line_offset()));
                let mut end = start;
                end.forward_chars(length.clamp(0, limit.line_offset() - start.line_offset()));
                b.select_range(&start, &end);
                t.view.scroll_to_iter(&mut start, 0.0, true, 0.0, 0.5);
                t.view.grab_focus();
                let mark = b.create_mark(None, &start, true).expect("buffer mark");
                let weak = Rc::downgrade(&t);
                glib::idle_add_local_full(glib::Priority::LOW, move || {
                    if let Some(t) = weak.upgrade() {
                        if !t.is_closed() && !mark.is_deleted() {
                            t.view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.5);
                            t.buffer.delete_mark(&mark);
                        }
                    }
                    glib::ControlFlow::Break
                });
            }
        });
    }
    pub fn focus_current(&self) -> bool {
        if let Some(t) = self.current_tab() {
            t.view.grab_focus();
            return true;
        }
        let Some(key) = self.current_view_key() else {
            return false;
        };
        let Some(page) = self.view_tab_widget(&key) else {
            return false;
        };
        let focus = self
            .toplevel()
            .and_then(|w| w.downcast::<gtk::Window>().ok())
            .and_then(|w| w.focused_widget());
        if focus.is_some_and(|w| w == page || w.is_ancestor(&page)) {
            return true;
        }
        if page.can_focus() {
            page.grab_focus();
        } else {
            page.child_focus(gtk::DirectionType::TabForward);
        }
        true
    }
    fn switched(&self) {
        if self.imp().closed.get() || self.imp().assembling.get() {
            return;
        }
        let current = self.current_tab();
        for tab in self.tabs() {
            tab.highlighter
                .set_active(current.as_ref().is_some_and(|t| Rc::ptr_eq(t, &tab)));
        }
        self.update_tabs();
        self.update_status();
        self.announce_current();
        if self.current_view_key().is_some() {
            self.search_bar().set_search_mode(false);
            self.focus_current();
        }
    }
    fn announce_current(&self) {
        let path = self.current_path();
        let changed = {
            let mut old = self.imp().announced.borrow_mut();
            if *old == path {
                false
            } else {
                *old = path.clone();
                true
            }
        };
        if changed {
            let value = path.map(|p| signal_path(&p)).unwrap_or_default();
            self.emit_by_name::<()>("current-file-changed", &[&value]);
        }
    }
    pub(crate) fn dirty_changed(&self) {
        let dirty = self.has_unsaved();
        if self.imp().dirty.replace(dirty) != dirty {
            self.emit_by_name::<()>("dirty-changed", &[&dirty]);
        }
    }
    pub(crate) fn update_status(&self) {
        if self.imp().widgets.get().is_none() {
            return;
        }
        let status = self.status();
        if self.current_view_key().is_some() {
            status.set_text(" | | | ");
            return;
        }
        let t = self.current_tab();
        let cursor = t.as_ref().map(|t| {
            t.buffer
                .iter_at_mark(&t.buffer.get_insert().expect("buffer insert mark"))
        });
        status.set_text(&format!(
            "Ln {}, Col {}  |  UTF-8  |  {}  |  {}",
            cursor.as_ref().map(|i| i.line() + 1).unwrap_or(1),
            cursor.as_ref().map(|i| i.line_offset() + 1).unwrap_or(1),
            t.as_ref().map(|t| t.ending()).unwrap_or("LF"),
            t.as_ref()
                .map(|t| t.language())
                .unwrap_or_else(|| "Plain Text".into())
        ));
    }
    fn size_tabs(&self) {
        let n = self.notebook();
        let width = ((self.allocated_width() - 16) / n.n_pages().max(1) as i32).clamp(120, 200) - 8;
        for tab in self.tabs() {
            if tab.header.widget.width_request() != width {
                tab.header.widget.set_size_request(width, 32);
            }
        }
        for tab in self.view_tabs() {
            if tab.header.widget.width_request() != width {
                tab.header.widget.set_size_request(width, 32);
            }
        }
    }
    fn update_tabs(&self) {
        let n = self.notebook();
        let current = n.current_page();
        let update = |page: &gtk::Widget, h: &TabHeader| {
            let index = n.page_num(page);
            let context = h.inner.style_context();
            if index == current {
                context.add_class("active");
            } else {
                context.remove_class("active");
            }
            h.divider.set_opacity(
                if index == current || index.and_then(|i| i.checked_add(1)) == current {
                    0.0
                } else {
                    1.0
                },
            );
        };
        for t in self.tabs() {
            update(t.page.upcast_ref(), &t.header);
        }
        for t in self.view_tabs() {
            update(&t.page, &t.header);
        }
        self.size_tabs();
    }
    pub fn add_view_tab(
        &self,
        key: &str,
        title: &str,
        widget: &impl IsA<gtk::Widget>,
        tooltip: Option<&str>,
        icon_name: Option<&str>,
    ) -> bool {
        self.add_view_tab_with_shutdown(key, title, widget, tooltip, icon_name, || {})
    }
    pub fn add_view_tab_with_shutdown(
        &self,
        key: &str,
        title: &str,
        widget: &impl IsA<gtk::Widget>,
        tooltip: Option<&str>,
        icon_name: Option<&str>,
        shutdown: impl FnOnce() + 'static,
    ) -> bool {
        if self.imp().closed.get() {
            return false;
        }
        let existing = self.imp().views.borrow().get(key).cloned();
        if let Some(tab) = existing {
            self.notebook()
                .set_current_page(self.notebook().page_num(&tab.page));
            self.focus_current();
            return false;
        }
        let header = TabHeader::new("Close tab", icon_name);
        header.label.set_text(title);
        header.widget.set_tooltip_text(tooltip);
        let tab = Rc::new(ViewTab {
            key: key.into(),
            page: widget.as_ref().clone(),
            header,
            shutdown: RefCell::new(Some(Box::new(shutdown))),
        });
        let weak = self.downgrade();
        let k = key.to_string();
        tab.header.close.connect_clicked(move |_| {
            if let Some(p) = weak.upgrade() {
                p.close_view_tab(&k);
            }
        });
        let weak = self.downgrade();
        let k = key.to_string();
        tab.header.widget.connect_button_press_event(move |_, e| {
            if e.button() == 2 {
                if let Some(p) = weak.upgrade() {
                    p.close_view_tab(&k);
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.imp()
            .views
            .borrow_mut()
            .insert(key.into(), tab.clone());
        let n = self.notebook();
        self.imp().assembling.set(true);
        n.append_page(&tab.page, Some(&tab.header.widget));
        self.imp().assembling.set(false);
        n.set_tab_reorderable(&tab.page, true);
        tab.page.show_all();
        tab.header.widget.show_all();
        self.stack().set_visible_child_name("editor");
        n.set_current_page(n.page_num(&tab.page));
        self.switched();
        true
    }
    pub fn has_view_tab(&self, key: &str) -> bool {
        self.imp().views.borrow().contains_key(key)
    }
    pub fn view_tab_widget(&self, key: &str) -> Option<gtk::Widget> {
        self.imp().views.borrow().get(key).map(|t| t.page.clone())
    }
    pub fn set_view_tab_title(&self, key: &str, title: &str, tooltip: Option<&str>) {
        let tab = self.imp().views.borrow().get(key).cloned();
        if let Some(t) = tab {
            t.header.label.set_text(title);
            t.header.widget.set_tooltip_text(tooltip);
        }
    }
    pub fn close_view_tab(&self, key: &str) -> bool {
        if self.imp().assembling.get() {
            let exists = self.has_view_tab(key);
            let weak = self.downgrade();
            let key = key.to_owned();
            glib::idle_add_local_once(move || {
                if let Some(p) = weak.upgrade() {
                    p.close_view_tab(&key);
                }
            });
            return exists;
        }
        let tab = self.imp().views.borrow_mut().remove(key);
        let Some(t) = tab else {
            return false;
        };
        t.shutdown();
        self.remove_page(&t.page);
        self.emit_by_name::<()>("view-tab-closed", &[&key]);
        true
    }
    fn remove_page(&self, page: &gtk::Widget) {
        let n = self.notebook();
        let index = n.page_num(page);
        let successor = index
            .and_then(|i| n.nth_page(Some(i + 1)))
            .and_then(|p| n.tab_label(&p));
        let gap = n
            .tab_label(page)
            .map(|h| h.allocated_width() + 8)
            .unwrap_or(0);
        if let Some(i) = index {
            n.remove_page(Some(i));
        }
        unsafe {
            page.destroy();
        }
        self.update_tabs();
        if let Some(h) = successor {
            if h.is_mapped() {
                let weak = h.downgrade();
                impro_ui::motion::animate(
                    &h,
                    "tab_close_reflow",
                    move |p| {
                        if let Some(h) = weak.upgrade() {
                            h.set_margin_start((gap as f64 * (1.0 - p)).round() as i32);
                        }
                    },
                    impro_ui::motion::AnimateOptions {
                        key: impro_ui::motion::MotionKey::unique(),
                        easing: impro_ui::motion::easing("reflow").expect("registered easing"),
                        ..Default::default()
                    },
                );
            }
        }
        if n.n_pages() == 0 {
            self.stack().set_visible_child_name("empty");
            self.search_bar().set_search_mode(false);
        }
        self.switched();
        self.dirty_changed();
    }
    pub fn close_current(&self, done: impl FnOnce(bool) + 'static) {
        if let Some(key) = self.current_view_key() {
            done(self.close_view_tab(&key));
        } else if let Some(t) = self.current_tab() {
            self.close_tab(&t, done);
        } else {
            done(true);
        }
    }
    pub fn close_tab(&self, tab: &Rc<EditorTab>, done: impl FnOnce(bool) + 'static) {
        if self.imp().assembling.get() {
            let weak = self.downgrade();
            let tab = tab.clone();
            glib::idle_add_local_once(move || {
                if let Some(p) = weak.upgrade() {
                    p.close_tab(&tab, done);
                } else {
                    done(false);
                }
            });
            return;
        }
        if tab.is_closed() {
            done(true);
            return;
        }
        if tab.buffer.is_modified() || tab.deleted() {
            match self.ask(Prompt::Close(tab.path())) {
                Response::Cancel | Response::Overwrite => {
                    done(false);
                    return;
                }
                Response::Save => {
                    let weak = self.downgrade();
                    let tab = tab.clone();
                    self.save_tab(&tab.clone(), move |ok| {
                        if let Some(p) = weak.upgrade() {
                            if ok && !tab.buffer.is_modified() {
                                p.discard_tab(&tab);
                                done(true);
                            } else {
                                done(false);
                            }
                        } else {
                            done(false);
                        }
                    });
                    return;
                }
                Response::Discard => {}
            }
        }
        self.discard_tab(tab);
        done(true);
    }
    fn discard_tab(&self, tab: &Rc<EditorTab>) {
        if tab.closed.get() {
            return;
        }
        self.imp().tabs.borrow_mut().retain(|t| !Rc::ptr_eq(t, tab));
        let path = tab.path();
        tab.destroy();
        self.remove_page(tab.page.upcast_ref());
        self.emit_by_name::<()>("tab-closed", &[&signal_path(&path)]);
    }
    pub fn save_current(&self, done: impl FnOnce(bool) + 'static) {
        if let Some(tab) = self.current_tab() {
            self.save_tab(&tab, done);
        } else {
            done(true);
        }
    }
    pub fn save_all(&self, done: impl FnOnce(bool) + 'static) {
        self.save_sequence(self.tabs(), 0, true, Box::new(done));
    }
    fn save_sequence(&self, tabs: Vec<Rc<EditorTab>>, index: usize, ok: bool, done: Done) {
        let Some(tab) = tabs.get(index).cloned() else {
            done(ok);
            return;
        };
        let weak = self.downgrade();
        self.save_tab(&tab, move |result| {
            if let Some(p) = weak.upgrade() {
                p.save_sequence(tabs, index + 1, ok && result, done);
            } else {
                done(false);
            }
        });
    }
    #[allow(deprecated)]
    pub fn save_tab(&self, tab: &Rc<EditorTab>, done: impl FnOnce(bool) + 'static) {
        if tab.is_closed() {
            done(false);
            return;
        }
        if !tab.buffer.is_modified() && !tab.deleted() {
            done(true);
            return;
        }
        if tab.saving.replace(true) {
            done(false);
            return;
        }
        let path = tab.path();
        let check = path.clone();
        let expected = tab.mtime.get();
        let deleted = tab.deleted();
        let weak = self.downgrade();
        let t = Rc::downgrade(tab);
        self.worker(
            move || match std::fs::metadata(check) {
                Ok(m) => m.modified().map(Some),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e),
            },
            move |result: std::io::Result<Option<SystemTime>>| {
                let (Some(p), Some(tab)) = (weak.upgrade(), t.upgrade()) else {
                    done(false);
                    return;
                };
                if tab.is_closed() {
                    tab.saving.set(false);
                    done(false);
                    return;
                }
                let current = match result {
                    Ok(t) => t,
                    Err(e) => {
                        tab.saving.set(false);
                        p.error(format!("Cannot save {}: {e}", path.display()));
                        done(false);
                        return;
                    }
                };
                if current != Some(expected)
                    && !(deleted && current.is_none())
                    && p.ask(Prompt::Overwrite(path.clone())) != Response::Overwrite
                {
                    tab.saving.set(false);
                    done(false);
                    return;
                }
                if tab.is_closed() {
                    tab.saving.set(false);
                    done(false);
                    return;
                }
                let text = tab.text();
                let copy = text.clone();
                let ending = tab.ending;
                let revision = tab.revision.get();
                let target = tab.path();
                let write_target = target.clone();
                let weak = p.downgrade();
                let t = Rc::downgrade(&tab);
                p.worker(
                    move || file::atomic_write(write_target, &copy, ending),
                    move |result| {
                        let (Some(p), Some(tab)) = (weak.upgrade(), t.upgrade()) else {
                            done(false);
                            return;
                        };
                        tab.saving.set(false);
                        match result {
                            Ok(mtime) => {
                                if !tab.is_closed() && tab.path() == target {
                                    tab.mtime.set(mtime);
                                    tab.mark_saved(text, revision);
                                    p.dirty_changed();
                                    p.emit_by_name::<()>("file-saved", &[&signal_path(&target)]);
                                }
                                done(true);
                            }
                            Err(e) => {
                                p.error(format!("Cannot save {}: {e}", target.display()));
                                done(false);
                            }
                        }
                    },
                );
            },
        );
    }
    #[allow(deprecated)]
    fn worker<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
        done: impl FnOnce(T) + 'static,
    ) {
        let id = self.imp().next.get();
        self.imp().next.set(id + 1);
        let cancel = Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        std::thread::spawn(move || {
            if c.load(Ordering::Relaxed) {
                return;
            }
            let value = work();
            if !c.load(Ordering::Relaxed) {
                let _ = tx.send(value);
            }
        });
        let weak = self.downgrade();
        let mut done = Some(done);
        let source = rx.attach(None, move |value| {
            if let Some(p) = weak.upgrade() {
                p.imp().jobs.borrow_mut().remove(&id);
                if !p.imp().closed.get() {
                    if let Some(done) = done.take() {
                        done(value);
                    }
                }
            }
            glib::ControlFlow::Break
        });
        self.imp().jobs.borrow_mut().insert(id, (cancel, source));
    }
    pub fn request_quit(&self, done: impl FnOnce(bool) + 'static) {
        if !self.has_unsaved() {
            done(true);
            return;
        }
        match self.ask(Prompt::Quit) {
            Response::Discard => done(true),
            Response::Save => {
                let weak = self.downgrade();
                self.save_all(move |ok| {
                    done(ok && weak.upgrade().is_some_and(|p| !p.has_unsaved()))
                });
            }
            _ => done(false),
        }
    }
    pub fn on_path_deleted(&self, path: impl AsRef<Path>) {
        let path = file::absolute(path.as_ref());
        for t in self.tabs() {
            if t.path().starts_with(&path) {
                t.deleted.set(true);
                t.buffer.set_modified(true);
                t.update_title();
            }
        }
        self.dirty_changed();
    }
    /// Retarget an exact file without appending an empty descendant component.
    /// Returns true when an open file was moved. Directory callers can use
    /// `on_path_renamed` when this returns false.
    pub fn on_file_renamed(&self, old: &Path, new: &Path) -> bool {
        let old = file::absolute(old);
        let new = file::absolute(new);
        let Some(tab) = self.tab_for_path(&old) else {
            return false;
        };
        tab.set_path(new.clone());
        let expected = new.clone();
        let weak = Rc::downgrade(&tab);
        self.worker(
            move || new.canonicalize().unwrap_or(new),
            move |resolved| {
                if let Some(tab) = weak
                    .upgrade()
                    .filter(|tab| !tab.is_closed() && tab.path() == expected)
                {
                    tab.set_resolved_path(resolved);
                }
            },
        );
        self.update_status();
        self.announce_current();
        true
    }
    pub fn on_path_renamed(&self, old: impl AsRef<Path>, new: impl AsRef<Path>) {
        let old = file::absolute(old.as_ref());
        let new = file::absolute(new.as_ref());
        for t in self.tabs() {
            if let Ok(rest) = t.path().strip_prefix(&old) {
                let path = new.join(rest);
                t.set_path(path.clone());
                if crate::tab::is_markdown(&path) {
                    let weak = Rc::downgrade(&t);
                    let expected = path.clone();
                    self.worker(
                        move || path.canonicalize().unwrap_or(path),
                        move |resolved| {
                            if let Some(t) = weak.upgrade() {
                                if !t.is_closed() && t.path() == expected {
                                    t.set_resolved_path(resolved);
                                }
                            }
                        },
                    );
                }
            }
        }
        self.update_status();
        self.announce_current();
    }
    pub fn set_root(&self, path: Option<&Path>) {
        let root = path.map(file::absolute);
        self.imp().config.borrow_mut().root = root.clone();
        let revision = self.imp().root_revision.get() + 1;
        self.imp().root_revision.set(revision);
        if let Some(root) = root {
            let weak = self.downgrade();
            self.worker(
                move || root.canonicalize().unwrap_or(root),
                move |resolved| {
                    if let Some(p) = weak.upgrade() {
                        if p.imp().root_revision.get() == revision {
                            p.imp().config.borrow_mut().root = Some(resolved);
                            p.reclassify_markdown();
                        }
                    }
                },
            );
        }
        self.reclassify_markdown();
    }
    fn reclassify_markdown(&self) {
        let focus = self
            .toplevel()
            .and_then(|w| w.downcast::<gtk::Window>().ok())
            .and_then(|w| w.focused_widget());
        for t in self.tabs() {
            if crate::tab::is_markdown(&t.path()) {
                t.sync_markdown();
            }
        }
        if let Some(w) = focus {
            w.grab_focus();
        }
        self.update_status();
    }
    pub fn set_line_numbers(&self, enabled: bool) {
        self.imp().config.borrow_mut().line_numbers = enabled;
        for t in self.tabs() {
            t.chrome
                .set_line_numbers(enabled && t.markdown_mode() != Some("preview"));
        }
    }
    pub fn set_highlight_line(&self, enabled: bool) {
        self.imp().config.borrow_mut().highlight_line = enabled;
        for t in self.tabs() {
            t.chrome
                .set_highlight_line(enabled && t.markdown_mode() != Some("preview"));
        }
    }
    pub fn set_indent(&self, tabs: bool, width: i32) -> Result<(), &'static str> {
        if !(1..=16).contains(&width) {
            return Err("Indent width must be between 1 and 16");
        }
        {
            let mut c = self.imp().config.borrow_mut();
            c.indent_tabs = tabs;
            c.indent_width = width;
        }
        for t in self.tabs() {
            t.set_tab_stops();
        }
        Ok(())
    }
    pub fn set_auto_pair(&self, enabled: bool) {
        self.imp().config.borrow_mut().auto_pair = enabled;
    }
    pub fn set_md_live_preview(&self, enabled: bool) {
        self.imp().config.borrow_mut().md_live_preview = enabled;
    }
    pub fn set_slash_commands(&self, enabled: bool) {
        self.imp().config.borrow_mut().slash_commands = enabled;
        for t in self.tabs() {
            t.sync_slash_menu();
        }
    }
    pub fn set_selection_toolbar(&self, enabled: bool) {
        impro_markdown::seltoolbar::set_enabled(self, enabled);
    }
    pub fn set_md_readable_width(&self, enabled: bool) {
        self.imp().config.borrow_mut().md_readable_width = enabled;
        for t in self.tabs() {
            let r = t.md_renderer.borrow().clone();
            if let Some(r) = r {
                r.set_readable_width(enabled);
            }
        }
    }
    pub fn set_markdown_mode(&self, mode: &str, path: Option<&Path>) -> Result<(), &'static str> {
        if !matches!(mode, "source" | "preview") {
            return Err("Markdown mode must be source or preview");
        }
        let tab = path.and_then(|p| self.tab_for_path(p)).or_else(|| {
            if path.is_none() {
                self.current_tab()
            } else {
                None
            }
        });
        if let Some(t) = tab {
            t.set_markdown_mode(mode);
        }
        Ok(())
    }
    pub fn markdown_mode(&self, path: Option<&Path>) -> Option<&'static str> {
        let tab = if let Some(p) = path {
            self.tab_for_path(p)
        } else {
            self.current_tab()
        };
        tab.and_then(|t| t.markdown_mode())
    }
    pub fn set_syntax_style(&self, styles: &SyntaxColors) {
        self.imp().config.borrow_mut().syntax = styles.clone();
        for t in self.tabs() {
            t.highlighter.set_style(styles);
            t.style_markdown();
        }
    }
    pub fn set_chrome(&self, colors: &UiColors) {
        {
            let mut c = self.imp().config.borrow_mut();
            for (k, v) in colors {
                if c.chrome.contains_key(k) && crate::gutter::valid_color(v) {
                    c.chrome.insert(k.clone(), v.clone());
                }
            }
        }
        let colors = self.chrome();
        for t in self.tabs() {
            t.chrome.apply(&colors);
            t.style_markdown();
        }
    }
    pub fn set_font_size(&self, size: i32) {
        let size = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        if self.font_size() == size {
            return;
        }
        self.imp().config.borrow_mut().font = size;
        for t in self.tabs() {
            t.set_font_size(size);
        }
        self.emit_by_name::<()>("font-size-changed", &[&size]);
    }
    pub fn set_default_font_size(&self, size: i32) {
        self.imp().config.borrow_mut().default_font = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
    }
    pub fn font_family(&self) -> Option<String> {
        self.imp().config.borrow().font_family.clone()
    }
    pub fn set_font_family(&self, family: Option<&str>) {
        self.imp().config.borrow_mut().font_family =
            family.filter(|s| !s.is_empty()).map(str::to_owned);
        for t in self.tabs() {
            t.chrome.set_font_family(family);
            t.set_tab_stops();
        }
    }
    pub fn zoom(&self, step: i32) {
        if self.current_view_key().is_none() {
            self.set_font_size(self.font_size().saturating_add(step));
        }
    }
    pub fn apply_theme(&self, theme: &Theme) {
        self.set_syntax_style(&theme.syntax);
        self.set_chrome(&theme.chrome);
    }
    pub fn apply_settings(&self, values: &serde_json::Map<String, serde_json::Value>) {
        let flag = |key: &str| values.get(key).and_then(|v| v.as_bool());
        if let Some(v) = flag("line_numbers") {
            self.set_line_numbers(v);
        }
        if let Some(v) = flag("highlight_line") {
            self.set_highlight_line(v);
        }
        let tabs = flag("indent_tabs").unwrap_or(self.indent_tabs());
        let width = values
            .get("indent_width")
            .and_then(|v| v.as_i64())
            .unwrap_or(self.indent_width() as i64)
            .clamp(1, 16) as i32;
        let _ = self.set_indent(tabs, width);
        if let Some(v) = flag("auto_pair") {
            self.set_auto_pair(v);
        }
        if let Some(v) = flag("md_live_preview") {
            self.set_md_live_preview(v);
        }
        if let Some(v) = flag("md_readable_width") {
            self.set_md_readable_width(v);
        }
        if let Some(v) = flag("notes_slash_commands") {
            self.set_slash_commands(v);
        }
        if let Some(v) = flag("notes_selection_toolbar") {
            self.set_selection_toolbar(v);
        }
        if let Some(size) = values.get("editor_font_size").and_then(|v| v.as_i64()) {
            let size = size.clamp(7, 32) as i32;
            self.set_default_font_size(size);
            self.set_font_size(size);
        }
        if let Some(family) = values.get("editor_font").and_then(|v| v.as_str()) {
            self.set_font_family(Some(family));
        }
    }
    pub fn focus_find(&self) {
        if self.current_tab().is_some() {
            self.search_bar().set_search_mode(true);
            self.search_entry().grab_focus();
        }
    }
    pub fn find(&self, backwards: bool, from_cursor: bool) {
        let query = self.search_entry().text();
        let style = self.search_entry().style_context();
        let Some(t) = self.current_tab() else {
            style.remove_class("error");
            return;
        };
        if query.is_empty() {
            style.remove_class("error");
            return;
        }
        let b = &t.buffer;
        let origin = b
            .selection_bounds()
            .map(|(a, z)| if backwards || from_cursor { a } else { z })
            .unwrap_or_else(|| b.iter_at_mark(&b.get_insert().expect("buffer insert mark")));
        let flags = gtk::TextSearchFlags::CASE_INSENSITIVE | gtk::TextSearchFlags::TEXT_ONLY;
        let search = |i: gtk::TextIter| {
            if backwards {
                i.backward_search(&query, flags, None)
            } else {
                i.forward_search(&query, flags, None)
            }
        };
        let found = search(origin).or_else(|| {
            search(if backwards {
                b.end_iter()
            } else {
                b.start_iter()
            })
        });
        if let Some((mut a, z)) = found {
            style.remove_class("error");
            b.select_range(&a, &z);
            t.view.scroll_to_iter(&mut a, 0.1, false, 0.0, 0.0);
        } else {
            style.add_class("error");
        }
    }
    fn tab_shortcut(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as k;
        if !event.state().contains(gdk::ModifierType::CONTROL_MASK) {
            return false;
        }
        let key = event.keyval();
        if key == k::w || key == k::W {
            self.close_current(|_| {});
            return true;
        }
        if [k::Tab, k::ISO_Left_Tab, k::Page_Down, k::Page_Up].contains(&key) {
            let n = self.notebook();
            let count = n.n_pages();
            if count > 0 {
                let back = event.state().contains(gdk::ModifierType::SHIFT_MASK)
                    || [k::ISO_Left_Tab, k::Page_Up].contains(&key);
                let current = n.current_page().unwrap_or(0);
                n.set_current_page(Some(if back {
                    (current + count - 1) % count
                } else {
                    (current + 1) % count
                }));
                self.focus_current();
            }
            return true;
        }
        false
    }
    pub fn handle_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as k;
        let tab = self.current_tab();
        if let Some(t) = &tab {
            let menu = t.slash_menu.borrow().clone();
            if menu.is_some_and(|m| m.handle_key(&t.view, event)) {
                return true;
            }
        }
        if self.tab_shortcut(event) {
            return true;
        }
        let Some(t) = tab else {
            return false;
        };
        let key = event.keyval();
        let ctrl = event.state().contains(gdk::ModifierType::CONTROL_MASK);
        let shift = event.state().contains(gdk::ModifierType::SHIFT_MASK);
        if ctrl {
            if shift && [k::m, k::M].contains(&key) && t.markdown_mode().is_some() {
                t.set_markdown_mode(if t.markdown_mode() == Some("source") {
                    "preview"
                } else {
                    "source"
                });
            } else if [k::z, k::Z].contains(&key) {
                t.undo(shift);
            } else if [k::y, k::Y].contains(&key) {
                t.undo(true);
            } else if [k::f, k::F].contains(&key) && !shift {
                self.focus_find();
            } else if key == k::slash {
                self.edit("comment");
            } else if [k::plus, k::equal, k::KP_Add].contains(&key) {
                self.zoom(1);
            } else if [k::minus, k::KP_Subtract].contains(&key) {
                self.zoom(-1);
            } else if [k::_0, k::KP_0].contains(&key) {
                self.set_font_size(self.default_font_size());
            } else {
                return false;
            }
            return true;
        }
        if !t.view.is_editable() {
            return false;
        }
        let renderer = t.md_renderer.borrow().clone();
        if renderer.is_some_and(|r| r.edit_key(event, self.indent_tabs(), self.indent_width())) {
            return true;
        }
        if self.auto_pair()
            && !event
                .state()
                .intersects(gdk::ModifierType::MOD1_MASK | gdk::ModifierType::SUPER_MASK)
        {
            if let Some(c) = key.to_unicode() {
                let b = &t.buffer;
                let cursor = b.iter_at_mark(&b.get_insert().expect("buffer insert mark"));
                let selection = b.selection_bounds();
                if ")]}\"'".contains(c) && selection.is_none() && cursor.char() == Some(c) {
                    let mut next = cursor;
                    next.forward_char();
                    b.place_cursor(&next);
                    return true;
                }
                let closer = match c {
                    '(' => Some(')'),
                    '[' => Some(']'),
                    '{' => Some('}'),
                    '\'' => Some('\''),
                    '"' => Some('"'),
                    _ => None,
                };
                if let Some(closer) = closer {
                    let start = selection
                        .map(|(a, _)| a.offset())
                        .unwrap_or(cursor.offset());
                    let selected = selection
                        .map(|(a, z)| b.text(&a, &z, true).unwrap_or_default().to_string())
                        .unwrap_or_default();
                    b.begin_user_action();
                    b.delete_selection(true, true);
                    b.insert_at_cursor(&format!("{c}{selected}{closer}"));
                    if selection.is_some() {
                        b.select_range(
                            &b.iter_at_offset(start + 1),
                            &b.iter_at_offset(start + 1 + selected.chars().count() as i32),
                        );
                    } else {
                        b.place_cursor(&b.iter_at_offset(start + 1));
                    }
                    b.end_user_action();
                    return true;
                }
            }
        }
        if [k::Return, k::KP_Enter].contains(&key) {
            self.edit("enter");
            true
        } else if key == k::Tab || key == k::ISO_Left_Tab {
            self.edit(if shift || key == k::ISO_Left_Tab {
                "unindent"
            } else {
                "tab"
            });
            true
        } else {
            false
        }
    }
    pub fn edit(&self, command: &str) {
        let Some(t) = self.current_tab() else {
            return;
        };
        if !t.view.is_editable() {
            return;
        }
        if command == "comment"
            && !t.path().extension().is_some_and(|e| {
                ["py", "sh", "toml", "yaml", "yml", "conf"]
                    .iter()
                    .any(|s| e.eq_ignore_ascii_case(s))
            })
        {
            return;
        }
        let b = &t.buffer;
        let cursor = b.iter_at_mark(&b.get_insert().expect("buffer insert mark"));
        let mut start = cursor;
        start.set_line_offset(0);
        b.begin_user_action();
        if command == "enter" || command == "tab" {
            let content = if command == "enter" {
                format!(
                    "\n{}",
                    file::leading_whitespace(&b.text(&start, &cursor, true).unwrap_or_default())
                )
            } else if self.indent_tabs() {
                "\t".into()
            } else {
                " ".repeat(self.indent_width() as usize)
            };
            b.delete_selection(true, true);
            b.insert_at_cursor(&content);
        } else {
            let mut end = cursor;
            if let Some((a, z)) = b.selection_bounds() {
                start = a;
                end = z;
                if end.line_offset() == 0 && end.offset() > start.offset() {
                    end.backward_char();
                }
                start.set_line_offset(0);
            }
            if !end.ends_line() {
                end.forward_to_line_end();
            }
            let content = b.text(&start, &end, true).unwrap_or_default();
            let replacement = if command == "comment" {
                file::toggle_comments(&content)
            } else {
                content
                    .split('\n')
                    .map(|s| file::unindent(s, self.indent_width() as usize))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let offset = start.offset();
            b.delete(&mut start, &mut end);
            b.insert(&mut b.iter_at_offset(offset), &replacement);
        }
        b.end_user_action();
    }
    pub fn shutdown(&self) {
        if self.imp().closed.replace(true) {
            return;
        }
        let pending = std::mem::take(&mut *self.imp().pending.borrow_mut());
        for (_, p) in pending {
            p.cancel.store(true, Ordering::Relaxed);
            p.source.remove();
        }
        let jobs = std::mem::take(&mut *self.imp().jobs.borrow_mut());
        for (_, (cancel, source)) in jobs {
            cancel.store(true, Ordering::Relaxed);
            source.remove();
        }
        let views = std::mem::take(&mut *self.imp().views.borrow_mut());
        for (_, v) in views {
            v.shutdown();
        }
        let tabs = std::mem::take(&mut *self.imp().tabs.borrow_mut());
        for t in tabs {
            t.destroy();
        }
    }
}
fn signal_path(path: &Path) -> String {
    path.to_str()
        .map(str::to_owned)
        .unwrap_or_else(|| file::path_signal(path))
}
fn decoded_path(value: &str) -> PathBuf {
    if value.starts_with("file:") {
        glib::filename_from_uri(value)
            .map(|x| x.0)
            .unwrap_or_else(|_| value.into())
    } else {
        value.into()
    }
}
impl EditorPanel {
    pub fn connect_current_file_changed(
        &self,
        f: impl Fn(&Self, Option<PathBuf>) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_local("current-file-changed", false, move |v| {
            let p = v[0].get::<Self>().expect("signal owner");
            let s = v[1].get::<String>().expect("signal path");
            f(
                &p,
                if s.is_empty() {
                    None
                } else {
                    Some(decoded_path(&s))
                },
            );
            None
        })
    }
    pub fn connect_view_tab_closed(
        &self,
        f: impl Fn(&Self, &str) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_local("view-tab-closed", false, move |v| {
            let p = v[0].get::<Self>().expect("signal owner");
            let key = v[1].get::<String>().expect("signal key");
            f(&p, &key);
            None
        })
    }
    pub fn connect_file_saved(&self, f: impl Fn(&Self, &Path) + 'static) -> glib::SignalHandlerId {
        self.connect_local("file-saved", false, move |v| {
            let p = v[0].get::<Self>().expect("signal owner");
            let s = v[1].get::<String>().expect("signal path");
            f(&p, &decoded_path(&s));
            None
        })
    }
    pub fn connect_tab_closed(&self, f: impl Fn(&Self, &Path) + 'static) -> glib::SignalHandlerId {
        self.connect_local("tab-closed", false, move |v| {
            let p = v[0].get::<Self>().expect("signal owner");
            let s = v[1].get::<String>().expect("signal path");
            f(&p, &decoded_path(&s));
            None
        })
    }
    pub fn connect_dirty_changed(
        &self,
        f: impl Fn(&Self, bool) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_local("dirty-changed", false, move |v| {
            f(
                &v[0].get::<Self>().expect("signal owner"),
                v[1].get().expect("dirty value"),
            );
            None
        })
    }
    pub fn connect_font_size_changed(
        &self,
        f: impl Fn(&Self, i32) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_local("font-size-changed", false, move |v| {
            f(
                &v[0].get::<Self>().expect("signal owner"),
                v[1].get().expect("font size"),
            );
            None
        })
    }
    pub fn connect_error(&self, f: impl Fn(&Self, &str) + 'static) -> glib::SignalHandlerId {
        self.connect_local("error", false, move |v| {
            f(
                &v[0].get::<Self>().expect("signal owner"),
                &v[1].get::<String>().expect("error message"),
            );
            None
        })
    }
}
#[derive(Clone)]
pub struct StatusBar {
    pub widget: gtk::Box,
    pub items: Vec<gtk::Label>,
    pub targets: Vec<gtk::EventBox>,
}
impl StatusBar {
    fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        widget.style_context().add_class("impro-status");
        widget.set_halign(gtk::Align::End);
        widget.set_valign(gtk::Align::End);
        let mut items = Vec::new();
        let mut targets = Vec::new();
        for index in 0..4 {
            if index > 0 {
                widget.pack_start(&gtk::Label::new(Some("|")), false, false, 0);
            }
            let event = gtk::EventBox::new();
            let surface = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            surface.style_context().add_class("impro-status-item");
            let label = gtk::Label::new(None);
            surface.add(&label);
            event.add(&surface);
            widget.pack_start(&event, false, false, 0);
            let weak = surface.downgrade();
            event.connect_enter_notify_event(move |_, _| {
                if let Some(w) = weak.upgrade() {
                    w.set_state_flags(gtk::StateFlags::PRELIGHT, false);
                }
                glib::Propagation::Proceed
            });
            let weak = surface.downgrade();
            event.connect_leave_notify_event(move |_, _| {
                if let Some(w) = weak.upgrade() {
                    w.unset_state_flags(gtk::StateFlags::PRELIGHT);
                }
                glib::Propagation::Proceed
            });
            items.push(label);
            targets.push(event);
        }
        Self {
            widget,
            items,
            targets,
        }
    }
    pub fn set_text(&self, text: &str) {
        for (label, value) in self.items.iter().zip(text.split('|')) {
            if label.text().as_str() != value.trim() {
                label.set_text(value.trim());
            }
        }
    }
    pub fn text(&self) -> String {
        self.items
            .iter()
            .map(|l| l.text())
            .collect::<Vec<_>>()
            .join("  |  ")
    }
}
