use crate::{
    panesplit::PaneSplit,
    resizegate::ResizeGate,
    terminal_bar::{self, AddButton, Fade, LimitToast, PaneHeader},
};
use gtk::{prelude::*, subclass::prelude::*};
use impro_core::themes::{TerminalColors, Theme};
use impro_vte::{CursorBlinkMode, CursorShape, Format, Terminal};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
};

pub const DEFAULT_MAX_PANES: usize = 6;
pub const HEADER_HEIGHT: i32 = 30;
pub const ANIMATION_MS: u32 = 220;
pub const SCROLLBACK_RANGE: (i64, i64) = (1000, 100000);
type Factory = Rc<dyn Fn() -> gtk::Widget>;

/// Shell used when `SHELL` (Unix) is unset or not executable.
#[cfg(unix)]
pub const DEFAULT_SHELL: &str = "/bin/bash";
#[cfg(windows)]
pub const DEFAULT_SHELL: &str = "cmd.exe";

/// Resolve the executable shell. Call on a worker when GTK's loop is running.
#[cfg(unix)]
pub fn pick_shell() -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let shell = std::env::var_os("SHELL").map(PathBuf::from);
    shell
        .filter(|p| {
            p.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
        .unwrap_or_else(|| DEFAULT_SHELL.into())
}
/// Resolve a configured shell, then PowerShell 7, Windows PowerShell, and cmd.
#[cfg(windows)]
pub fn pick_shell() -> PathBuf {
    fn find(name: &str) -> Option<PathBuf> {
        let path = PathBuf::from(name);
        if path.is_file() {
            return Some(path);
        }
        std::env::var_os("PATH")
            .into_iter()
            .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
            .map(|p| p.join(name))
            .find(|p| p.is_file())
    }
    if let Some(shell) = std::env::var_os("SHELL").and_then(|s| find(&s.to_string_lossy())) {
        return shell;
    }
    if let Some(shell) = find("pwsh.exe") {
        return shell;
    }
    if let Some(shell) = std::env::var_os("ProgramFiles")
        .map(|p| {
            PathBuf::from(p)
                .join("PowerShell")
                .join("7")
                .join("pwsh.exe")
        })
        .filter(|p| p.is_file())
    {
        return shell;
    }
    if let Some(shell) = find("powershell.exe") {
        return shell;
    }
    if let Some(shell) = std::env::var_os("SystemRoot")
        .map(|p| {
            PathBuf::from(p)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .filter(|p| p.is_file())
    {
        return shell;
    }
    std::env::var_os("ComSpec")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .unwrap_or_else(|| DEFAULT_SHELL.into())
}
/// The filesystem root used as the last-resort working directory.
#[cfg(unix)]
fn fs_root() -> PathBuf {
    "/".into()
}
#[cfg(windows)]
fn fs_root() -> PathBuf {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    PathBuf::from(format!("{drive}\\"))
}
/// Whether a directory can be entered (`access(X_OK)` on Unix, listable on Windows).
#[cfg(unix)]
fn is_searchable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes())
        .is_ok_and(|c| unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0)
}
#[cfg(windows)]
fn is_searchable(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok()
}
/// Existing searchable directory, falling back to the home directory then the root.
/// Filesystem checks belong on a worker thread.
pub fn safe_cwd(path: impl AsRef<Path>) -> PathBuf {
    for p in [
        path.as_ref().to_path_buf(),
        impro_core::platform::home_dir().unwrap_or_default(),
        fs_root(),
    ] {
        if p.is_dir() && is_searchable(&p) {
            return if p.is_absolute() {
                p
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| fs_root())
                    .join(p)
            };
        }
    }
    fs_root()
}
/// Convert the spawn result to the numeric child id stored by the panel.
#[cfg(unix)]
pub(crate) fn pid_number(pid: glib::Pid) -> i32 {
    pid.0
}
#[cfg(windows)]
pub(crate) fn pid_number(pid: glib::Pid) -> i32 {
    impro_vte::pid_number(pid)
}
/// Ask a child to hang up (`SIGHUP`).
#[cfg(unix)]
pub(crate) fn hangup_pid(pid: i32) {
    unsafe {
        libc::kill(pid, libc::SIGHUP);
    }
}
#[cfg(windows)]
pub(crate) fn hangup_pid(pid: i32) {
    impro_vte::hangup_pid(pid);
}

// Reserve admission before constructing widgets that can emit user signals.
struct Admission<'a> {
    panel: &'a TerminalPanel,
    kind: Option<String>,
}
impl<'a> Admission<'a> {
    fn new(panel: &'a TerminalPanel, kind: Option<&str>) -> Self {
        panel.imp().pending.set(panel.imp().pending.get() + 1);
        let kind = kind.map(str::to_string);
        if let Some(kind) = &kind {
            panel.imp().pending_custom.borrow_mut().push(kind.clone());
        }
        Self { panel, kind }
    }
}
impl Drop for Admission<'_> {
    fn drop(&mut self) {
        self.panel
            .imp()
            .pending
            .set(self.panel.imp().pending.get().saturating_sub(1));
        if let Some(kind) = &self.kind {
            self.panel
                .imp()
                .pending_custom
                .borrow_mut()
                .retain(|k| k != kind);
        }
    }
}

