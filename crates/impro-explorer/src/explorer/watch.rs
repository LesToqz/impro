//! Watching the expanded folders, so files created, removed or renamed by other
//! programs show up without a manual refresh.
//!
//! One `GFileMonitor` per expanded folder (the root included), at most [`MAX_WATCHES`],
//! shallowest first. A burst of events becomes one re-listing of the folder, spaced out
//! in proportion to its size; the listing merges into the tree in place like any other.
use super::*;
use gio::prelude::*;
use std::time::Duration;

/// Folders watched at once (each costs an inotify watch).
const MAX_WATCHES: usize = 128;
/// Quiet time between the first event of a burst and the re-listing.
const BASE_DELAY: Duration = Duration::from_millis(250);
/// Longest delay for a very large folder.
const MAX_DELAY: Duration = Duration::from_secs(5);

impl ExplorerPanel {
    /// Watch the expanded folders for changes made elsewhere (default `true`).
    pub fn set_watch_folders(&self, enabled: bool) {
        let imp = self.imp();
        if imp.watch_folders.replace(enabled) == enabled || imp.destroyed.get() {
            return;
        }
        self.sync_watches();
    }

    pub fn watch_folders(&self) -> bool {
        self.imp().watch_folders.get()
    }

    /// The folders being watched, shallowest first.
    #[doc(hidden)]
    pub fn watched_folders(&self) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = self.imp().dir_monitors.borrow().keys().cloned().collect();
        folders.sort_by_key(|path| (path.components().count(), path.clone()));
        folders
    }

    /// Bring the monitors in line with the expanded folders, once the main loop is idle
    /// (expanding many folders at once asks for a single pass).
    pub(super) fn schedule_watch_sync(&self) {
        let imp = self.imp();
        if imp.dir_sync.borrow().is_some() || imp.destroyed.get() {
            return;
        }
        let weak = self.downgrade();
        let source = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            if let Some(panel) = weak.upgrade() {
                panel.imp().dir_sync.take();
                panel.sync_watches();
            }
            glib::ControlFlow::Break
        });
        *imp.dir_sync.borrow_mut() = Some(source);
    }

    /// The folders inside folded rows below the expanded folders, each with the folder whose
    /// listing made that row: a change in `a` or `a/b` of the row `a/b/c` can split or extend
    /// the fold, which only re-listing the row's parent shows. The innermost folder counts
    /// while its row is collapsed (an expanded one is watched for its own listing).
    fn folded_folders(&self) -> Vec<(PathBuf, PathBuf)> {
        let w = self.w();
        let parents = RefCell::new(Vec::new());
        w.tree.map_expanded_rows(|_, path| {
            if let Some(iter) = w.store.iter(path) {
                parents.borrow_mut().push(iter);
            }
        });
        let mut found = Vec::new();
        for parent in parents.into_inner() {
            let listed = util::decode_path(&util::string(&w.store, &parent, 2));
            let mut row = w.store.iter_children(Some(&parent));
            while let Some(child) = row {
                // Only folded rows show a separator in their name.
                if util::string(&w.store, &child, 1).contains(std::path::MAIN_SEPARATOR) {
                    let inner = util::decode_path(&util::string(&w.store, &child, 2));
                    let open = w
                        .store
                        .path(&child)
                        .is_some_and(|path| w.tree.row_expanded(&path));
                    let mut folder = if open {
                        inner.parent().map(Path::to_path_buf)
                    } else {
                        Some(inner.clone())
                    };
                    while let Some(dir) = folder {
                        if dir == listed || !dir.starts_with(&listed) {
                            break;
                        }
                        folder = dir.parent().map(Path::to_path_buf);
                        found.push((dir, listed.clone()));
                    }
                }
                row = util::next_iter(&w.store, &child);
            }
        }
        found
    }

    fn sync_watches(&self) {
        let imp = self.imp();
        if self.is_destroyed() {
            return;
        }
        // Watched folder -> folder to re-list when it changes.
        let mut targets: HashMap<PathBuf, PathBuf> = HashMap::new();
        if imp.watch_folders.get() {
            for dir in self.expanded_paths() {
                targets.insert(dir.clone(), dir);
            }
            if imp.auto_fold.get() {
                for (dir, listed) in self.folded_folders() {
                    targets.entry(dir).or_insert(listed);
                }
            }
        }
        let mut wanted: Vec<PathBuf> = targets.keys().cloned().collect();
        wanted.sort_by_key(|path| (path.components().count(), path.clone()));
        wanted.truncate(MAX_WATCHES);
        let wanted: HashSet<PathBuf> = wanted.into_iter().collect();
        targets.retain(|dir, _| wanted.contains(dir));
        *imp.watch_targets.borrow_mut() = targets;
        let stale: Vec<PathBuf> = imp
            .dir_monitors
            .borrow()
            .keys()
            .filter(|dir| !wanted.contains(*dir))
            .cloned()
            .collect();
        for dir in stale {
            if let Some(monitor) = imp.dir_monitors.borrow_mut().remove(&dir) {
                monitor.cancel();
            }
            // Changes seen but not yet listed would be missed on the next expansion.
            if imp.dir_dirty.borrow_mut().remove(&dir) {
                imp.loaded.borrow_mut().remove(&dir);
            }
        }
        let mut order: Vec<&PathBuf> = wanted.iter().collect();
        order.sort_by_key(|path| (path.components().count(), (*path).clone()));
        for dir in order {
            if imp.dir_monitors.borrow().contains_key(dir) {
                continue;
            }
            let monitor = gio::File::for_path(dir)
                .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE);
            let Ok(monitor) = monitor else { continue };
            let (weak, folder) = (self.downgrade(), dir.clone());
            monitor.connect_changed(move |_, file, other, event| {
                use gio::FileMonitorEvent as Event;
                if !matches!(
                    event,
                    Event::Created
                        | Event::Deleted
                        | Event::MovedIn
                        | Event::MovedOut
                        | Event::Renamed
                        | Event::Moved
                ) {
                    return;
                }
                if let Some(panel) = weak.upgrade() {
                    panel.folder_changed(&folder, file, other);
                }
            });
            imp.dir_monitors.borrow_mut().insert(dir.clone(), monitor);
        }
    }

    fn folder_changed(&self, dir: &Path, file: &gio::File, other: Option<&gio::File>) {
        let imp = self.imp();
        if imp.destroyed.get() {
            return;
        }
        let hidden = |file: &gio::File| {
            file.basename()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        };
        if !imp.show_hidden.get() && hidden(file) && other.is_none_or(hidden) {
            return;
        }
        // A folder inside a folded row is re-listed through the folder that lists the row.
        let target = imp
            .watch_targets
            .borrow()
            .get(dir)
            .cloned()
            .unwrap_or_else(|| dir.to_path_buf());
        imp.dir_dirty.borrow_mut().insert(target.clone());
        // With folding, a folder that now holds a single folder (or no longer does) changes
        // shape in the listing of the folder above it: re-list that one too.
        if imp.auto_fold.get() && target == dir {
            let parent = self.find_exact_row(dir).and_then(|row| {
                let store = &self.w().store;
                let parent = store.iter_parent(&row)?;
                Some(util::decode_path(&util::string(store, &parent, 2)))
            });
            if let Some(parent) = parent {
                imp.dir_dirty.borrow_mut().insert(parent);
            }
        }
        if imp.dir_timeout.borrow().is_some() {
            return;
        }
        // A big folder takes long to list and merge: look at it less often.
        let rows = self
            .find_exact_row(&target)
            .map_or(0, |row| self.w().store.iter_n_children(Some(&row)));
        let delay = (BASE_DELAY + Duration::from_millis(rows.max(0) as u64 / 5)).min(MAX_DELAY);
        let weak = self.downgrade();
        let source = glib::timeout_add_local_once(delay, move || {
            if let Some(panel) = weak.upgrade() {
                panel.imp().dir_timeout.take();
                panel.relist_changed_folders();
            }
        });
        *imp.dir_timeout.borrow_mut() = Some(source);
    }

    fn relist_changed_folders(&self) {
        let imp = self.imp();
        if self.is_destroyed() {
            return;
        }
        let mut dirty: Vec<PathBuf> = imp.dir_dirty.take().into_iter().collect();
        dirty.sort_by_key(|path| path.components().count());
        for dir in dirty {
            if !imp.dir_monitors.borrow().contains_key(&dir) {
                continue;
            }
            let expanded = self.find_exact_row(&dir).is_some_and(|row| {
                let path = util::path_of(&self.w().store, &row);
                self.w().tree.row_expanded(&path)
            });
            if expanded {
                if imp.auto_fold.get() {
                    self.remember_expansion();
                }
                self.request_scan(&dir);
            }
        }
    }

    pub(super) fn stop_watches(&self) {
        let imp = self.imp();
        if let Some(source) = imp.dir_sync.take() {
            source.remove();
        }
        if let Some(source) = imp.dir_timeout.take() {
            source.remove();
        }
        for (_, monitor) in imp.dir_monitors.take() {
            monitor.cancel();
        }
        imp.dir_dirty.take();
        imp.watch_targets.take();
    }
}
