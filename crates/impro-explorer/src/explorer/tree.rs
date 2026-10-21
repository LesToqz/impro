//! Lazy, time-sliced directory tree: scans run on the pool, rows are merged into the
//! `GtkTreeStore` in steps of a few milliseconds so even a 50 000-entry folder keeps the
//! interface responsive.
use super::*;
use crate::pool::{self, Lane};
use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Main-thread time spent merging rows per step.
const APPLY_BUDGET: Duration = Duration::from_millis(3);

/// One directory entry, sorted and pre-formatted off the main thread.
#[derive(Clone, Debug)]
pub struct Entry {
    pub is_dir: bool,
    pub name: String,
    pub path: String,
    /// Case-folded name used for ordering.
    pub key: String,
}

static COUNTING: AtomicBool = AtomicBool::new(false);
static SCANS: Mutex<Vec<(PathBuf, usize)>> = Mutex::new(Vec::new());

/// Test seam: count directory scans per path while enabled.
#[doc(hidden)]
pub fn set_scan_counting(enabled: bool) {
    COUNTING.store(enabled, AtomicOrdering::SeqCst);
    SCANS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// Directory scans of `dir` since [`set_scan_counting`] was enabled.
#[doc(hidden)]
pub fn scan_count(dir: &Path) -> usize {
    SCANS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(path, _)| path == dir)
        .map_or(0, |(_, count)| *count)
}

fn record_scan(dir: &Path) {
    if COUNTING.load(AtomicOrdering::Relaxed) {
        let mut scans = SCANS.lock().unwrap_or_else(|e| e.into_inner());
        match scans.iter_mut().find(|(path, _)| path == dir) {
            Some((_, count)) => *count += 1,
            None => scans.push((dir.to_path_buf(), 1)),
        }
    }
}

/// Ordering of siblings: folders first (optionally), then case-folded name, then name.
pub(crate) fn order(a: &Entry, b: &Entry, folders_first: bool) -> Ordering {
    let rank = |e: &Entry| folders_first && !e.is_dir;
    rank(a)
        .cmp(&rank(b))
        .then_with(|| a.key.cmp(&b.key))
        .then_with(|| a.name.cmp(&b.name))
}

/// List and sort a directory. Directory symlinks remain leaf entries, avoiding cycles;
/// unreadable directories list as empty.
pub fn scan_dir(dir: &Path, show_hidden: bool, folders_first: bool) -> Vec<Entry> {
    record_scan(dir);
    let mut entries = Vec::new();
    if let Ok(listing) = std::fs::read_dir(dir) {
        for item in listing.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            let Ok(kind) = item.file_type() else { continue };
            entries.push(Entry {
                is_dir: kind.is_dir(),
                key: crate::search::fold(&name),
                path: item.path().to_string_lossy().into_owned(),
                name,
            });
        }
    }
    entries.sort_unstable_by(|a, b| order(a, b, folders_first));
    entries
}

/// An in-progress merge of a fresh listing into one directory's children.
pub struct Apply {
    generation: u64,
    dir: PathBuf,
    row: gtk::TreeRowReference,
    entries: Vec<Entry>,
    folders_first: bool,
    pos: usize,
    started: bool,
    placeholder: Option<gtk::TreeIter>,
    cursor: Option<gtk::TreeIter>,
    to_expand: Vec<gtk::TreeRowReference>,
}