/// Stable pane identity and widget handles. Closed handles may outlive their widgets.
/// Keep a weak widget reference when checking whether a killed pane was released.
pub struct Pane {
    pub id: u64,
    pub kind: String,
    pub page: gtk::Box,
    pub content: gtk::Box,
    pub header: PaneHeader,
    pub frame: gtk::Widget,
    pub terminal: Option<Terminal>,
    pub view: Option<gtk::Widget>,
    pub gate: Option<ResizeGate>,
    pub(crate) pid: Cell<Option<i32>>,
    pub(crate) closed: Cell<bool>,
    pub(crate) closing: Cell<bool>,
    pub(crate) cancellable: gio::Cancellable,
    shutdown: RefCell<Option<Box<dyn FnOnce()>>>,
    fade: RefCell<Option<Fade>>,
    startup: RefCell<Option<Rc<crate::startup::Startup>>>,
}
impl Pane {
    pub fn pid(&self) -> Option<i32> {
        self.pid.get()
    }
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }
    pub fn is_closing(&self) -> bool {
        self.closing.get()
    }
    pub fn cancellable(&self) -> &gio::Cancellable {
        &self.cancellable
    }
    fn hangup(&self) {
        if let Some(pid) = self.pid.take() {
            hangup_pid(pid);
        }
    }
    fn shutdown(&self) {
        self.cancellable.cancel();
        let startup = self.startup.borrow_mut().take();
        if let Some(s) = startup {
            s.cancel();
        }
        self.hangup();
        let shutdown = self.shutdown.borrow_mut().take();
        if let Some(f) = shutdown {
            f();
        }
    }
    fn cancel_fade(&self) {
        let fade = self.fade.borrow_mut().take();
        if let Some(f) = fade {
            f.cancel();
        }
    }
}
mod imp {
    use super::*;
    pub struct TerminalPanel {
        pub panes: RefCell<Vec<Rc<Pane>>>,
        pub closing: RefCell<Vec<Rc<Pane>>>,
        pub active: Cell<Option<u64>>,
        pub maximized: Cell<Option<u64>>,
        pub next_id: Cell<u64>,
        pub destroyed: Cell<bool>,
        pub stacked: Cell<bool>,
        pub max_panes: Cell<usize>,
        pub pending: Cell<usize>,
        pub pending_custom: RefCell<Vec<String>>,
        pub cwd: RefCell<PathBuf>,
        pub shell: RefCell<String>,
        pub kinds: RefCell<Vec<String>>,
        pub factories: RefCell<BTreeMap<String, Factory>>,
        pub font_size: Cell<i32>,
        pub cursor: Cell<CursorShape>,
        pub blink: Cell<bool>,
        pub scrollback: Cell<i64>,
        pub bell: Cell<bool>,
        pub copy_on_select: Cell<bool>,
        pub palette: RefCell<TerminalColors>,
        pub body: RefCell<Option<PaneSplit>>,
        pub overlay: RefCell<Option<gtk::Overlay>>,
        pub add: RefCell<Option<AddButton>>,
        pub toast: RefCell<Option<LimitToast>>,
        pub animation_ms: Cell<u32>,
        pub resize_gate: Cell<bool>,
        pub initial_extent: Cell<Option<(bool, i32)>>,
        pub commit_pending: Cell<bool>,
        pub commit_clock: RefCell<Option<(gdk::FrameClock, glib::SignalHandlerId)>>,
    }
    impl Default for TerminalPanel {
        fn default() -> Self {
            Self {
                panes: RefCell::new(Vec::new()),
                closing: RefCell::new(Vec::new()),
                active: Cell::new(None),
                maximized: Cell::new(None),
                next_id: Cell::new(1),
                destroyed: Cell::new(false),
                stacked: Cell::new(false),
                max_panes: Cell::new(DEFAULT_MAX_PANES),
                pending: Cell::new(0),
                pending_custom: RefCell::new(Vec::new()),
                cwd: RefCell::new(PathBuf::from("/")),
                shell: RefCell::new(String::new()),
                kinds: RefCell::new(vec!["terminal".into(), "graph".into(), "gitgraph".into()]),
                factories: RefCell::new(BTreeMap::new()),
                font_size: Cell::new(11),
                cursor: Cell::new(CursorShape::Block),
                blink: Cell::new(true),
                scrollback: Cell::new(10000),
                bell: Cell::new(false),
                copy_on_select: Cell::new(false),
                palette: RefCell::new({
                    let mut colors = impro_core::themes::get_theme(Some("dark-plus"))
                        .terminal
                        .clone();
                    colors.cursor.clear();
                    colors
                }),
                body: RefCell::new(None),
                overlay: RefCell::new(None),
                add: RefCell::new(None),
                toast: RefCell::new(None),
                animation_ms: Cell::new(ANIMATION_MS),
                resize_gate: Cell::new(true),
                initial_extent: Cell::new(None),
                commit_pending: Cell::new(false),
                commit_clock: RefCell::new(None),
            }
        }
    }
    #[glib::object_subclass]
    impl ObjectSubclass for TerminalPanel {
        const NAME: &'static str = "ImproTerminalPanel";
        type Type = super::TerminalPanel;
        type ParentType = gtk::Box;
    }
    impl ObjectImpl for TerminalPanel {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
                std::sync::OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("last-closed").build(),
                    glib::subclass::Signal::builder("limit-reached").build(),
                    glib::subclass::Signal::builder("maximize-toggled")
                        .param_types([bool::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("feature-disabled")
                        .param_types([String::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("hide-requested").build(),
                ]
            })
        }
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_orientation(gtk::Orientation::Vertical);
            obj.set_hexpand(true);
            obj.set_vexpand(true);
            obj.style_context().add_class("impro-terminal");
            obj.connect_destroy(|obj| obj.shutdown());
            obj.connect_map(|obj| {
                obj.update_layout();
                obj.commit_layout();
            });
            obj.connect_unmap(|obj| obj.commit_layout());
        }
        fn dispose(&self) {
            self.obj().shutdown();
        }
    }
    impl WidgetImpl for TerminalPanel {}
    impl ContainerImpl for TerminalPanel {}
    impl BoxImpl for TerminalPanel {}
}
glib::wrapper! { pub struct TerminalPanel(ObjectSubclass<imp::TerminalPanel>) @extends gtk::Box,gtk::Container,gtk::Widget,@implements gtk::Buildable,gtk::Orientable; }
impl TerminalPanel {
    /// Construct a lazy dock. `stacked` selects the right dock's vertical layout.
    pub fn new(cwd: impl AsRef<Path>, autostart: bool, stacked: bool, font_size: i32) -> Self {
        let obj: Self = glib::Object::new();
        let i = obj.imp();
        *i.cwd.borrow_mut() = cwd.as_ref().into();
        i.stacked.set(stacked);
        i.font_size.set(font_size);
        let body = PaneSplit::new(
            if stacked {
                gtk::Orientation::Vertical
            } else {
                gtk::Orientation::Horizontal
            },
            if stacked { HEADER_HEIGHT + 90 } else { 160 },
        );
        let overlay = gtk::Overlay::new();
        overlay.add(&body);
        let add = AddButton::new(DEFAULT_MAX_PANES);
        let toast = LimitToast::new(DEFAULT_MAX_PANES);
        toast.widget().set_margin_top(HEADER_HEIGHT + 6);
        toast.widget().set_margin_end(10);
        overlay.add_overlay(add.widget());
        overlay.add_overlay(toast.widget());
        overlay.set_overlay_pass_through(toast.widget(), true);
        obj.pack_start(&overlay, true, true, 0);
        *i.body.borrow_mut() = Some(body.clone());
        *i.overlay.borrow_mut() = Some(overlay.clone());
        *i.add.borrow_mut() = Some(add.clone());
        *i.toast.borrow_mut() = Some(toast);
        let weak = obj.downgrade();
        add.connect_choose(move |kind| {
            if let Some(obj) = weak.upgrade() {
                obj.add_kind(kind);
            }
        });
        overlay.show();
        body.show();
        add.widget().show_all();
        if autostart {
            obj.new_terminal();
        }
        obj
    }
    pub fn body(&self) -> PaneSplit {
        self.imp()
            .body
            .borrow()
            .as_ref()
            .expect("constructed dock")
            .clone()
    }
    pub fn overlay(&self) -> gtk::Overlay {
        self.imp()
            .overlay
            .borrow()
            .as_ref()
            .expect("constructed dock")
            .clone()
    }
    pub fn add_button(&self) -> AddButton {
        self.imp()
            .add
            .borrow()
            .as_ref()
            .expect("constructed dock")
            .clone()
    }
    pub fn toast(&self) -> LimitToast {
        self.imp()
            .toast
            .borrow()
            .as_ref()
            .expect("constructed dock")
            .clone()
    }
    pub fn panes(&self) -> Vec<Rc<Pane>> {
        self.imp().panes.borrow().clone()
    }
    pub fn closing_panes(&self) -> Vec<Rc<Pane>> {
        self.imp().closing.borrow().clone()
    }
    fn all_panes(&self) -> Vec<Rc<Pane>> {
        let mut panes = self.panes();
        panes.extend(self.closing_panes());
        panes
    }
    pub fn stacked(&self) -> bool {
        self.imp().stacked.get()
    }
    pub fn pane_count(&self) -> usize {
        self.imp().panes.borrow().len()
    }
    pub fn terminal_count(&self) -> usize {
        self.count_kind("terminal")
    }
    pub fn graph_count(&self) -> usize {
        self.count_kind("graph")
    }
    pub fn gitgraph_count(&self) -> usize {
        self.count_kind("gitgraph")
    }
    pub fn count_kind(&self, kind: &str) -> usize {
        self.imp()
            .panes
            .borrow()
            .iter()
            .filter(|p| p.kind == kind)
            .count()
    }
    pub fn has_graph(&self) -> bool {
        self.graph_count() > 0
    }
    pub fn has_gitgraph(&self) -> bool {
        self.gitgraph_count() > 0
    }
    pub fn get_views(&self, kind: &str) -> Vec<gtk::Widget> {
        self.imp()
            .panes
            .borrow()
            .iter()
            .filter(|p| p.kind == kind)
            .filter_map(|p| p.view.clone())
            .collect()
    }
    pub fn graph_views(&self) -> Vec<gtk::Widget> {
        self.get_views("graph")
    }
    pub fn gitgraph_views(&self) -> Vec<gtk::Widget> {
        self.get_views("gitgraph")
    }
    pub fn header_labels(&self) -> Vec<String> {
        self.panes().iter().map(|p| p.header.text()).collect()
    }
    pub fn max_panes(&self) -> usize {
        self.imp().max_panes.get()
    }
    pub fn is_full(&self) -> bool {
        self.pane_count() + self.imp().pending.get() >= self.max_panes()
    }
    pub fn set_max_panes(&self, count: usize) {
        let count = count.clamp(1, 8);
        self.imp().max_panes.set(count);
        self.add_button().set_limit(count);
        self.toast()
            .widget()
            .set_text(&format!("Cannot add (limit {count})"));
        self.update_layout();
    }
    pub fn set_pane_kinds(&self, kinds: &[&str]) {
        *self.imp().kinds.borrow_mut() = kinds.iter().map(|k| k.to_string()).collect();
        self.add_button().set_kinds(kinds);
    }
    fn allow_kind(&self, kind: &str) -> bool {
        let allowed = self.imp().kinds.borrow().iter().any(|k| k == kind);
        if !allowed {
            self.emit_by_name::<()>("feature-disabled", &[&kind]);
        }
        allowed
    }
    fn can_add(&self, kind: &str) -> bool {
        if self.imp().destroyed.get() || !self.allow_kind(kind) {
            return false;
        }
        if self.is_full() {
            self.toast().popup();
            self.emit_by_name::<()>("limit-reached", &[]);
            return false;
        }
        true
    }
    pub fn set_factory(&self, kind: &str, f: impl Fn() -> gtk::Widget + 'static) {
        self.imp()
            .factories
            .borrow_mut()
            .insert(kind.into(), Rc::new(f));
    }
    pub fn clear_factory(&self, kind: &str) {
        self.imp().factories.borrow_mut().remove(kind);
    }
    pub fn add_kind(&self, kind: &str) -> bool {
        if kind == "terminal" {
            return self.new_terminal();
        }
        if self.focus_existing_kind(kind) {
            return false;
        }
        if !self.can_add(kind) {
            return false;
        }
        let factory = self.imp().factories.borrow().get(kind).cloned();
        if let Some(factory) = factory {
            let widget = factory();
            self.add_custom_pane(kind, &widget, None)
        } else {
            false
        }
    }
    pub fn add_graph(&self, view: Option<&gtk::Widget>) -> bool {
        if let Some(v) = view {
            self.add_custom_pane("graph", v, None)
        } else {
            self.add_kind("graph")
        }
    }
    pub fn add_gitgraph(&self, view: Option<&gtk::Widget>) -> bool {
        if let Some(v) = view {
            self.add_custom_pane("gitgraph", v, None)
        } else {
            self.add_kind("gitgraph")
        }
    }
    fn focus_existing_kind(&self, kind: &str) -> bool {
        let existing = self
            .imp()
            .panes
            .borrow()
            .iter()
            .find(|p| p.kind == kind)
            .cloned();
        if let Some(p) = existing {
            self.focus_pane(p.id);
            true
        } else {
            false
        }
    }
    /// Add a unique custom kind. Shutdown runs once at close, before exit animation.
    pub fn add_custom_pane(
        &self,
        kind: &str,
        view: &impl IsA<gtk::Widget>,
        shutdown: Option<Box<dyn FnOnce()>>,
    ) -> bool {
        if self.imp().destroyed.get() || !self.allow_kind(kind) {
            return false;
        }
        if self.imp().pending_custom.borrow().iter().any(|k| k == kind) {
            return false;
        }
        if self.focus_existing_kind(kind) {
            return false;
        }
        if !self.can_add(kind) || view.parent().is_some() {
            return false;
        }
        if self.is_maximized() {
            self.restore();
        }
        if self.imp().destroyed.get() || self.is_full() {
            return false;
        }
        let admission = Admission::new(self, Some(kind));
        let header = PaneHeader::new("", kind);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.pack_start(view, true, true, 0);
        let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        page.pack_start(&header.widget, false, false, 0);
        page.pack_start(&content, true, true, 0);
        let pane = Rc::new(Pane {
            id: self.next_id(),
            kind: kind.into(),
            page: page.clone(),
            content: content.clone(),
            header,
            frame: page.upcast(),
            terminal: None,
            view: Some(view.clone().upcast()),
            gate: None,
            pid: Cell::new(None),
            closed: Cell::new(false),
            closing: Cell::new(false),
            cancellable: gio::Cancellable::new(),
            shutdown: RefCell::new(shutdown),
            fade: RefCell::new(None),
            startup: RefCell::new(None),
        });
        let weak = self.downgrade();
        let p = Rc::downgrade(&pane);
        content.connect_local("set-focus-child", false, move |values| {
            let focused = values[1]
                .get::<Option<gtk::Widget>>()
                .ok()
                .flatten()
                .is_some();
            if let (Some(obj), Some(p)) = (weak.upgrade(), p.upgrade()) {
                if !p.is_closed() {
                    p.header.set_focused(focused);
                    if focused {
                        obj.set_active(Some(p.id));
                    }
                }
            }
            None
        });
        self.wire_focus(view, &pane);
        self.insert_pane(pane, admission)
    }
    fn next_id(&self) -> u64 {
        let id = self.imp().next_id.get();
        self.imp().next_id.set(id + 1);
        id
    }
    pub fn new_terminal(&self) -> bool {
        if !self.can_add("terminal") {
            return false;
        }
        if self.is_maximized() {
            self.restore();
        }
        if self.imp().destroyed.get() || self.is_full() {
            return false;
        }
        let admission = Admission::new(self, None);
        let shell = self.imp().shell.borrow().clone();
        let label_shell = if shell.is_empty() {
            std::env::var("SHELL").unwrap_or_else(|_| DEFAULT_SHELL.into())
        } else {
            shell.clone()
        };
        let terminal = Terminal::new();
        self.configure(&terminal);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        content.pack_start(&terminal, true, true, 0);
        let adjustment = terminal.vadjustment().expect("VTE vertical adjustment");
        let scrollbar = gtk::Scrollbar::new(gtk::Orientation::Vertical, Some(&adjustment));
        let gutter = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        gutter.set_size_request(8, -1);
        gutter.set_no_show_all(true);
        scrollbar.set_no_show_all(true);
        gutter.show();
        content.pack_start(&scrollbar, false, false, 0);
        content.pack_start(&gutter, false, false, 0);
        let pending = Rc::new(Cell::new(false));
        for changed in [false, true] {
            let pending = pending.clone();
            let content = content.downgrade();
            let scrollbar = scrollbar.downgrade();
            let gutter = gutter.downgrade();
            let f = move |a: &gtk::Adjustment| {
                if pending.replace(true) {
                    return;
                }
                let pending = pending.clone();
                let c = content.clone();
                let s = scrollbar.clone();
                let g = gutter.clone();
                let a = a.downgrade();
                glib::idle_add_local_once(move || {
                    pending.set(false);
                    if let (Some(c), Some(s), Some(g), Some(a)) =
                        (c.upgrade(), s.upgrade(), g.upgrade(), a.upgrade())
                    {
                        if c.parent().is_some() {
                            let history = a.upper() > a.page_size();
                            s.set_visible(history);
                            g.set_visible(!history);
                        }
                    }
                });
            };
            if changed {
                adjustment.connect_changed(f);
            } else {
                adjustment.connect_value_changed(f);
            }
        }
        let header = PaneHeader::new(
            Path::new(&label_shell)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("bash"),
            "terminal",
        );
        let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        page.pack_start(&header.widget, false, false, 0);
        page.pack_start(&content, true, true, 0);
        let gate = self.imp().resize_gate.get().then(ResizeGate::new);
        let frame = if let Some(g) = &gate {
            g.set_child_minimum(true);
            let background = self.imp().palette.borrow().background.clone();
            let _ = g.set_background(&background);
            g.add(&page);
            g.clone().upcast()
        } else {
            page.clone().upcast()
        };
        let pane = Rc::new(Pane {
            id: self.next_id(),
            kind: "terminal".into(),
            page,
            content,
            header,
            frame,
            terminal: Some(terminal.clone()),
            view: None,
            gate,
            pid: Cell::new(None),
            closed: Cell::new(false),
            closing: Cell::new(false),
            cancellable: gio::Cancellable::new(),
            shutdown: RefCell::new(None),
            fade: RefCell::new(None),
            startup: RefCell::new(None),
        });
        let weak = self.downgrade();
        let p = Rc::downgrade(&pane);
        terminal.connect_child_exited(move |_, _| {
            if let (Some(obj), Some(p)) = (weak.upgrade(), p.upgrade()) {
                p.pid.set(None);
                obj.close_pane(p.id);
            }
        });
        let weak = self.downgrade();
        terminal.connect_key_press_event(move |t, e| {
            weak.upgrade()
                .map(|obj| obj.handle_key(t, e.keyval(), e.state()))
                .unwrap_or(glib::Propagation::Proceed)
        });
        let weak = self.downgrade();
        terminal.connect_button_press_event(move |t, e| {
            if e.button() != 3 {
                return glib::Propagation::Proceed;
            }
            if let Some(obj) = weak.upgrade() {
                t.grab_focus();
                let menu = obj.context_menu(t);
                menu.popup_at_pointer(Some(e));
            }
            glib::Propagation::Stop
        });
        self.wire_focus(&terminal, &pane);
        let inserted = self.insert_pane(pane.clone(), admission);
        if inserted && !pane.is_closed() && !self.imp().destroyed.get() {
            let cwd = self.imp().cwd.borrow().clone();
            let startup = crate::startup::Startup::new(self, &pane, shell, cwd);
            *pane.startup.borrow_mut() = Some(startup.clone());
            startup.begin();
        }
        inserted
    }
    fn wire_focus(&self, widget: &impl IsA<gtk::Widget>, pane: &Rc<Pane>) {
        let weak = self.downgrade();
        let p = Rc::downgrade(pane);
        widget.connect_focus_in_event(move |w, _| {
            if let (Some(obj), Some(p)) = (weak.upgrade(), p.upgrade()) {
                if !p.is_closed() {
                    p.header.set_focused(true);
                    terminal_bar::set_class(w, "focused", true);
                    obj.set_active(Some(p.id));
                }
            }
            glib::Propagation::Proceed
        });
        let p = Rc::downgrade(pane);
        widget.connect_focus_out_event(move |w, _| {
            if let Some(p) = p.upgrade() {
                p.header.set_focused(false);
                terminal_bar::set_class(w, "focused", false);
            }
            glib::Propagation::Proceed
        });
    }
    fn insert_pane(&self, pane: Rc<Pane>, admission: Admission<'_>) -> bool {
        let available = self.imp().kinds.borrow().iter().any(|k| *k == pane.kind);
        if self.imp().destroyed.get()
            || !available
            || self.pane_count() + self.imp().pending.get() > self.max_panes()
        {
            drop(admission);
            pane.closed.set(true);
            pane.shutdown();
            unsafe {
                pane.frame.destroy();
            }
            return false;
        }

        let weak = self.downgrade();
        let id = pane.id;
        pane.header.connect_focus_requested(move || {
            if let Some(obj) = weak.upgrade() {
                obj.focus_pane(id);
            }
        });
        let weak = self.downgrade();
        pane.header.connect_fullscreen_requested(move || {
            if let Some(obj) = weak.upgrade() {
                obj.toggle_maximize_pane(id);
            }
        });
        let weak = self.downgrade();
        pane.header.connect_delete_requested(move || {
            if let Some(obj) = weak.upgrade() {
                obj.close_pane(id);
            }
        });
        let active = self.current_pane();
        let index = {
            let panes = self.imp().panes.borrow();
            active
                .and_then(|p| panes.iter().position(|q| q.id == p.id))
                .map(|i| i + 1)
                .unwrap_or(panes.len())
        };
        let next = self
            .imp()
            .panes
            .borrow()
            .get(index)
            .map(|p| p.frame.clone());
        let children = self.body().children();
        let position = next
            .and_then(|w| children.iter().position(|c| *c == w))
            .unwrap_or(children.len());
        self.imp().panes.borrow_mut().insert(index, pane.clone());
        drop(admission);
        self.body().insert(&pane.frame, position);
        if pane.is_closed() || self.imp().destroyed.get() {
            return false;
        }
        self.set_active(Some(pane.id));
        let animated = self.animated();
        if animated {
            pane.frame.set_opacity(0.0);
        }
        pane.frame.show_all();
        self.update_layout();

        self.commit_layout();
        if animated && !pane.is_closed() {
            let f = terminal_bar::fade(&pane.frame, 0.0, 1.0, self.imp().animation_ms.get(), None);
            *pane.fade.borrow_mut() = Some(f);
        }
        let weak = self.downgrade();
        let id = pane.id;
        glib::idle_add_local_once(move || {
            if let Some(obj) = weak.upgrade() {
                if obj.imp().active.get() == Some(id) {
                    obj.focus_terminal();
                }
            }
        });
        true
    }
    fn animated(&self) -> bool {
        self.imp().animation_ms.get() > 0
            && self.is_mapped()
            && !self.imp().destroyed.get()
            && impro_ui::motion::animations_enabled()
    }
    pub fn set_animation_ms(&self, ms: u32) {
        self.imp().animation_ms.set(ms);
    }
    pub fn set_resize_gate(&self, enabled: bool) {
        self.imp().resize_gate.set(enabled);
    }
    fn update_layout(&self) {
        let panes = self.panes();
        let all = self.all_panes();
        let mut number = 0;
        let shown: Vec<_> = self
            .body()
            .children()
            .into_iter()
            .filter(|w| w.is_visible())
            .collect();
        let corner = if self.stacked() {
            shown.first()
        } else {
            shown.last()
        };
        for p in panes {
            if p.kind == "terminal" {
                number += 1;
            }
            if !p.is_closed() {
                p.header.set_number(number);
            }
        }
        for p in all {
            p.header.set_corner(corner == Some(&p.frame));
        }
        self.body().sync_handles();
        self.add_button().set_full(self.is_full());
    }
    fn set_active(&self, id: Option<u64>) {
        self.imp().active.set(id);
        for p in self.panes() {
            p.header.set_active(Some(p.id) == id);
        }
    }
    fn focused_pane(&self) -> Option<Rc<Pane>> {
        let focus = self
            .toplevel()
            .and_then(|w| w.downcast::<gtk::Window>().ok())
            .and_then(|w| w.focused_widget());
        let focus = focus?;
        self.panes().into_iter().find(|p| {
            p.terminal
                .as_ref()
                .is_some_and(|t| t.upcast_ref::<gtk::Widget>() == &focus)
                || p.view.as_ref() == Some(&focus)
                || focus.is_ancestor(&p.content)
        })
    }
    pub fn current_pane(&self) -> Option<Rc<Pane>> {
        self.focused_pane().or_else(|| {
            let id = self.imp().active.get();
            self.imp()
                .panes
                .borrow()
                .iter()
                .find(|p| Some(p.id) == id)
                .cloned()
        })
    }
    pub fn focus_pane(&self, id: u64) {
        let pane = self
            .imp()
            .panes
            .borrow()
            .iter()
            .find(|p| p.id == id)
            .cloned();
        if let Some(p) = pane {
            if self.is_maximized() && self.imp().maximized.get() != Some(id) {
                self.toggle_maximize_pane(id);
            }
            if !p.is_closed() {
                self.set_active(Some(id));
                self.focus_terminal();
            }
        }
    }
    pub fn focus_terminal(&self) {
        if self.imp().destroyed.get() {
            return;
        }
        let id = self.imp().active.get();
        let pane = self
            .imp()
            .panes
            .borrow()
            .iter()
            .find(|p| Some(p.id) == id)
            .cloned();
        if let Some(p) = pane {
            if let Some(t) = &p.terminal {
                t.grab_focus();
            } else if let Some(v) = &p.view {
                v.grab_focus();
            }
        }
    }
    pub fn close_current(&self) {
        if let Some(p) = self.current_pane() {
            self.close_pane(p.id);
        }
    }
    pub fn close_pane(&self, id: u64) {
        let refocus = self.focused_pane().is_some_and(|p| p.id == id);
        let pane = {
            let mut panes = self.imp().panes.borrow_mut();
            let Some(index) = panes.iter().position(|p| p.id == id) else {
                return;
            };
            let pane = panes.remove(index);
            pane.closed.set(true);
            if self.imp().active.get() == Some(id) {
                self.imp().active.set(
                    panes
                        .get(index.min(panes.len().saturating_sub(1)))
                        .map(|p| p.id),
                );
            }
            pane
        };
        // Release the model borrow before cancellation, shutdown or any GTK signal.
        let was_maximized = self.imp().maximized.get() == Some(id);
        if was_maximized {
            self.restore_with_focus(false);
        }
        pane.shutdown();
        pane.cancel_fade();
        if self.imp().destroyed.get() {
            return;
        }
        self.set_active(self.imp().active.get());
        if self.animated() {
            pane.closing.set(true);
            self.imp().closing.borrow_mut().push(pane.clone());
            self.update_layout();
            let weak = self.downgrade();
            let f = terminal_bar::fade(
                &pane.frame,
                pane.frame.opacity(),
                0.0,
                self.imp().animation_ms.get(),
                Some(Box::new(move || {
                    if let Some(obj) = weak.upgrade() {
                        obj.finish_close(id);
                    }
                })),
            );
            if pane.is_closing() {
                *pane.fade.borrow_mut() = Some(f);
            }
        } else {
            self.finish_close_widget(&pane);
        }
        if refocus && self.is_visible() {
            let weak = self.downgrade();
            glib::idle_add_local_once(move || {
                if let Some(obj) = weak.upgrade() {
                    obj.focus_terminal();
                }
            });
        }
    }
    fn finish_close(&self, id: u64) {
        let p = {
            let mut closing = self.imp().closing.borrow_mut();
            closing
                .iter()
                .position(|p| p.id == id)
                .map(|i| closing.remove(i))
        };
        if let Some(p) = p {
            self.finish_close_widget(&p);
        }
    }
    fn finish_close_widget(&self, pane: &Pane) {
        pane.closing.set(false);
        pane.cancel_fade();
        if pane.frame.parent().is_some() {
            self.body().remove(&pane.frame);
        }
        unsafe {
            pane.frame.destroy();
        }
        if self.imp().destroyed.get() {
            return;
        }
        self.update_layout();
        self.commit_layout();
        if self.pane_count() == 0 && self.imp().closing.borrow().is_empty() {
            self.emit_by_name::<()>("last-closed", &[]);
        }
    }
    pub fn close_kind(&self, kind: &str) {
        for p in self.panes() {
            if p.kind == kind {
                self.close_pane(p.id);
            }
        }
        for p in self.closing_panes() {
            if p.kind == kind {
                self.finish_close(p.id);
            }
        }
    }
    pub fn is_maximized(&self) -> bool {
        self.imp().maximized.get().is_some()
    }
    pub fn toggle_maximize(&self) {
        if let Some(p) = self.current_pane() {
            self.toggle_maximize_pane(p.id);
        }
    }
    pub fn toggle_maximize_pane(&self, id: u64) {
        if self.imp().destroyed.get() {
            return;
        }
        let pane = self
            .imp()
            .panes
            .borrow()
            .iter()
            .find(|p| p.id == id)
            .cloned();
        let Some(pane) = pane else { return };
        if self.imp().maximized.get() == Some(id) {
            self.restore();
            return;
        }
        let was_normal = self.imp().maximized.replace(Some(id)).is_none();
        for p in self.all_panes() {
            p.frame.set_no_show_all(p.id != id);
            p.frame.set_visible(p.id == id);
            p.header.set_maximized(p.id == id);
        }
        if pane.is_closed() {
            return;
        }
        self.set_active(Some(id));
        self.update_layout();
        self.focus_terminal();
        self.commit_layout();
        if was_normal {
            self.emit_by_name::<()>("maximize-toggled", &[&true]);
        }
    }
    pub fn restore(&self) {
        self.restore_with_focus(true);
    }
    fn restore_with_focus(&self, focus: bool) {
        if self.imp().maximized.take().is_none() || self.imp().destroyed.get() {
            return;
        }
        for p in self.panes() {
            p.frame.set_no_show_all(false);
            p.frame.show();
            p.header.set_maximized(false);
        }
        self.update_layout();
        if focus {
            self.focus_terminal();
        }
        self.emit_by_name::<()>("maximize-toggled", &[&false]);
        self.commit_layout();
    }
    pub fn hide_dock(&self) {
        self.set_no_show_all(true);
        self.hide();
    }
    pub fn show_dock(&self) {
        if self.imp().destroyed.get() {
            return;
        }
        self.set_no_show_all(false);
        self.show_all();
        if self.pane_count() == 0 {
            self.new_terminal();
        }
        self.focus_terminal();
    }
    pub fn request_hide(&self) {
        self.emit_by_name::<()>("hide-requested", &[]);
    }
    pub fn set_initial_dock_extent(&self, right: bool, extent: i32) {
        self.imp().initial_extent.set(Some((right, extent.max(0))));
    }
    pub(crate) fn initial_extent_ready(&self) -> bool {
        let Some((right, extent)) = self.imp().initial_extent.get() else {
            return true;
        };
        let actual = if right {
            self.allocated_width()
        } else {
            self.allocated_height()
        };
        if actual == extent {
            self.imp().initial_extent.set(None);
            true
        } else {
            false
        }
    }
    pub fn set_cwd(&self, path: impl AsRef<Path>) {
        *self.imp().cwd.borrow_mut() = path.as_ref().into();
    }
    pub fn cwd(&self) -> PathBuf {
        self.imp().cwd.borrow().clone()
    }
    pub fn set_shell(&self, shell: &str) {
        *self.imp().shell.borrow_mut() = shell.into();
    }
    pub fn set_copy_on_select(&self, value: bool) {
        self.imp().copy_on_select.set(value);
    }
    pub fn set_bell(&self, value: bool) {
        self.imp().bell.set(value);
        for p in self.all_panes() {
            if let Some(t) = &p.terminal {
                t.set_audible_bell(value);
            }
        }
    }
    pub fn set_font_size(&self, size: i32) {
        self.imp().font_size.set(size);
        for p in self.all_panes() {
            if let Some(t) = &p.terminal {
                let rewrap = t.rewrap_on_resize();
                if p.gate.is_some() {
                    t.set_rewrap_on_resize(false);
                }
                t.set_font(&pango::FontDescription::from_string(&format!(
                    "Monospace {size}"
                )));
                t.set_rewrap_on_resize(rewrap);
                p.frame.queue_resize();
            }
        }
        self.commit_layout();
    }
    pub fn set_cursor_shape(&self, shape: &str) -> Result<(), String> {
        let shape = match shape {
            "block" => CursorShape::Block,
            "ibeam" => CursorShape::Ibeam,
            "underline" => CursorShape::Underline,
            _ => return Err(format!("unknown cursor shape: {shape:?}")),
        };
        self.imp().cursor.set(shape);
        for p in self.all_panes() {
            if let Some(t) = &p.terminal {
                t.set_cursor_shape(shape);
            }
        }
        Ok(())
    }
    pub fn set_cursor_blink(&self, value: bool) {
        self.imp().blink.set(value);
        for p in self.all_panes() {
            if let Some(t) = &p.terminal {
                t.set_cursor_blink_mode(if value {
                    CursorBlinkMode::On
                } else {
                    CursorBlinkMode::Off
                });
            }
        }
    }
    pub fn set_scrollback(&self, lines: i64) {
        let lines = lines.clamp(SCROLLBACK_RANGE.0, SCROLLBACK_RANGE.1);
        self.imp().scrollback.set(lines);
        for p in self.all_panes() {
            if let Some(t) = &p.terminal {
                t.set_scrollback_lines(lines as _);
            }
        }
    }
    pub fn apply_palette(
        &self,
        foreground: &str,
        background: &str,
        ansi: &[String],
        cursor: Option<&str>,
    ) -> Result<(), String> {
        if ansi.len() != 16 {
            return Err("Expected 16 ANSI colours".into());
        }
        for color in std::iter::once(foreground)
            .chain(std::iter::once(background))
            .chain(ansi.iter().map(String::as_str))
            .chain(cursor)
        {
            gdk::RGBA::parse(color).map_err(|_| format!("Invalid colour: {color}"))?;
        }
        *self.imp().palette.borrow_mut() = TerminalColors {
            foreground: foreground.into(),
            background: background.into(),
            ansi: std::array::from_fn(|i| ansi[i].clone()),
            cursor: cursor.unwrap_or(foreground).into(),
        };
        for p in self.all_panes() {
            if let Some(t) = &p.terminal {
                self.apply_colors(t);
            }
            if let Some(g) = &p.gate {
                let _ = g.set_background(background);
            }
        }
        Ok(())
    }
    pub fn apply_theme(&self, theme: &Theme) {
        let p = &theme.terminal;
        let _ = self.apply_palette(&p.foreground, &p.background, &p.ansi, Some(&p.cursor));
    }
    /// Apply an already-loaded settings snapshot; this method performs no file IO.
    pub fn apply_settings(&self, values: &serde_json::Map<String, serde_json::Value>) {
        if let Some(v) = values.get("terminal_max_panes").and_then(|v| v.as_u64()) {
            self.set_max_panes(v as usize);
        }
        if let Some(v) = values.get("terminal_font_size").and_then(|v| v.as_i64()) {
            self.set_font_size(v as i32);
        }
        if let Some(v) = values.get("terminal_shell").and_then(|v| v.as_str()) {
            self.set_shell(v);
        }
        if let Some(v) = values.get("terminal_cursor").and_then(|v| v.as_str()) {
            let _ = self.set_cursor_shape(v);
        }
        if let Some(v) = values.get("terminal_scrollback").and_then(|v| v.as_i64()) {
            self.set_scrollback(v);
        }
        for (key, method) in [
            (
                "terminal_copy_on_select",
                Self::set_copy_on_select as fn(&Self, bool),
            ),
            ("terminal_bell", Self::set_bell),
            ("terminal_blink", Self::set_cursor_blink),
        ] {
            if let Some(v) = values.get(key).and_then(|v| v.as_bool()) {
                method(self, v);
            }
        }
    }
    fn apply_colors(&self, t: &Terminal) {
        let p = self.imp().palette.borrow().clone();
        let fg = gdk::RGBA::parse(&p.foreground).ok();
        let bg = gdk::RGBA::parse(&p.background).ok();
        let ansi: Vec<_> = p
            .ansi
            .iter()
            .filter_map(|c| gdk::RGBA::parse(c).ok())
            .collect();
        t.set_colors(fg.as_ref(), bg.as_ref(), &ansi);
        t.set_color_cursor(gdk::RGBA::parse(&p.cursor).ok().as_ref());
    }
    fn configure(&self, t: &Terminal) {
        t.style_context().add_class("impro-vte");
        for set in [
            Terminal::set_margin_start,
            Terminal::set_margin_end,
            Terminal::set_margin_top,
            Terminal::set_margin_bottom,
        ] {
            set(t, 8);
        }
        t.set_font(&pango::FontDescription::from_string(&format!(
            "Monospace {}",
            self.imp().font_size.get()
        )));
        self.apply_colors(t);
        t.set_cursor_shape(self.imp().cursor.get());
        t.set_cursor_blink_mode(if self.imp().blink.get() {
            CursorBlinkMode::On
        } else {
            CursorBlinkMode::Off
        });
        t.set_scrollback_lines(self.imp().scrollback.get() as _);
        t.set_scroll_on_output(false);
        t.set_scroll_on_keystroke(true);
        t.set_audible_bell(self.imp().bell.get());
        t.set_mouse_autohide(true);
        t.set_hexpand(true);
        t.set_vexpand(true);
        t.set_size(20, 4);
        let weak = self.downgrade();
        t.connect_selection_changed(move |t| {
            if weak.upgrade().is_some_and(|p| p.imp().copy_on_select.get()) && t.has_selection() {
                t.copy_clipboard_format(Format::Text);
            }
        });
    }
    pub fn handle_key(
        &self,
        t: &Terminal,
        key: gdk::keys::Key,
        state: gdk::ModifierType,
    ) -> glib::Propagation {
        let state = state & gtk::accelerator_get_default_mod_mask();
        let name = key.name().map(|s| s.to_lowercase()).unwrap_or_default();
        let ctrl = gdk::ModifierType::CONTROL_MASK;
        let shift = gdk::ModifierType::SHIFT_MASK;
        if state == ctrl && name == "s" {
            t.feed_child(&[0x13]);
            return glib::Propagation::Stop;
        }
        if state == ctrl | gdk::ModifierType::MOD1_MASK
            && (["left", "right"].contains(&name.as_str())
                || (self.stacked() && ["up", "down"].contains(&name.as_str())))
        {
            let panes = self.panes();
            if let Some(index) = panes.iter().position(|p| p.terminal.as_ref() == Some(t)) {
                let step = if ["right", "down"].contains(&name.as_str()) {
                    1
                } else {
                    panes.len() - 1
                };
                self.focus_pane(panes[(index + step) % panes.len()].id);
            }
            return glib::Propagation::Stop;
        }
        if state != ctrl | shift {
            return glib::Propagation::Proceed;
        }
        match name.as_str() {
            "c" => t.copy_clipboard_format(Format::Text),
            "v" => t.paste_clipboard(),
            "t" | "5" | "percent" => {
                self.new_terminal();
            }
            "w" => self.close_current(),
            "plus" | "equal" | "minus" | "underscore" | "0" | "parenright" | "kp_add"
            | "kp_subtract" | "kp_0" => {
                let scale = if ["0", "parenright", "kp_0"].contains(&name.as_str()) {
                    1.0
                } else {
                    t.font_scale()
                        + if ["plus", "equal", "kp_add"].contains(&name.as_str()) {
                            0.1
                        } else {
                            -0.1
                        }
                };
                let rewrap = t.rewrap_on_resize();
                t.set_rewrap_on_resize(false);
                t.set_font_scale(scale.clamp(0.5, 3.0));
                t.set_rewrap_on_resize(rewrap);
                self.commit_layout();
            }
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    }
    pub fn context_menu(&self, t: &Terminal) -> gtk::Menu {
        let menu = gtk::Menu::new();
        for label in [
            "Copy",
            "Paste",
            "Select All",
            "Clear",
            "",
            "Split Terminal",
            "Kill Terminal",
        ] {
            if label.is_empty() {
                menu.append(&gtk::SeparatorMenuItem::new());
                continue;
            }
            let item = gtk::MenuItem::with_label(label);
            if label == "Copy" {
                item.set_sensitive(t.has_selection());
            }
            let weak = self.downgrade();
            let t = t.downgrade();
            item.connect_activate(move |_| {
                if let (Some(obj), Some(t)) = (weak.upgrade(), t.upgrade()) {
                    match label {
                        "Copy" => t.copy_clipboard_format(Format::Text),
                        "Paste" => t.paste_clipboard(),
                        "Select All" => t.select_all(),
                        "Clear" => t.reset(true, true),
                        "Split Terminal" => {
                            obj.new_terminal();
                        }
                        "Kill Terminal" => obj.close_current(),
                        _ => {}
                    }
                }
            });
            menu.append(&item);
        }
        menu.set_attach_widget(Some(t));
        menu.connect_selection_done(|m| unsafe {
            m.destroy();
        });
        menu.show_all();
        menu
    }
    /// Coalesce discrete layout commits after paint; pointer drags use gate pacing.
    pub fn commit_layout(&self) {
        if self.imp().destroyed.get() || self.imp().commit_pending.replace(true) {
            return;
        }
        let Some(clock) = self.frame_clock() else {
            self.imp().commit_pending.set(false);
            return;
        };
        let weak = self.downgrade();
        let handler = clock.connect_after_paint(move |_| {
            if let Some(obj) = weak.upgrade() {
                let old = obj.imp().commit_clock.borrow_mut().take();
                if let Some((c, h)) = old {
                    crate::disconnect(&c, h);
                }
                obj.imp().commit_pending.set(false);
                if let Some(top) = obj.toplevel() {
                    fn walk(w: &gtk::Widget) {
                        if let Some(g) = w.downcast_ref::<ResizeGate>() {
                            if g.is_mapped() {
                                g.commit_when_answered();
                            }
                        } else if let Some(c) = w.downcast_ref::<gtk::Container>() {
                            for child in c.children() {
                                walk(&child);
                            }
                        }
                    }
                    walk(&top);
                }
            }
        });
        *self.imp().commit_clock.borrow_mut() = Some((clock, handler));
        self.queue_resize();
    }
    /// Cancel spawn/resize/motion work, shut down custom panes and hang up children.
    /// GTK destroy invokes this automatically; repeated calls are harmless.
    pub fn shutdown(&self) {
        if self.imp().destroyed.replace(true) {
            return;
        }
        let panes = std::mem::take(&mut *self.imp().panes.borrow_mut());
        let closing = std::mem::take(&mut *self.imp().closing.borrow_mut());
        self.imp().active.set(None);
        self.imp().maximized.set(None);
        let clock = self.imp().commit_clock.borrow_mut().take();
        if let Some((c, h)) = clock {
            crate::disconnect(&c, h);
        }
        self.imp().commit_pending.set(false);
        let factories = std::mem::take(&mut *self.imp().factories.borrow_mut());
        drop(factories);
        for p in panes.into_iter().chain(closing) {
            p.closed.set(true);
            p.shutdown();
            p.cancel_fade();
        }
    }
    pub fn connect_last_closed(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_local("last-closed", false, move |v| {
            if let Ok(p) = v[0].get::<Self>() {
                f(&p);
            }
            None
        })
    }
    pub fn connect_limit_reached(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_local("limit-reached", false, move |v| {
            if let Ok(p) = v[0].get::<Self>() {
                f(&p);
            }
            None
        })
    }
    pub fn connect_maximize_toggled(
        &self,
        f: impl Fn(&Self, bool) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_local("maximize-toggled", false, move |v| {
            if let (Ok(p), Ok(b)) = (v[0].get::<Self>(), v[1].get::<bool>()) {
                f(&p, b);
            }
            None
        })
    }
    pub fn connect_feature_disabled(
        &self,
        f: impl Fn(&Self, &str) + 'static,
    ) -> glib::SignalHandlerId {
        self.connect_local("feature-disabled", false, move |v| {
            if let (Ok(p), Ok(k)) = (v[0].get::<Self>(), v[1].get::<String>()) {
                f(&p, &k);
            }
            None
        })
    }
    pub fn connect_hide_requested(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_local("hide-requested", false, move |v| {
            if let Ok(p) = v[0].get::<Self>() {
                f(&p);
            }
            None
        })
    }
}
