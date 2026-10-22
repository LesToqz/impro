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

    fn sync_watches(&self) {
        let imp = self.imp();
        if self.is_destroyed() {
            return;
        }
        let mut wanted: Vec<PathBuf> = if imp.watch_folders.get() {
            self.expanded_paths().into_iter().collect()
        } else {
            Vec::new()
        };
        wanted.sort_by_key(|path| (path.components().count(), path.clone()));
        wanted.truncate(MAX_WATCHES);
        let wanted: HashSet<PathBuf> = wanted.into_iter().collect();
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
        imp.dir_dirty.borrow_mut().insert(dir.to_path_buf());
        if imp.dir_timeout.borrow().is_some() {
            return;
        }
        // A big folder takes long to list and merge: look at it less often.
        let rows = self
            .find_row(dir)
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
            let expanded = self.find_row(&dir).is_some_and(|row| {
                let path = util::path_of(&self.w().store, &row);
                self.w().tree.row_expanded(&path)
            });
            if expanded {
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
    }
}