fn placeholder_values() -> [(u32, &'static dyn ToValue); 3] {
    [(0, &None::<&str>), (1, &""), (2, &"")]
}

impl ExplorerPanel {
    /// Queue a scan of `dir`; its rows are merged when the listing arrives.
    pub(super) fn request_scan(&self, dir: &Path) {
        let imp = self.imp();
        imp.loaded.borrow_mut().insert(dir.to_path_buf());
        let Some(sender) = self.sender() else { return };
        imp.pending_scans.set(imp.pending_scans.get() + 1);
        let generation = imp.generation.get();
        let (hidden, folders_first) = (imp.show_hidden.get(), imp.folders_first.get());
        let dir = dir.to_path_buf();
        pool::spawn(Lane::Interactive, move || {
            let entries = scan_dir(&dir, hidden, folders_first);
            let _ = sender.send(Msg::Dir {
                generation,
                dir,
                entries,
            });
        });
    }

    pub(super) fn on_dir_listed(&self, generation: u64, dir: PathBuf, entries: Vec<Entry>) {
        let imp = self.imp();
        if generation != imp.generation.get() {
            return;
        }
        imp.pending_scans.set(imp.pending_scans.get().saturating_sub(1));
        let row = self
            .find_row(&dir)
            .and_then(|iter| self.w().store.path(&iter))
            .and_then(|path| gtk::TreeRowReference::new(&self.w().store, &path));
        let Some(row) = row else {
            imp.loaded.borrow_mut().remove(&dir);
            self.settle();
            return;
        };
        {
            let mut tasks = imp.tasks.borrow_mut();
            tasks.retain(|task| task.dir != dir);
            tasks.push_back(Apply {
                generation,
                dir,
                row,
                entries,
                folders_first: imp.folders_first.get(),
                pos: 0,
                started: false,
                placeholder: None,
                cursor: None,
                to_expand: Vec::new(),
            });
        }
        self.run_tasks();
    }

    /// Merge queued listings for a few milliseconds; continue from an idle source.
    pub(super) fn run_tasks(&self) {
        let imp = self.imp();
        if let Some(source) = imp.apply_source.take() {
            source.remove();
        }
        let deadline = Instant::now() + APPLY_BUDGET;
        loop {
            let next = imp.tasks.borrow_mut().pop_front();
            let Some(mut task) = next else { break };
            if task.generation != imp.generation.get() {
                continue;
            }
            if self.step_apply(&mut task, deadline) {
                self.apply_finished();
            } else if task.generation == imp.generation.get() {
                let mut tasks = imp.tasks.borrow_mut();
                // A newer listing of the same folder replaces this one.
                if !tasks.iter().any(|t| t.dir == task.dir) {
                    tasks.push_back(task);
                }
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        if imp.tasks.borrow().is_empty() {
            self.settle();
        } else {
            let weak = self.downgrade();
            let source = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
                if let Some(panel) = weak.upgrade() {
                    panel.imp().apply_source.take();
                    panel.run_tasks();
                }
                glib::ControlFlow::Break
            });
            *imp.apply_source.borrow_mut() = Some(source);
        }
    }

    fn insert_entry(&self, parent: &gtk::TreeIter, sibling: Option<&gtk::TreeIter>, entry: &Entry) -> gtk::TreeIter {
        let store = &self.w().store;
        let icon = if entry.is_dir { "folder" } else { "text-x-generic" };
        let values: [(u32, &dyn ToValue); 3] = [(0, &icon), (1, &entry.name), (2, &entry.path)];
        let iter = match sibling {
            None => store.insert_with_values(Some(parent), None, &values),
            Some(sibling) => {
                let iter = store.insert_before(Some(parent), Some(sibling));
                store.set(&iter, &values);
                iter
            }
        };
        if entry.is_dir {
            store.insert_with_values(Some(&iter), None, &placeholder_values());
        }
        iter
    }

    /// Advance one merge by up to the deadline; `true` when the folder is fully merged
    /// (or its row vanished).
    fn step_apply(&self, task: &mut Apply, deadline: Instant) -> bool {
        let w = self.w();
        let store = &w.store;
        let Some(dir_path) = task.row.path() else {
            return true;
        };
        let Some(parent) = store.iter(&dir_path) else {
            return true;
        };
        if !task.started {
            task.started = true;
            match store.iter_children(Some(&parent)) {
                Some(first) if util::string(store, &first, 2).is_empty() => task.placeholder = Some(first),
                other => task.cursor = other,
            }
        }
        let wanted_empty = self.imp().want_expanded.borrow().is_empty();
        let mut counter = 0u32;
        while task.pos < task.entries.len() {
            counter += 1;
            if counter % 16 == 0 && Instant::now() >= deadline {
                break;
            }
            let entry = &task.entries[task.pos];
            let mut inserted: Option<gtk::TreeIter> = None;
            let mut kept: Option<gtk::TreeIter> = None;
            match task.cursor.take() {
                None => {
                    inserted = Some(self.insert_entry(&parent, None, entry));
                    task.pos += 1;
                }
                Some(current) => {
                    let current_path = util::string(store, &current, 2);
                    if current_path == entry.path {
                        let current_is_dir = util::string(store, &current, 0) == "folder";
                        if current_is_dir == entry.is_dir {
                            let next = current.clone();
                            task.cursor = store.iter_next(&next).then_some(next);
                            kept = Some(current);
                        } else {
                            // A file became a folder (or back): replace the row.
                            inserted = Some(self.insert_entry(&parent, Some(&current), entry));
                            let next = current.clone();
                            task.cursor = store.remove(&next).then_some(next);
                        }
                        task.pos += 1;
                    } else {
                        let current_entry = Entry {
                            is_dir: util::string(store, &current, 0) == "folder",
                            name: util::string(store, &current, 1),
                            key: String::new(),
                            path: current_path,
                        };
                        let current_entry = Entry {
                            key: crate::search::fold(&current_entry.name),
                            ..current_entry
                        };
                        if order(&current_entry, entry, task.folders_first) == Ordering::Less {
                            // Gone from disk.
                            let next = current.clone();
                            task.cursor = store.remove(&next).then_some(next);
                        } else {
                            inserted = Some(self.insert_entry(&parent, Some(&current), entry));
                            task.pos += 1;
                            task.cursor = Some(current);
                        }
                    }
                }
            }
            if inserted.is_some() {
                // Keep the placeholder until real rows exist, or GTK collapses the row.
                if let Some(placeholder) = task.placeholder.take() {
                    store.remove(&placeholder);
                }
            }
            if !wanted_empty && entry.is_dir {
                if let Some(row) = inserted.as_ref().or(kept.as_ref()) {
                    if self.imp().want_expanded.borrow().contains(Path::new(&entry.path)) {
                        if let Some(reference) = store
                            .path(row)
                            .and_then(|path| gtk::TreeRowReference::new(store, &path))
                        {
                            task.to_expand.push(reference);
                        }
                    }
                }
            }
        }
        let finished = task.pos >= task.entries.len();
        if finished {
            // Everything left over vanished from disk.
            while let Some(current) = task.cursor.take() {
                let next = current.clone();
                task.cursor = store.remove(&next).then_some(next);
            }
            if let Some(placeholder) = task.placeholder.take() {
                store.remove(&placeholder);
            }
        }
        for reference in std::mem::take(&mut task.to_expand) {
            if let Some(path) = reference.path() {
                if !w.tree.row_expanded(&path) {
                    w.tree.expand_row(&path, false);
                }
            }
        }
        finished
    }

    fn apply_finished(&self) {
        self.advance_reveal();
    }

    /// When every listing has been merged, forget the one-shot restore requests.
    pub(super) fn settle(&self) {
        let imp = self.imp();
        if imp.pending_scans.get() != 0 || !imp.tasks.borrow().is_empty() {
            return;
        }
        self.advance_reveal();
        imp.want_expanded.borrow_mut().clear();
        imp.want_select.borrow_mut().take();
    }

    /// Listings are being fetched or merged.
    pub fn is_busy(&self) -> bool {
        let imp = self.imp();
        imp.pending_scans.get() != 0 || !imp.tasks.borrow().is_empty()
    }

    // ---- lookup ---------------------------------------------------------------------------

    /// The row of `path`, if it has been loaded into the tree.
    pub fn find_row(&self, path: &Path) -> Option<gtk::TreeIter> {
        let store = &self.w().store;
        let root = self.root();
        let mut row = store.iter_first()?;
        if path == root {
            return Some(row);
        }
        let relative = path.strip_prefix(&root).ok()?;
        for component in relative.components() {
            let name = component.as_os_str().to_string_lossy();
            let child = store.iter_children(Some(&row))?;
            loop {
                if util::string(store, &child, 1) == name {
                    row = child;
                    break;
                }
                if !store.iter_next(&child) {
                    return None;
                }
            }
        }
        Some(row)
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        let (_, iter) = self.w().tree.selection().selected()?;
        let path = util::string(&self.w().store, &iter, 2);
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    /// Select and scroll to `path` (it must be visible: loaded under expanded folders).
    pub fn select(&self, path: &Path) {
        if let Some(row) = self.find_row(path) {
            self.select_iter(&row);
        }
    }

    fn select_iter(&self, row: &gtk::TreeIter) {
        let tree = &self.w().tree;
        let path = util::path_of(&self.w().store, row);
        tree.set_cursor(&path, None::<&gtk::TreeViewColumn>, false);
        tree.scroll_to_cell(Some(&path), None::<&gtk::TreeViewColumn>, false, 0.0, 0.0);
    }

    pub fn expanded_paths(&self) -> HashSet<PathBuf> {
        let w = self.w();
        let found = RefCell::new(HashSet::new());
        w.tree.map_expanded_rows(|_, path| {
            if let Some(iter) = w.store.iter(path) {
                found.borrow_mut().insert(PathBuf::from(util::string(&w.store, &iter, 2)));
            }
        });
        found.into_inner()
    }

    /// Where "New File" / "New Folder" create: the selected folder, else its parent.
    pub fn target_folder(&self) -> PathBuf {
        let root = self.root();
        let w = self.w();
        let Some((_, iter)) = w.tree.selection().selected() else {
            return root;
        };
        let selected = util::string(&w.store, &iter, 2);
        if selected.is_empty() {
            return root;
        }
        let selected = PathBuf::from(selected);
        // Directory symlinks are leaf rows, so the icon tells real folders apart.
        if util::string(&w.store, &iter, 0) == "folder" {
            selected
        } else {
            selected.parent().map_or(root, Path::to_path_buf)
        }
    }

    // ---- rebuilding -------------------------------------------------------------------------

    fn restart_generation(&self) {
        let imp = self.imp();
        imp.generation.set(imp.generation.get() + 1);
        imp.tasks.borrow_mut().clear();
        if let Some(source) = imp.apply_source.take() {
            source.remove();
        }
        imp.pending_scans.set(0);
    }

    /// Expand every wanted folder that is already in the tree (parents first).
    fn expand_wanted(&self) {
        let mut wanted: Vec<PathBuf> = self.imp().want_expanded.borrow().iter().cloned().collect();
        wanted.sort_by_key(|path| path.components().count());
        let tree = &self.w().tree;
        for path in wanted {
            if let Some(row) = self.find_row(&path) {
                let tree_path = util::path_of(&self.w().store, &row);
                if !tree.row_expanded(&tree_path) {
                    tree.expand_row(&tree_path, false);
                }
            }
        }
    }

    /// Replace the whole tree with the root alone, then restore `expanded` and `selected`
    /// as folders load.
    pub(crate) fn rebuild_clear(&self, expanded: HashSet<PathBuf>, selected: Option<PathBuf>) {
        let imp = self.imp();
        let w = self.w();
        self.restart_generation();
        imp.loaded.borrow_mut().clear();
        w.store.clear();
        let root = self.root();
        let name = root
            .file_name()
            .map_or_else(|| model_path(&root), |n| n.to_string_lossy().into_owned());
        let iter = w.store.insert_with_values(
            None,
            None,
            &[(0, &"folder"), (1, &name), (2, &model_path(&root))],
        );
        w.store
            .insert_with_values(Some(&iter), None, &placeholder_values());
        *imp.want_expanded.borrow_mut() = expanded;
        *imp.want_select.borrow_mut() = selected;
        self.expand_wanted();
        self.settle();
    }

    /// Re-list every visible folder and merge the changes in place, keeping expansion,
    /// selection and scroll position. Collapsed folders reload when next expanded.
    pub(crate) fn rebuild(&self, expanded: HashSet<PathBuf>, selected: Option<PathBuf>) {
        let imp = self.imp();
        self.restart_generation();
        *imp.want_expanded.borrow_mut() = expanded;
        *imp.want_select.borrow_mut() = selected;
        let visible = self.expanded_paths();
        let loaded: Vec<PathBuf> = imp.loaded.take().into_iter().collect();
        for dir in loaded {
            if !visible.contains(&dir) {
                self.reset_dir(&dir);
            }
        }
        // Rescan the expanded folders, shallowest first.
        let mut visible: Vec<PathBuf> = visible.into_iter().collect();
        visible.sort_by_key(|path| path.components().count());
        for dir in &visible {
            self.request_scan(dir);
        }
        self.expand_wanted();
        self.settle();
    }

    /// Drop a folder's children (it is collapsed) and leave only the placeholder.
    fn reset_dir(&self, dir: &Path) {
        let store = &self.w().store;
        let Some(row) = self.find_row(dir) else { return };
        while let Some(child) = store.iter_children(Some(&row)) {
            store.remove(&child);
        }
        store.insert_with_values(Some(&row), None, &placeholder_values());
    }

    pub(super) fn on_expanded(&self, iter: &gtk::TreeIter) {
        // GTK3 exposes neither animated child geometry nor a per-row chevron widget, so
        // expansion stays instant, including when reduced motion is enabled.
        let path = util::string(&self.w().store, iter, 2);
        if path.is_empty() {
            return;
        }
        let path = PathBuf::from(path);
        if self.imp().loaded.borrow().contains(&path) {
            return;
        }
        self.request_scan(&path);
    }

    pub(super) fn on_activated(&self, path: &gtk::TreePath) {
        let w = self.w();
        let Some(iter) = w.store.iter(path) else { return };
        let full_path = util::string(&w.store, &iter, 2);
        if util::string(&w.store, &iter, 0) == "folder" {
            if w.tree.row_expanded(path) {
                w.tree.collapse_row(path);
            } else {
                w.tree.expand_row(path, false);
            }
        } else if !full_path.is_empty() {
            self.emit_file_activated(Path::new(&full_path));
        }
    }

    // ---- reveal -----------------------------------------------------------------------------

    /// Expand to `path` as listings arrive, then select it; outside the root only clears
    /// the selection.
    pub(super) fn reveal_path(&self, path: Option<&Path>) {
        let tree = &self.w().tree;
        let root = self.root();
        let Some(path) = path.map(impro_core::fsops::absolute) else {
            tree.selection().unselect_all();
            self.imp().want_select.borrow_mut().take();
            return;
        };
        if !path.starts_with(&root) {
            tree.selection().unselect_all();
            self.imp().want_select.borrow_mut().take();
            return;
        }
        tree.selection().unselect_all();
        *self.imp().want_select.borrow_mut() = Some(path);
        self.advance_reveal();
        if !self.is_busy() {
            self.imp().want_select.borrow_mut().take();
        }
    }

    /// Walk from the root towards the wanted path, expanding folders; stops where a folder
    /// still has to load (the listing's completion resumes here).
    pub(super) fn advance_reveal(&self) {
        let Some(target) = self.imp().want_select.borrow().clone() else {
            return;
        };
        let w = self.w();
        let root = self.root();
        let Ok(relative) = target.strip_prefix(&root) else {
            return;
        };
        let Some(mut row) = w.store.iter_first() else {
            return;
        };
        for component in relative.components() {
            let tree_path = util::path_of(&w.store, &row);
            if !w.tree.row_expanded(&tree_path) {
                w.tree.expand_row(&tree_path, false);
            }
            let name = component.as_os_str().to_string_lossy();
            let Some(child) = w.store.iter_children(Some(&row)) else {
                return;
            };
            loop {
                if util::string(&w.store, &child, 1) == name {
                    break;
                }
                if !w.store.iter_next(&child) {
                    return;
                }
            }
            row = child;
        }
        self.imp().want_select.borrow_mut().take();
        self.select_iter(&row);
    }
}
