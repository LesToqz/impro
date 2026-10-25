//! Windows stand-in for the WebKit hosts: same public items, a notice instead of a page.
//!
//! WebKitGTK does not exist on Windows. Every method keeps its Unix signature except
//! `webview()`, which is omitted because its `webkit2gtk::WebView` return type cannot exist
//! here. Nothing ever loads or fires: `connect_open_commit`, `connect_error` and
//! `connect_open_file` callbacks are accepted and never invoked. A later task replaces
//! this module with a real renderer.
use crate::types;
use gtk::prelude::*;
use impro_core::{
    settings,
    themes::{Theme, UiColors},
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

pub use types::{GraphSettings, PaintTimings};

/// Syntax adapters must be safe to use from Git workers. No editor dependency is required.
pub type SyntaxAdapter = Arc<dyn crate::diff::Highlighter + Send + Sync>;
type VoidCallback = Rc<dyn Fn()>;

/// Text of the notice shown in place of the Git pages.
pub const NOTICE: &str = "Git views are not available in this Windows build yet.";

fn notice_widget() -> gtk::Box {
    let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let label = gtk::Label::new(Some(NOTICE));
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    label.set_hexpand(true);
    label.set_vexpand(true);
    label.set_line_wrap(true);
    label.style_context().add_class("dim-label");
    widget.add(&label);
    widget.show_all();
    widget
}

struct Inner {
    widget: gtk::Box,
    root: RefCell<PathBuf>,
    rev: String,
    closed: Cell<bool>,
    graph_settings: RefCell<GraphSettings>,
    root_changed: RefCell<Vec<VoidCallback>>,
}
impl Inner {
    fn new(root: &Path, rev: &str) -> Rc<Self> {
        Rc::new(Self {
            widget: notice_widget(),
            root: RefCell::new(root.to_path_buf()),
            rev: rev.to_owned(),
            closed: Cell::new(false),
            graph_settings: RefCell::new(GraphSettings::default()),
            root_changed: RefCell::new(Vec::new()),
        })
    }
}

#[derive(Clone)]
pub struct GitGraphView(Rc<Inner>);
impl GitGraphView {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self(Inner::new(root.as_ref(), ""))
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.widget
    }
    pub fn timings(&self) -> PaintTimings {
        PaintTimings::default()
    }
    pub fn is_ready(&self) -> bool {
        false
    }
    pub fn is_closed(&self) -> bool {
        self.0.closed.get()
    }
    pub fn root(&self) -> PathBuf {
        self.0.root.borrow().clone()
    }
    pub fn last_data(&self) -> Option<Arc<Value>> {
        None
    }
    pub fn refresh(&self) {}
    pub fn focus_head(&self) {}
    pub fn grab_focus(&self) {}
    pub fn select(&self, _hash: Option<&str>) {}
    /// Invalid values leave settings unchanged.
    pub fn set_orientation(&self, value: &str) -> Result<(), &'static str> {
        if !matches!(value, "horizontal" | "vertical") {
            return Err("Orientation must be horizontal or vertical");
        }
        self.0.graph_settings.borrow_mut().orientation = value.into();
        Ok(())
    }
    pub fn orientation(&self) -> String {
        self.0.graph_settings.borrow().orientation.clone()
    }
    pub fn set_options(
        &self,
        show_remotes: bool,
        show_tags: bool,
        time_format: &str,
    ) -> Result<(), &'static str> {
        if !matches!(time_format, "relative" | "absolute") {
            return Err("Time format must be relative or absolute");
        }
        let mut s = self.0.graph_settings.borrow_mut();
        s.show_remotes = show_remotes;
        s.show_tags = show_tags;
        s.time_format = time_format.into();
        Ok(())
    }
    pub fn set_max_commits(&self, limit: i64) {
        self.0.graph_settings.borrow_mut().max_commits = limit.clamp(100, 10000);
    }
    pub fn settings(&self) -> GraphSettings {
        self.0.graph_settings.borrow().clone()
    }
    pub fn set_settings(&self, settings: GraphSettings) {
        *self.0.graph_settings.borrow_mut() = GraphSettings::from_value(&settings.to_value());
    }
    /// Read raw stored settings off-thread, then apply them like the Unix host does.
    pub fn load_settings(&self) {
        #[allow(deprecated)]
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        let weak = Rc::downgrade(&self.0);
        rx.attach(None, move |value| {
            if let Some(inner) = weak.upgrade() {
                if !inner.closed.get() {
                    GitGraphView(inner).set_settings(GraphSettings::from_value(&value));
                }
            }
            glib::ControlFlow::Break
        });
        std::thread::spawn(move || {
            let _ = tx.send(settings::get_gitgraph());
        });
    }
    pub fn set_theme(&self, _ui: &UiColors, _accent: &str) {}
    pub fn set_reduce_motion(&self, _value: bool) {}
    /// Notify the window to close that repository's commit tabs.
    pub fn set_root(&self, root: impl AsRef<Path>) {
        if self.is_closed() {
            return;
        }
        *self.0.root.borrow_mut() = root.as_ref().to_path_buf();
        let callbacks = self.0.root_changed.borrow().clone();
        for cb in callbacks {
            cb();
        }
    }
    pub fn connect_open_commit(&self, _f: impl Fn(&str) + 'static) {}
    pub fn connect_error(&self, _f: impl Fn(&str) + 'static) {}
    pub fn connect_orientation_changed(&self, _f: impl Fn(&str) + 'static) {}
    pub fn connect_root_changed(&self, f: impl Fn() + 'static) {
        self.0.root_changed.borrow_mut().push(Rc::new(f));
    }
    pub fn shutdown(&self) {
        self.0.closed.set(true);
    }
}

/// Read-only commit tab placeholder.
#[derive(Clone)]
pub struct CommitView(Rc<Inner>);
impl CommitView {
    pub fn new(root: impl AsRef<Path>, rev: &str, _highlighter: Option<SyntaxAdapter>) -> Self {
        Self(Inner::new(root.as_ref(), rev))
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.widget
    }
    pub fn rev(&self) -> &str {
        &self.0.rev
    }
    pub fn tab_key(&self) -> String {
        format!("commit:{}", self.rev())
    }
    pub fn timings(&self) -> PaintTimings {
        PaintTimings::default()
    }
    pub fn is_ready(&self) -> bool {
        false
    }
    pub fn is_closed(&self) -> bool {
        self.0.closed.get()
    }
    pub fn last_data(&self) -> Option<Arc<Value>> {
        None
    }
    pub fn reload(&self) {}
    pub fn grab_focus(&self) {}
    pub fn set_theme(&self, _theme: &Theme, _ui: &UiColors, _accent: &str, _size: i32) {}
    pub fn set_font_size(&self, _size: i32) {}
    pub fn set_reduce_motion(&self, _value: bool) {}
    pub fn connect_open_commit(&self, _f: impl Fn(&str) + 'static) {}
    pub fn connect_error(&self, _f: impl Fn(&str) + 'static) {}
    pub fn connect_open_file(&self, _f: impl Fn(&Path, i32) + 'static) {}
    pub fn shutdown(&self) {
        self.0.closed.set(true);
    }
}
