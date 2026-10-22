use crate::MainWindow;
use gtk::prelude::*;
use impro_core::themes;
use impro_gitviews::{CommitView, GitGraphView};
use impro_graph::{GraphType, GraphView};
use impro_terminal::TerminalPanel;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

/// Character-offset syntax adapter used by Git worker threads.
pub struct DiffHighlighter;
impl impro_gitviews::diff::Highlighter for DiffHighlighter {
    fn language(&self, path: &str) -> Option<String> {
        let language = impro_editor::highlight::language_name(path, "");
        (language != "Plain Text").then(|| language.to_string())
    }
    fn classify(&self, path: &str, text: &str) -> Result<Vec<(usize, usize, String)>, String> {
        Ok(impro_editor::highlight::tokenize(path, text)
            .into_iter()
            .map(|(start, end, kind)| (start, end, kind.into()))
            .collect())
    }
}

impl MainWindow {
    pub fn graph_views(&self) -> Vec<GraphView> {
        self.graphs
            .borrow()
            .iter()
            .map(|(_, v)| v.clone())
            .collect()
    }
    pub fn gitgraph_views(&self) -> Vec<GitGraphView> {
        self.gitgraphs
            .borrow()
            .iter()
            .map(|(_, v)| v.clone())
            .collect()
    }
    pub fn commit_views(&self) -> Vec<CommitView> {
        self.commits.borrow().values().cloned().collect()
    }
    pub fn pending_commits(&self) -> usize {
        self.commit_pending.borrow().len()
    }
    pub fn add_dock_pane(&self, panel: &TerminalPanel, kind: &str) -> bool {
        if !self.require_feature(kind) {
            return false;
        }
        if kind == "terminal" {
            return panel.new_terminal();
        }
        if panel.count_kind(kind) > 0 {
            return panel.add_kind(kind);
        }
        if panel.is_full() {
            panel.add_kind(kind);
            return false;
        }
        match kind {
            "graph" => {
                let view = self.make_graph();
                let shutdown = view.dock_shutdown();
                let weak = self.weak();
                let p = panel.downgrade();
                let accepted = panel.add_custom_pane(
                    "graph",
                    view.widget(),
                    Some(Box::new(move || {
                        shutdown();
                        if let (Some(this), Some(panel)) = (weak.upgrade().map(Self), p.upgrade()) {
                            this.graphs.borrow_mut().retain(|(p, _)| *p != panel);
                        }
                    })),
                );
                if accepted {
                    self.graphs.borrow_mut().push((panel.clone(), view));
                } else {
                    view.shutdown();
                }
                accepted
            }
            "gitgraph" => {
                let view = self.make_gitgraph();
                let closed = view.clone();
                let weak = self.weak();
                let p = panel.downgrade();
                let accepted = panel.add_custom_pane(
                    "gitgraph",
                    view.widget(),
                    Some(Box::new(move || {
                        closed.shutdown();
                        if let (Some(this), Some(panel)) = (weak.upgrade().map(Self), p.upgrade()) {
                            this.gitgraphs.borrow_mut().retain(|(p, _)| *p != panel);
                        }
                    })),
                );
                if accepted {
                    self.gitgraphs.borrow_mut().push((panel.clone(), view));
                } else {
                    view.shutdown();
                }
                accepted
            }
            _ => false,
        }
    }
    fn make_graph(&self) -> GraphView {
        let values = self.values();
        let store = Arc::new(impro_graph::view::store::MemoryStore::new(
            values.get("graph").cloned().unwrap_or_default(),
        ));
        let view =
            GraphView::with_options(self.root(), impro_graph::ViewOptions { store, scan: true });
        if let Some(prefs) = values.get("graph") {
            view.apply_prefs(prefs);
        }
        view.set_graph_type(
            GraphType::parse(values["graph_default_type"].as_str().unwrap_or("global"))
                .unwrap_or_default(),
        );
        view.set_active_file(self.editor.current_path().as_deref());
        let ui = self.git_theme_ui();
        view.set_theme(&ui, &ui["accent"]);
        view.set_font(values["ui_font"].as_str());
        view.set_reduce_motion(values["reduce_motion"] == true);
        for tab in self.editor.tabs() {
            if tab.buffer.is_modified() && !tab.deleted() {
                view.buffer_changed(&tab.path(), Some(tab.text()));
            }
        }
        let weak = self.weak();
        view.connect_open_file(move |path, _mode| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.restore_terminal_fullscreen();
                this.editor.open_file(path);
            }
        });
        let weak = self.weak();
        view.connect_search_tag(move |tag| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.search_in_files();
                if this.feature_enabled("explorer") {
                    this.explorer
                        .focus_search(Some(&impro_graph::tag_query(tag)));
                }
            }
        });
        let weak = self.weak();
        view.connect_create_note(move |name| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.create_graph_note(name);
            }
        });
        let weak = self.weak();
        view.connect_reveal(move |path| {
            if let Some(this) = weak.upgrade().map(Self) {
                if this.require_feature("explorer") {
                    this.restore_terminal_fullscreen();
                    Self::set_visible(&this.explorer, true);
                    this.explorer.set_current_file(Some(path));
                }
            }
        });
        view.connect_copy_path(|path| {
            gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&path.display().to_string())
        });
        let weak = self.weak();
        view.connect_settings_changed(move |value| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.graph_preferences_changed(value);
            }
        });
        let weak = self.weak();
        view.connect_error(move |error| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.view_error("Graph view", error);
            }
        });
        view
    }
    fn make_gitgraph(&self) -> GitGraphView {
        let view = GitGraphView::new(self.root());
        view.set_settings(self.git_settings());
        let ui = self.git_theme_ui();
        view.set_theme(&ui, &ui["accent"]);
        view.set_reduce_motion(self.values()["reduce_motion"] == true);
        view.select(self.selected_commit().as_deref());
        let weak = self.weak();
        view.connect_open_commit(move |rev| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.open_commit(rev);
            }
        });
        let weak = self.weak();
        view.connect_error(move |error| {
            if let Some(this) = weak.upgrade().map(Self) {
                this.view_error("Git graph", error);
            }
        });
        let weak = self.weak();
        view.connect_orientation_changed(move |orientation| {
            if let Some(this) = weak.upgrade().map(Self) {
                let _ = this.set_setting("gitgraph_orientation", orientation.into(), true);
            }
        });
        let weak = self.weak();
        view.connect_root_changed(move || {
            if let Some(this) = weak.upgrade().map(Self) {
                this.close_commit_tabs();
            }
        });
        view
    }
    pub fn open_graph(&self) -> Option<GraphView> {
        if !self.require_feature("graph") {
            return None;
        }
        self.restore_terminal_fullscreen();
        let panel = &self.terminal_right;
        let existing = panel.has_graph();
        Self::set_visible(panel, true);
        self.add_dock_pane(panel, "graph");
        let view = self
            .graphs
            .borrow()
            .iter()
            .find(|(p, _)| p == panel)
            .map(|(_, v)| v.clone());
        if let Some(view) = &view {
            if existing {
                view.open_global();
            }
            view.canvas().grab_focus();
        }
        view
    }
    pub fn open_local_graph(&self) -> Option<GraphView> {
        if !self.require_feature("graph") {
            return None;
        }
        let Some(path) = self.editor.current_path() else {
            self.notify("Open a file to show its local graph.", "info");
            return None;
        };
        let view = self.open_graph()?;
        view.open_local_with_depth(&path, 1);
        Some(view)
    }
    pub fn create_graph_note(&self, name: &str) {
        if name.trim().is_empty() || name.contains(['/', '\\']) || name.contains("..") {
            self.notify("Graph note names must be simple file names.", "info");
            return;
        }
        let name = if name.to_ascii_lowercase().ends_with(".md") {
            name.to_owned()
        } else {
            format!("{name}.md")
        };
        let root = self.root();
        let weak = self.weak();
        self.worker(
            move || impro_core::fsops::create_file(root, &name),
            move |result| {
                if let Some(this) = weak.upgrade().map(Self) {
                    match result {
                        Ok(path) => {
                            this.explorer.refresh();
                            for view in this.graph_views() {
                                view.file_saved(&path);
                            }
                            this.restore_terminal_fullscreen();
                            this.editor.open_file(path);
                        }
                        Err(error) => this.notify(&error.to_string(), "info"),
                    }
                }
            },
        );
    }
    pub fn rebuild_graph(&self) {
        if self.require_feature("graph") {
            for view in self.graph_views() {
                view.rebuild();
            }
        }
    }
    pub fn reset_graph_camera(&self) {
        if self.require_feature("graph") {
            for view in self.graph_views() {
                view.reset_camera();
            }
        }
    }
    pub(crate) fn schedule_tree_refresh(&self) {
        if self.graph_views().is_empty()
            || self.tree_refresh.borrow().is_some()
            || self.closed.get()
        {
            return;
        }
        let weak = self.weak();
        let source = glib::timeout_add_local_once(Duration::from_millis(80), move || {
            if let Some(this) = weak.upgrade().map(Self) {
                this.tree_refresh.borrow_mut().take();
                this.rebuild_graph();
            }
        });
        *self.tree_refresh.borrow_mut() = Some(source);
    }
    pub fn open_gitgraph(&self) -> Option<GitGraphView> {
        if !self.require_feature("gitgraph") {
            return None;
        }
        self.restore_terminal_fullscreen();
        let panels = self.panels();
        let Some(panel) = panels
            .iter()
            .find(|p| p.has_gitgraph())
            .or_else(|| panels.iter().find(|p| !p.is_full()))
        else {
            if !self.full_notice.replace(true) {
                self.notify(
                    "Both docks are full. Close a pane to open Git graph.",
                    "info",
                );
            }
            return None;
        };
        self.full_notice.set(false);
        Self::set_visible(panel, true);
        self.add_dock_pane(panel, "gitgraph");
        let view = self
            .gitgraphs
            .borrow()
            .iter()
            .find(|(p, _)| p == panel)
            .map(|(_, v)| v.clone());
        if let Some(view) = &view {
            view.grab_focus();
        }
        view
    }
    pub fn refresh_gitgraph(&self) {
        if self.require_feature("gitgraph") {
            for view in self.gitgraph_views() {
                view.refresh();
            }
        }
    }
    pub fn focus_git_head(&self) {
        if let Some(view) = self.open_gitgraph() {
            view.focus_head();
        }
    }
    pub fn selected_commit(&self) -> Option<String> {
        self.editor
            .current_view_key()
            .and_then(|key| key.strip_prefix("commit:").map(str::to_owned))
    }
    pub(crate) fn sync_git_selection(&self) {
        let selected = self.selected_commit();
        for view in self.gitgraph_views() {
            view.select(selected.as_deref());
        }
    }
    pub fn open_commit(&self, rev: &str) {
        if self.closed.get() {
            return;
        }
        if !(4..=40).contains(&rev.len()) || !rev.bytes().all(|b| b.is_ascii_hexdigit()) {
            self.notify("Invalid commit hash", "info");
            return;
        }
        let rev = rev.to_ascii_lowercase();
        if rev.len() == 40 {
            self.open_full_commit(&rev);
            return;
        }
        if !self.commit_pending.borrow_mut().insert(rev.clone()) {
            return;
        }
        let epoch = self.epoch.get();
        let root = self.root();
        let weak = self.weak();
        let requested = rev.clone();
        self.worker(
            move || {
                let argument = format!("{rev}^{{commit}}");
                impro_gitviews::log::run(
                    &root,
                    &["rev-parse", "--verify", "--end-of-options", &argument],
                    Instant::now() + Duration::from_secs(5),
                    256,
                    false,
                )
                .ok()
                .and_then(|(bytes, _)| String::from_utf8(bytes).ok())
                .map(|s| s.trim().to_owned())
                .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            },
            move |hash| {
                if let Some(this) = weak.upgrade().map(Self) {
                    if this.epoch.get() != epoch {
                        return;
                    }
                    this.commit_pending.borrow_mut().remove(&requested);
                    if let Some(hash) = hash {
                        this.open_full_commit(&hash);
                    } else {
                        this.notify("Commit not found", "info");
                    }
                }
            },
        );
    }
    fn open_full_commit(&self, rev: &str) {
        let key = format!("commit:{rev}");
        let existing = self.commits.borrow().get(&key).cloned();
        let view = existing.unwrap_or_else(|| {
            let view = CommitView::new(self.root(), rev, Some(Arc::new(DiffHighlighter)));
            let ui = self.git_theme_ui();
            let theme = themes::get_theme(Some(&self.theme_id()));
            view.set_theme(theme, &ui, &ui["accent"], self.editor.font_size());
            view.set_reduce_motion(self.values()["reduce_motion"] == true);
            let weak = self.weak();
            view.connect_open_commit(move |rev| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.open_commit(rev);
                }
            });
            let weak = self.weak();
            view.connect_open_file(move |path, line| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.open_commit_file(path, line);
                }
            });
            let weak = self.weak();
            view.connect_error(move |error| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.view_error("Git graph", error);
                }
            });
            self.commits.borrow_mut().insert(key.clone(), view.clone());
            view
        });
        self.restore_terminal_fullscreen();
        let shutdown = view.clone();
        let subject = self
            .gitgraph_views()
            .iter()
            .filter_map(|view| view.last_data())
            .find_map(|data| {
                data["commits"]
                    .as_array()?
                    .iter()
                    .find(|commit| commit["hash"].as_str() == Some(rev))
                    .and_then(|commit| commit["subject"].as_str().map(str::to_owned))
            })
            .unwrap_or_else(|| rev.to_owned());
        self.editor.add_view_tab_with_shutdown(
            &key,
            &rev[..7],
            view.widget(),
            Some(&subject),
            Some("document-open-recent-symbolic"),
            move || shutdown.shutdown(),
        );
        self.sync_git_selection();
    }
    pub fn open_commit_file(&self, path: &Path, line: i32) {
        let path = path.to_path_buf();
        let check = path.clone();
        let weak = self.weak();
        self.worker(
            move || check.is_absolute() && check.is_file(),
            move |valid| {
                if let Some(this) = weak.upgrade().map(Self) {
                    if valid {
                        this.restore_terminal_fullscreen();
                        this.open_file_at(&path, (line - 1).max(0), 0, 0);
                    } else {
                        this.notify("File not in the working tree", "info");
                    }
                }
            },
        );
    }
    pub fn close_commit_tabs(&self) {
        self.epoch.set(self.epoch.get() + 1);
        self.commit_pending.borrow_mut().clear();
        let keys: Vec<_> = self.commits.borrow().keys().cloned().collect();
        for key in keys {
            self.editor.close_view_tab(&key);
        }
    }
    pub fn view_error(&self, kind: &str, message: &str) {
        let first = !self.errors.borrow().iter().any(|(k, _)| k == kind);
        if !self
            .errors
            .borrow_mut()
            .insert((kind.into(), message.into()))
        {
            return;
        }
        eprintln!("{kind}: {message}");
        if first {
            self.notify(
                if kind == "Graph view" {
                    "Graph view encountered an error. See the application log."
                } else {
                    "Git view encountered an error. See the application log."
                },
                "error",
            );
        }
    }
}
