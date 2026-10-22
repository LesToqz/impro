//! GTK/WebKit hosts. All Git commands, lexing and large JSON encoding run in workers.
use crate::{assets, diff, log, palette};
use gio::prelude::*;
use gtk::prelude::*;
use impro_core::{
    settings,
    themes::{Theme, UiColors},
};
use javascriptcore::ValueExt;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use webkit2gtk::{
    CacheModel, HardwareAccelerationPolicy, LoadEvent, NavigationPolicyDecision,
    NavigationPolicyDecisionExt, PolicyDecisionExt, PolicyDecisionType, SettingsExt, URIRequestExt,
    UserContentManager, UserContentManagerExt, WebContext, WebContextExt, WebView, WebViewExt,
};

/// Syntax adapters must be safe to use from Git workers. No editor dependency is required.
pub type SyntaxAdapter = Arc<dyn diff::Highlighter + Send + Sync>;
type TextCallback = Rc<dyn Fn(&str)>;
type FileCallback = Rc<dyn Fn(&Path, i32)>;
type VoidCallback = Rc<dyn Fn()>;

/// Validated settings shared by all graph panes. Keys match the settings dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphSettings {
    pub orientation: String,
    pub max_commits: i64,
    pub show_remotes: bool,
    pub show_tags: bool,
    pub time_format: String,
}
impl Default for GraphSettings {
    fn default() -> Self {
        Self {
            orientation: "horizontal".into(),
            max_commits: 1500,
            show_remotes: true,
            show_tags: true,
            time_format: "relative".into(),
        }
    }
}
impl GraphSettings {
    /// Accept either the raw Git graph blob or the settings dialog's flat values.
    pub fn from_value(value: &Value) -> Self {
        let mut s = Self::default();
        let get = |k: &str| {
            value
                .get(format!("gitgraph_{k}"))
                .or_else(|| value.get(k))
                .unwrap_or(&Value::Null)
        };
        if get("orientation") == "vertical" {
            s.orientation = "vertical".into();
        }
        s.max_commits = get("max_commits")
            .as_i64()
            .unwrap_or(1500)
            .clamp(100, 10000);
        s.show_remotes = get("show_remotes").as_bool().unwrap_or(true);
        s.show_tags = get("show_tags").as_bool().unwrap_or(true);
        if get("time_format") == "absolute" {
            s.time_format = "absolute".into();
        }
        s
    }
    pub fn to_value(&self) -> Value {
        json!({"orientation":self.orientation,"max_commits":self.max_commits,"show_remotes":self.show_remotes,"show_tags":self.show_tags,"time_format":self.time_format})
    }
    /// Persist opaque settings on a worker; the callback reports whether saving succeeded.
    pub fn save(&self, done: impl FnOnce(bool) + 'static) {
        let value = self.to_value();
        #[allow(deprecated)]
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        let mut done = Some(done);
        rx.attach(None, move |ok| {
            if let Some(done) = done.take() {
                done(ok);
            }
            glib::ControlFlow::Break
        });
        std::thread::spawn(move || {
            let _ = tx.send(settings::set_gitgraph(&value));
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Graph,
    Commit,
}
struct Update {
    generation: u64,
    data: Arc<Value>,
    script: String,
    watches: Vec<(PathBuf, bool)>,
    context: bool,
}
struct ContextRequest {
    rev: String,
    path: String,
    start: i64,
    end: i64,
}
struct Inner {
    widget: gtk::Box,
    web: WebView,
    manager: UserContentManager,
    api: &'static str,
    kind: Kind,
    root: RefCell<PathBuf>,
    rev: String,
    highlighter: Option<SyntaxAdapter>,
    closed: Cell<bool>,
    ready: Cell<bool>,
    generation: Cell<u64>,
    running: Cell<bool>,
    dirty: Cell<bool>,
    queue: RefCell<Vec<String>>,
    last: RefCell<Option<Arc<Value>>>,
    last_script: RefCell<Option<String>>,
    graph_settings: RefCell<GraphSettings>,
    selection: RefCell<Option<String>>,
    focus: Cell<bool>,
    theme: RefCell<Option<Value>>,
    reduce_motion: Cell<bool>,
    monitors: RefCell<Vec<gio::FileMonitor>>,
    debounce: RefCell<Option<glib::SourceId>>,
    receiver: RefCell<Option<glib::SourceId>>,
    sender: glib::Sender<Update>,
    cancelled: Arc<AtomicBool>,
    cancellable: gio::Cancellable,
    message_handler: RefCell<Option<glib::SignalHandlerId>>,
    focus_window: RefCell<Option<(glib::WeakRef<gtk::Window>, glib::SignalHandlerId)>>,
    contexts: RefCell<VecDeque<ContextRequest>>,
    context_running: Cell<bool>,
    open_commit: RefCell<Vec<TextCallback>>,
    errors: RefCell<Vec<TextCallback>>,
    orientation_changed: RefCell<Vec<TextCallback>>,
    open_file: RefCell<Vec<FileCallback>>,
    root_changed: RefCell<Vec<VoidCallback>>,
}
thread_local! {static COMMITS:RefCell<Vec<Weak<Inner>>>=const {RefCell::new(Vec::new())};}

impl Inner {
    fn new(root: &Path, rev: &str, kind: Kind, highlighter: Option<SyntaxAdapter>) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        let manager = UserContentManager::new();
        let prefs = webkit2gtk::Settings::new();
        prefs.set_enable_developer_extras(false);
        prefs.set_enable_write_console_messages_to_stdout(false);
        prefs.set_javascript_can_open_windows_automatically(false);
        prefs.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Never);
        prefs.set_enable_page_cache(false);
        prefs.set_enable_html5_database(false);
        prefs.set_enable_html5_local_storage(false);
        prefs.set_enable_dns_prefetching(false);
        prefs.set_enable_webgl(false);
        prefs.set_enable_webaudio(false);
        prefs.set_enable_media(false);
        prefs.set_enable_media_stream(false);
        let related = if kind == Kind::Commit {
            COMMITS.with(|views| {
                views
                    .borrow()
                    .iter()
                    .filter_map(Weak::upgrade)
                    .find(|v| !v.closed.get())
            })
        } else {
            None
        };
        let web = if let Some(other) = related {
            WebView::builder()
                .related_view(&other.web)
                .settings(&prefs)
                .user_content_manager(&manager)
                .build()
        } else {
            let context = WebContext::new_ephemeral();
            context.set_cache_model(CacheModel::DocumentViewer);
            WebView::builder()
                .web_context(&context)
                .settings(&prefs)
                .user_content_manager(&manager)
                .build()
        };
        web.set_hexpand(true);
        web.set_vexpand(true);
        widget.pack_start(&web, true, true, 0);
        #[allow(deprecated)]
        let (sender, receiver) = glib::MainContext::channel(glib::Priority::DEFAULT);
        let this = Rc::new(Self {
            widget,
            web,
            manager,
            api: if kind == Kind::Graph {
                "ImproGit"
            } else {
                "ImproCommit"
            },
            kind,
            root: RefCell::new(root.to_path_buf()),
            rev: rev.into(),
            highlighter,
            closed: Cell::new(false),
            ready: Cell::new(false),
            generation: Cell::new(0),
            running: Cell::new(false),
            dirty: Cell::new(true),
            queue: RefCell::new(Vec::new()),
            last: RefCell::new(None),
            last_script: RefCell::new(None),
            graph_settings: RefCell::new(GraphSettings::default()),
            selection: RefCell::new(None),
            focus: Cell::new(false),
            theme: RefCell::new(None),
            reduce_motion: Cell::new(false),
            monitors: RefCell::new(Vec::new()),
            debounce: RefCell::new(None),
            receiver: RefCell::new(None),
            sender,
            cancelled: Arc::new(AtomicBool::new(false)),
            cancellable: gio::Cancellable::new(),
            message_handler: RefCell::new(None),
            focus_window: RefCell::new(None),
            contexts: RefCell::new(VecDeque::new()),
            context_running: Cell::new(false),
            open_commit: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
            orientation_changed: RefCell::new(Vec::new()),
            open_file: RefCell::new(Vec::new()),
            root_changed: RefCell::new(Vec::new()),
        });
        let weak = Rc::downgrade(&this);
        let source = receiver.attach(None, move |update| {
            if let Some(s) = weak.upgrade() {
                if !s.closed.get() {
                    s.deliver(update);
                    return glib::ControlFlow::Continue;
                }
            }
            glib::ControlFlow::Break
        });
        *this.receiver.borrow_mut() = Some(source);
        this.connect();
        if kind == Kind::Commit {
            COMMITS.with(|v| {
                let mut v = v.borrow_mut();
                v.retain(|w| w.strong_count() > 0);
                v.push(Rc::downgrade(&this));
            });
        }
        let weak = Rc::downgrade(&this);
        let handler = this
            .manager
            .connect_script_message_received(Some("impro"), move |_, r| {
                if let (Some(s), Some(v)) = (weak.upgrade(), r.js_value()) {
                    if let Some(raw) = v.to_json(0) {
                        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
                            s.message(v);
                        }
                    }
                }
            });
        *this.message_handler.borrow_mut() = Some(handler);
        this.manager.register_script_message_handler("impro");
        this.state("loading", "");
        this.call("pause", &[]);
        this.web.load_html(
            &if kind == Kind::Graph {
                assets::graph_html()
            } else {
                assets::commit_html()
            },
            Some("about:blank"),
        );
        this.web.show();
        if kind == Kind::Commit {
            this.refresh();
        }
        this
    }
    fn connect(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.widget.connect_destroy(move |_| {
            if let Some(s) = weak.upgrade() {
                s.shutdown();
            }
        });
        let weak = Rc::downgrade(self);
        self.widget.connect_map(move |_| {
            if let Some(s) = weak.upgrade() {
                s.call("resume", &[]);
                s.watch_focus();
                if s.dirty.get() {
                    s.start();
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.widget.connect_unmap(move |_| {
            if let Some(s) = weak.upgrade() {
                s.call("pause", &[]);
            }
        });
        let weak = Rc::downgrade(self);
        self.widget.connect_hierarchy_changed(move |_, _| {
            if let Some(s) = weak.upgrade() {
                s.watch_focus();
            }
        });
        self.web.connect_context_menu(|_, _, _, _| true);
        self.web.connect_create(|_, _| None);
        self.web.connect_decide_policy(|_, decision, kind| {
            if kind == PolicyDecisionType::NewWindowAction {
                decision.ignore();
                return true;
            }
            if kind == PolicyDecisionType::NavigationAction {
                if let Some(nav) = decision.dynamic_cast_ref::<NavigationPolicyDecision>() {
                    if nav
                        .navigation_action()
                        .and_then(|a| a.request())
                        .and_then(|r| r.uri())
                        .is_some_and(|u| u != "about:blank")
                    {
                        decision.ignore();
                        return true;
                    }
                }
            }
            false
        });
        let weak = Rc::downgrade(self);
        self.web.connect_load_changed(move |_, event| {
            if event == LoadEvent::Started {
                if let Some(s) = weak.upgrade() {
                    if s.ready.replace(false) {
                        s.queue.borrow_mut().clear();
                        let theme = s.theme.borrow().clone();
                        if let Some(theme) = theme {
                            s.call("setTheme", &[theme]);
                        }
                        s.settings();
                        let data = s.last_script.borrow().clone();
                        if let Some(script) = data {
                            s.post_script(script);
                        } else {
                            s.state("loading", "");
                        }
                    }
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.web.connect_load_failed(move |_, _, _, error| {
            if let Some(s) = weak.upgrade() {
                s.error(error.message());
            }
            false
        });
        let weak = Rc::downgrade(self);
        self.web.connect_web_process_terminated(move |_, reason| {
            if let Some(s) = weak.upgrade() {
                s.error(&format!("Git web process stopped: {reason:?}"));
            }
        });
    }
    fn error(&self, text: &str) {
        if !self.closed.get() {
            let callbacks = self.errors.borrow().clone();
            for cb in callbacks {
                cb(text);
            }
        }
    }
    fn evaluate(self: &Rc<Self>, script: &str) {
        if self.closed.get() {
            return;
        }
        let weak = Rc::downgrade(self);
        self.web
            .evaluate_javascript(script, None, None, Some(&self.cancellable), move |r| {
                if let (Some(s), Err(e)) = (weak.upgrade(), r) {
                    s.error(e.message());
                }
            });
    }
    fn post_script(self: &Rc<Self>, script: String) {
        if self.closed.get() {
            return;
        }
        if self.ready.get() {
            self.evaluate(&script);
        } else {
            self.queue.borrow_mut().push(script);
        }
    }
    fn command(&self, cmd: &str, args: &[Value]) -> String {
        format!(
            "window.{}.{}({});",
            self.api,
            cmd,
            args.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
    fn call(self: &Rc<Self>, cmd: &str, args: &[Value]) {
        let mut script = self.command(cmd, args);
        if self.kind == Kind::Commit && matches!(cmd, "pause" | "resume") {
            script = format!(
                "if(typeof window.{}.{cmd}==='function'){{{script}}}",
                self.api
            );
        }
        self.post_script(script);
    }
    fn state(self: &Rc<Self>, kind: &str, message: &str) {
        self.call("setState", &[json!(kind), json!(message)]);
    }
    fn settings(self: &Rc<Self>) {
        self.call(
            "setSettings",
            &[json!({"reduceMotion":self.reduce_motion.get()})],
        );
        if self.kind == Kind::Graph {
            let settings = self.graph_settings.borrow().clone();
            self.call("setOrientation", &[json!(settings.orientation)]);
            self.call("setOptions",&[json!({"showRemotes":settings.show_remotes,"showTags":settings.show_tags,"timeFormat":settings.time_format})]);
        }
    }
    fn message(self: &Rc<Self>, v: Value) {
        if self.closed.get() || !v.is_object() {
            return;
        }
        match v["type"].as_str().unwrap_or("") {
            "ready" => {
                if !self.ready.replace(true) {
                    let queue = std::mem::take(&mut *self.queue.borrow_mut());
                    self.evaluate(&queue.join("\n"));
                    self.call(
                        if self.widget.is_mapped() {
                            "resume"
                        } else {
                            "pause"
                        },
                        &[],
                    );
                }
            }
            "open-commit" => {
                if let Some(hash) = v["hash"].as_str().filter(|h| !h.is_empty()) {
                    let callbacks = self.open_commit.borrow().clone();
                    for cb in callbacks {
                        cb(hash);
                    }
                }
            }
            "error" => self.error(v["message"].as_str().unwrap_or("Git page error")),
            "refresh" if self.kind == Kind::Graph => self.refresh(),
            "orientation" if self.kind == Kind::Graph => {
                if let Some(value) = v["value"]
                    .as_str()
                    .filter(|s| matches!(*s, "horizontal" | "vertical"))
                {
                    let changed = {
                        let mut settings = self.graph_settings.borrow_mut();
                        let changed = settings.orientation != value;
                        settings.orientation = value.into();
                        changed
                    };
                    if changed {
                        let callbacks = self.orientation_changed.borrow().clone();
                        for cb in callbacks {
                            cb(value);
                        }
                    }
                }
            }
            "copy" if self.kind == Kind::Commit => {
                if let Some(text) = v["text"].as_str() {
                    gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(text);
                }
            }
            "open-file" if self.kind == Kind::Commit => self.file_request(v),
            "expand" if self.kind == Kind::Commit => self.expand(v),
            _ => {}
        }
    }
    fn refresh(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        self.generation.set(self.generation.get().wrapping_add(1));
        self.dirty.set(true);
        if self.kind == Kind::Commit {
            self.last.borrow_mut().take();
            self.last_script.borrow_mut().take();
            self.contexts.borrow_mut().clear();
            self.state("loading", "");
        }
        if self.kind == Kind::Commit || self.widget.is_mapped() {
            self.start();
        }
    }
    fn start(self: &Rc<Self>) {
        if self.closed.get() || self.running.replace(true) {
            return;
        }
        self.dirty.set(false);
        let generation = self.generation.get();
        let root = self.root.borrow().clone();
        let rev = self.rev.clone();
        let limit = self.graph_settings.borrow().max_commits;
        let kind = self.kind;
        let highlighter = self.highlighter.clone();
        let tx = self.sender.clone();
        let cancelled = self.cancelled.clone();
        std::thread::spawn(move || {
            let mut watches = Vec::new();
            let data = if kind == Kind::Graph {
                let data = log::collect_graph(&root, Some(limit));
                if !cancelled.load(Ordering::Relaxed) {
                    watches = watch_paths(&root);
                }
                data
            } else {
                diff::commit_detail(
                    &root,
                    &rev,
                    diff::Limits::default(),
                    highlighter.as_deref().map(|h| h as &dyn diff::Highlighter),
                )
            };
            if cancelled.load(Ordering::Relaxed) {
                return;
            }
            let script = if data["ok"] == true {
                format!(
                    "window.{}.{}({data});",
                    if kind == Kind::Graph {
                        "ImproGit"
                    } else {
                        "ImproCommit"
                    },
                    if kind == Kind::Graph {
                        "setData"
                    } else {
                        "setCommit"
                    }
                )
            } else {
                let error = data["error"].as_str().unwrap_or("error");
                let (state, message) = match error {
                    "empty" => ("empty", "Make your first commit to start the timeline."),
                    "not-a-repo" if kind == Kind::Graph => {
                        ("not-a-repo", "Open a folder containing a Git repository.")
                    }
                    "git-missing" => ("git-missing", "Install Git to explore repository history."),
                    "unknown-revision" | "not-a-repo" => ("not-found", error),
                    _ => ("error", error),
                };
                format!(
                    "window.{}.setState({},{});",
                    if kind == Kind::Graph {
                        "ImproGit"
                    } else {
                        "ImproCommit"
                    },
                    json!(state),
                    json!(message)
                )
            };
            let _ = tx.send(Update {
                generation,
                data: Arc::new(data),
                script,
                watches,
                context: false,
            });
        });
    }
    fn deliver(self: &Rc<Self>, u: Update) {
        if u.context {
            self.context_running.set(false);
        } else {
            self.running.set(false);
        }
        if u.generation == self.generation.get() {
            if u.context {
                self.post_script(u.script);
            } else {
                if self.kind == Kind::Graph {
                    self.watch(u.watches);
                }
                let unchanged = self.last_script.borrow().as_ref() == Some(&u.script);
                *self.last.borrow_mut() = Some(u.data);
                *self.last_script.borrow_mut() = Some(u.script.clone());
                if !unchanged {
                    let mut script = u.script;
                    if self.kind == Kind::Graph {
                        let selection = self.selection.borrow().clone();
                        script.push_str(&self.command("select", &[json!(selection)]));
                        if self.focus.get() {
                            script.push_str(&self.command("focus", &[json!("HEAD")]));
                        }
                    }
                    self.post_script(script);
                }
            }
        }
        if self.dirty.get() && (self.kind == Kind::Commit || self.widget.is_mapped()) {
            self.start();
        }
        self.expand_next();
    }
    fn watch(self: &Rc<Self>, paths: Vec<(PathBuf, bool)>) {
        let previous = std::mem::take(&mut *self.monitors.borrow_mut());
        for m in previous {
            m.cancel();
        }
        let mut monitors = Vec::new();
        for (path, directory) in paths {
            let file = gio::File::for_path(path);
            let result = if directory {
                file.monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            } else {
                file.monitor_file(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            };
            if let Ok(monitor) = result {
                let weak = Rc::downgrade(self);
                monitor.connect_changed(move |_, _, _, _| {
                    if let Some(s) = weak.upgrade() {
                        s.debounce();
                    }
                });
                monitors.push(monitor);
            }
        }
        *self.monitors.borrow_mut() = monitors;
    }
    fn debounce(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        let old = self.debounce.borrow_mut().take();
        if let Some(old) = old {
            old.remove();
        }
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(Duration::from_millis(300), move || {
            if let Some(s) = weak.upgrade() {
                s.debounce.borrow_mut().take();
                s.refresh();
            }
        });
        *self.debounce.borrow_mut() = Some(source);
    }
    fn watch_focus(self: &Rc<Self>) {
        let window = self
            .widget
            .toplevel()
            .and_then(|w| w.downcast::<gtk::Window>().ok());
        if self
            .focus_window
            .borrow()
            .as_ref()
            .and_then(|(w, _)| w.upgrade())
            == window
        {
            return;
        }
        self.disconnect_focus();
        if let Some(window) = window {
            let weak = Rc::downgrade(self);
            let handler = window.connect_focus_in_event(move |_, _| {
                if let Some(s) = weak.upgrade() {
                    if s.kind == Kind::Graph {
                        s.debounce();
                    }
                }
                glib::Propagation::Proceed
            });
            *self.focus_window.borrow_mut() = Some((window.downgrade(), handler));
        }
    }
    fn disconnect_focus(&self) {
        let old = self.focus_window.borrow_mut().take();
        if let Some((window, id)) = old {
            if let Some(w) = window.upgrade() {
                w.disconnect(id);
            }
        }
    }
    fn file_request(self: &Rc<Self>, v: Value) {
        let Some(path) = v["path"]
            .as_str()
            .filter(|p| !p.is_empty() && !p.contains('\0') && !Path::new(p).is_absolute())
        else {
            return;
        };
        let root = self.root.borrow().clone();
        let path = path.to_owned();
        let line = v["line"].as_i64().unwrap_or(1).clamp(1, i32::MAX as i64) as i32;
        // Canonicalization and existence checks can touch remote storage: keep them off GTK.
        #[allow(deprecated)]
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        let weak = Rc::downgrade(self);
        let generation = self.generation.get();
        rx.attach(None, move |path: Option<PathBuf>| {
            if let (Some(s), Some(path)) = (weak.upgrade(), path) {
                if !s.closed.get() && s.generation.get() == generation {
                    let callbacks = s.open_file.borrow().clone();
                    for cb in callbacks {
                        cb(&path, line);
                    }
                }
            }
            glib::ControlFlow::Break
        });
        std::thread::spawn(move || {
            let result = root.canonicalize().ok().and_then(|r| {
                r.join(path)
                    .canonicalize()
                    .ok()
                    .filter(|p| p.starts_with(&r) && p.is_file())
            });
            let _ = tx.send(result);
        });
    }
    fn expand(self: &Rc<Self>, v: Value) {
        let (Some(path), Some(start), Some(end)) =
            (v["path"].as_str(), v["start"].as_i64(), v["end"].as_i64())
        else {
            return;
        };
        if start < 1 || end < start || self.contexts.borrow().len() >= 32 {
            return;
        }
        let hash = self
            .last
            .borrow()
            .as_ref()
            .and_then(|d| d["hash"].as_str())
            .map(str::to_owned);
        if let Some(rev) = hash {
            self.contexts.borrow_mut().push_back(ContextRequest {
                rev,
                path: path.into(),
                start,
                end: end.min(start.saturating_add(199)),
            });
            self.expand_next();
        }
    }
    fn expand_next(self: &Rc<Self>) {
        if self.closed.get() || self.context_running.get() {
            return;
        }
        let request = self.contexts.borrow_mut().pop_front();
        let Some(r) = request else {
            return;
        };
        self.context_running.set(true);
        let generation = self.generation.get();
        let root = self.root.borrow().clone();
        let h = self.highlighter.clone();
        let tx = self.sender.clone();
        let cancelled = self.cancelled.clone();
        std::thread::spawn(move || {
            let data = diff::file_context(
                &root,
                &r.rev,
                &r.path,
                r.start,
                r.end,
                h.as_deref().map(|h| h as &dyn diff::Highlighter),
            );
            if !cancelled.load(Ordering::Relaxed) {
                let script = format!(
                    "window.ImproCommit.provideContext({},{},{},{});",
                    json!(r.path),
                    r.start,
                    r.end,
                    data["lines"]
                );
                let _ = tx.send(Update {
                    generation,
                    data: Arc::new(data),
                    script,
                    watches: Vec::new(),
                    context: true,
                });
            }
        });
    }
    fn apply_theme(self: &Rc<Self>, colors: Value) {
        let bg = colors["background"]
            .as_str()
            .or_else(|| colors["bg"].as_str());
        if let Some(bg) = bg {
            if let Ok(color) = gdk::RGBA::parse(bg) {
                self.web.set_background_color(&color);
            }
        }
        *self.theme.borrow_mut() = Some(colors.clone());
        self.call("setTheme", &[colors]);
        self.settings();
    }
    fn shutdown(&self) {
        if self.closed.replace(true) {
            return;
        }
        self.cancelled.store(true, Ordering::Relaxed);
        self.cancellable.cancel();
        if let Some(handler) = self.message_handler.borrow_mut().take() {
            self.manager.disconnect(handler);
        }
        self.disconnect_focus();
        let debounce = self.debounce.borrow_mut().take();
        if let Some(source) = debounce {
            source.remove();
        }
        let source = self.receiver.borrow_mut().take();
        if let Some(source) = source {
            source.remove();
        }
        let monitors = std::mem::take(&mut *self.monitors.borrow_mut());
        for m in monitors {
            m.cancel();
        }
        self.queue.borrow_mut().clear();
        self.contexts.borrow_mut().clear();
        self.manager.unregister_script_message_handler("impro");
        self.web.stop_loading();
        let siblings = self.kind == Kind::Commit
            && COMMITS.with(|v| {
                v.borrow()
                    .iter()
                    .filter_map(Weak::upgrade)
                    .any(|v| !v.closed.get())
            });
        if !siblings {
            self.web.terminate_web_process();
        }
        // All callbacks hold weak state and shutdown has invalidated every entry point.
        unsafe {
            self.web.destroy();
        }
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn watch_paths(root: &Path) -> Vec<(PathBuf, bool)> {
    let mut paths = std::collections::BTreeSet::new();
    for arg in ["--absolute-git-dir", "--git-common-dir"] {
        if let Ok((raw, _)) = log::run(
            root,
            &["rev-parse", arg],
            Instant::now() + log::TIMEOUT,
            65536,
            false,
        ) {
            let p = PathBuf::from(String::from_utf8_lossy(&raw).trim());
            let dir = if p.is_absolute() { p } else { root.join(p) };
            paths.insert((dir.clone(), true));
            for name in ["HEAD", "logs/HEAD", "packed-refs"] {
                paths.insert((dir.join(name), false));
            }
            let mut todo: Vec<_> = ["refs/heads", "refs/remotes", "refs/tags"]
                .iter()
                .map(|p| dir.join(p))
                .collect();
            while let Some(dir) = todo.pop() {
                if paths.len() >= 512 {
                    break;
                }
                paths.insert((dir.clone(), true));
                if let Ok(entries) = dir.read_dir() {
                    for entry in entries.flatten() {
                        if entry.file_type().is_ok_and(|t| t.is_dir()) {
                            todo.push(entry.path());
                        }
                    }
                }
            }
        }
    }
    paths.into_iter().collect()
}

/// Lazy history pane: constructing it is the first point that creates WebKit.
#[derive(Clone)]
pub struct GitGraphView(Rc<Inner>);
impl GitGraphView {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self(Inner::new(root.as_ref(), "", Kind::Graph, None))
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.widget
    }
    /// Exposed for host accessibility, inspection and screenshot tools.
    pub fn webview(&self) -> &WebView {
        &self.0.web
    }
    pub fn is_ready(&self) -> bool {
        self.0.ready.get()
    }
    pub fn is_closed(&self) -> bool {
        self.0.closed.get()
    }
    pub fn root(&self) -> PathBuf {
        self.0.root.borrow().clone()
    }
    pub fn last_data(&self) -> Option<Arc<Value>> {
        self.0.last.borrow().clone()
    }
    pub fn refresh(&self) {
        self.0.refresh();
    }
    pub fn focus_head(&self) {
        self.0.focus.set(true);
        self.0.call("focus", &[json!("HEAD")]);
    }
    pub fn grab_focus(&self) {
        self.0.web.grab_focus();
    }
    pub fn select(&self, hash: Option<&str>) {
        *self.0.selection.borrow_mut() = hash.map(str::to_owned);
        self.0.call("select", &[json!(hash)]);
    }
    /// Invalid values leave settings unchanged.
    pub fn set_orientation(&self, value: &str) -> Result<(), &'static str> {
        if !matches!(value, "horizontal" | "vertical") {
            return Err("Orientation must be horizontal or vertical");
        }
        self.0.graph_settings.borrow_mut().orientation = value.into();
        self.0.call("setOrientation", &[json!(value)]);
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
        {
            let mut s = self.0.graph_settings.borrow_mut();
            s.show_remotes = show_remotes;
            s.show_tags = show_tags;
            s.time_format = time_format.into();
        }
        self.0.settings();
        Ok(())
    }
    pub fn set_max_commits(&self, limit: i64) {
        let limit = limit.clamp(100, 10000);
        let changed = {
            let mut s = self.0.graph_settings.borrow_mut();
            let changed = s.max_commits != limit;
            s.max_commits = limit;
            changed
        };
        if changed {
            self.refresh();
        }
    }
    pub fn settings(&self) -> GraphSettings {
        self.0.graph_settings.borrow().clone()
    }
    pub fn set_settings(&self, settings: GraphSettings) {
        let settings = GraphSettings::from_value(&settings.to_value());
        let changed = self.0.graph_settings.borrow().max_commits != settings.max_commits;
        *self.0.graph_settings.borrow_mut() = settings;
        self.0.settings();
        if changed {
            self.refresh();
        }
    }
    /// Read raw stored settings off-thread. Explicit live settings can be applied with `set_settings`.
    pub fn load_settings(&self) {
        #[allow(deprecated)]
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        let weak = Rc::downgrade(&self.0);
        rx.attach(None, move |value| {
            if let Some(s) = weak.upgrade() {
                if !s.closed.get() {
                    GitGraphView(s).set_settings(GraphSettings::from_value(&value));
                }
            }
            glib::ControlFlow::Break
        });
        std::thread::spawn(move || {
            let _ = tx.send(settings::get_gitgraph());
        });
    }
    pub fn set_theme(&self, ui: &UiColors, accent: &str) {
        self.0.apply_theme(palette::git_colors(ui, accent));
    }
    pub fn set_reduce_motion(&self, value: bool) {
        self.0.reduce_motion.set(value);
        self.0.settings();
    }
    /// Invalidate old work and notify the window to close that repository's commit tabs.
    pub fn set_root(&self, root: impl AsRef<Path>) {
        if self.is_closed() {
            return;
        }
        *self.0.root.borrow_mut() = root.as_ref().to_path_buf();
        let source = self.0.debounce.borrow_mut().take();
        if let Some(source) = source {
            source.remove();
        }
        let monitors = std::mem::take(&mut *self.0.monitors.borrow_mut());
        for m in monitors {
            m.cancel();
        }
        self.0.last.borrow_mut().take();
        self.0.last_script.borrow_mut().take();
        self.0.focus.set(false);
        self.select(None);
        self.0.call("setData", &[json!({"ok":true,"commits":[]})]);
        self.0.state("loading", "");
        self.refresh();
        let callbacks = self.0.root_changed.borrow().clone();
        for cb in callbacks {
            cb();
        }
    }
    pub fn connect_open_commit(&self, f: impl Fn(&str) + 'static) {
        self.0.open_commit.borrow_mut().push(Rc::new(f));
    }
    pub fn connect_error(&self, f: impl Fn(&str) + 'static) {
        self.0.errors.borrow_mut().push(Rc::new(f));
    }
    pub fn connect_orientation_changed(&self, f: impl Fn(&str) + 'static) {
        self.0.orientation_changed.borrow_mut().push(Rc::new(f));
    }
    pub fn connect_root_changed(&self, f: impl Fn() + 'static) {
        self.0.root_changed.borrow_mut().push(Rc::new(f));
    }
    pub fn shutdown(&self) {
        self.0.shutdown();
    }
}
/// Read-only commit tab. Parent chips emit `open_commit`; files emit existing safe paths.
#[derive(Clone)]
pub struct CommitView(Rc<Inner>);
impl CommitView {
    pub fn new(root: impl AsRef<Path>, rev: &str, highlighter: Option<SyntaxAdapter>) -> Self {
        Self(Inner::new(root.as_ref(), rev, Kind::Commit, highlighter))
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.0.widget
    }
    pub fn webview(&self) -> &WebView {
        &self.0.web
    }
    pub fn rev(&self) -> &str {
        &self.0.rev
    }
    pub fn tab_key(&self) -> String {
        format!("commit:{}", self.rev())
    }
    pub fn is_ready(&self) -> bool {
        self.0.ready.get()
    }
    pub fn is_closed(&self) -> bool {
        self.0.closed.get()
    }
    pub fn last_data(&self) -> Option<Arc<Value>> {
        self.0.last.borrow().clone()
    }
    pub fn reload(&self) {
        self.0.refresh();
    }
    pub fn grab_focus(&self) {
        self.0.web.grab_focus();
    }
    pub fn set_theme(&self, theme: &Theme, ui: &UiColors, accent: &str, size: i32) {
        self.0
            .apply_theme(palette::commit_colors(theme, ui, accent, size));
    }
    pub fn set_font_size(&self, size: i32) {
        let colors = self.0.theme.borrow().clone();
        if let Some(mut colors) = colors {
            colors["size"] = json!(size);
            self.0.apply_theme(colors);
        }
    }
    pub fn set_reduce_motion(&self, value: bool) {
        self.0.reduce_motion.set(value);
        self.0.settings();
    }
    pub fn connect_open_commit(&self, f: impl Fn(&str) + 'static) {
        self.0.open_commit.borrow_mut().push(Rc::new(f));
    }
    pub fn connect_error(&self, f: impl Fn(&str) + 'static) {
        self.0.errors.borrow_mut().push(Rc::new(f));
    }
    pub fn connect_open_file(&self, f: impl Fn(&Path, i32) + 'static) {
        self.0.open_file.borrow_mut().push(Rc::new(f));
    }
    pub fn shutdown(&self) {
        self.0.shutdown();
    }
}
