//! Git decorations: debounced worker scans, snapshot diffing and metadata watching.
use super::*;
use crate::gitstatus::{self, Kind};
use crate::pool::{self, CancelToken, Lane};
use gio::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Quiet period before a Git scan starts after a burst of triggers.
const DEBOUNCE: Duration = Duration::from_millis(150);

type Collector = Arc<dyn Fn(&Path) -> Snapshot + Send + Sync>;
static COLLECTOR: Mutex<Option<Collector>> = Mutex::new(None);

/// Test seam: replace the Git status collection (runs on a worker thread).
#[doc(hidden)]
pub fn set_git_collector(collector: Option<Collector>) {
    *COLLECTOR.lock().unwrap_or_else(|e| e.into_inner()) = collector;
}

fn collect(root: &Path, cancel: &CancelToken) -> Snapshot {
    let collector = COLLECTOR.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match collector {
        Some(collector) => collector(root),
        None => gitstatus::collect_status_with(root, &gitstatus::Limits::default(), Some(cancel)),
    }
}

fn basename_is_metadata(file: Option<&gio::File>) -> bool {
    file.and_then(|f| f.basename())
        .is_some_and(|name| matches!(name.to_str(), Some("index" | "HEAD" | "packed-refs")))
}

impl ExplorerPanel {
    /// Resolve the name colour of a row from the current snapshot at paint time.
    pub(super) fn git_cell_data(
        &self,
        cell: &gtk::CellRenderer,
        model: &gtk::TreeModel,
        iter: &gtk::TreeIter,
    ) {
        let imp = self.imp();
        let status = if imp.git_enabled.get() && imp.git_colors.get() {
            let snapshot = imp.git_snapshot.borrow().clone();
            let path = util::string(model, iter, 2);
            let statuses = if util::string(model, iter, 0) == "folder" {
                &snapshot.folders
            } else {
                &snapshot.files
            };
            statuses.get(Path::new(&path)).copied()
        } else {
            None
        };
        if let Some(status) = status {
            let role = match status.kind {
                Kind::New => "git_new",
                Kind::Modified => "git_modified",
                Kind::Conflict => "git_conflict",
            };
            let colour = self
                .w()
                .tree
                .style_context()
                .lookup_color(&format!("impro_{role}"))
                .or_else(|| gdk::RGBA::parse(&impro_core::themes::get_theme(None).ui[role]).ok());
            if let Some(colour) = colour {
                cell.set_property("foreground-rgba", colour);
            }
        }
        cell.set_property("foreground-set", status.is_some());
    }

    pub fn set_git_colors(&self, enabled: bool) {
        self.imp().git_colors.set(enabled);
        self.w().tree.queue_draw();
    }

    pub fn git_colors(&self) -> bool {
        self.imp().git_colors.get()
    }

    /// Debounce scans, immediately invalidating any in-flight result.
    pub fn refresh_git(&self) {
        let imp = self.imp();
        if imp.git_closed.get() {
            return;
        }
        imp.git_generation.set(imp.git_generation.get() + 1);
        if let Some(source) = imp.git_timeout.take() {
            source.remove();
        }
        if imp.git_enabled.get() {
            self.schedule_git();
        }
    }

    fn schedule_git(&self) {
        let weak = self.downgrade();
        let source = glib::timeout_add_local_once(DEBOUNCE, move || {
            if let Some(panel) = weak.upgrade() {
                panel.imp().git_timeout.take();
                panel.start_git();
            }
        });
        *self.imp().git_timeout.borrow_mut() = Some(source);
    }

    pub fn set_git_enabled(&self, enabled: bool) {
        let imp = self.imp();
        imp.git_enabled.set(enabled);
        if !enabled {
            imp.git_cancel.borrow().cancel();
            *imp.git_snapshot.borrow_mut() = Rc::new(Snapshot::default());
            self.set_git_monitor(None);
            self.w().tree.queue_draw();
        }
        self.refresh_git();
    }

    pub fn git_enabled(&self) -> bool {
        self.imp().git_enabled.get()
    }

    fn start_git(&self) {
        let imp = self.imp();
        if imp.git_running.get() || imp.git_closed.get() {
            return;
        }
        let Some(sender) = self.sender() else { return };
        imp.git_running.set(true);
        let generation = imp.git_generation.get();
        let root = imp.root.borrow().clone();
        let cancel = CancelToken::new();
        *imp.git_cancel.borrow_mut() = cancel.clone();
        pool::spawn(Lane::Interactive, move || {
            let snapshot = collect(&root, &cancel);
            let _ = sender.send(Msg::Git {
                generation,
                snapshot,
            });
        });
    }

    pub(super) fn deliver_git(&self, generation: u64, snapshot: Snapshot) {
        let imp = self.imp();
        if imp.git_closed.get() {
            return;
        }
        imp.git_running.set(false);
        if generation == imp.git_generation.get() && imp.git_enabled.get() {
            self.apply_snapshot(snapshot);
        } else if imp.git_enabled.get() && imp.git_timeout.borrow().is_none() {
            self.schedule_git();
        }
    }

    /// Install a snapshot, repainting only when some decoration actually changed.
    #[doc(hidden)]
    pub fn apply_snapshot(&self, snapshot: Snapshot) {
        let imp = self.imp();
        let git_dir = snapshot.git_dir.clone();
        let snapshot = Rc::new(snapshot);
        let changed = {
            let old = imp.git_snapshot.borrow();
            !old.diff(&snapshot).is_empty()
        };
        *imp.git_snapshot.borrow_mut() = snapshot;
        self.set_git_monitor(git_dir);
        if changed {
            self.w().tree.queue_draw();
        }
    }

    /// Direct delivery of a result for `generation` (stale or disabled ones are dropped).
    #[doc(hidden)]
    pub fn deliver_git_snapshot(&self, generation: u64, snapshot: Snapshot) {
        self.deliver_git(generation, snapshot);
    }

    #[doc(hidden)]
    pub fn git_generation(&self) -> u64 {
        self.imp().git_generation.get()
    }

    #[doc(hidden)]
    pub fn git_snapshot(&self) -> Rc<Snapshot> {
        self.imp().git_snapshot.borrow().clone()
    }

    #[doc(hidden)]
    pub fn git_running(&self) -> bool {
        self.imp().git_running.get()
    }

    #[doc(hidden)]
    pub fn git_timeout_pending(&self) -> bool {
        self.imp().git_timeout.borrow().is_some()
    }

    #[doc(hidden)]
    pub fn git_closed(&self) -> bool {
        self.imp().git_closed.get()
    }

    #[doc(hidden)]
    pub fn git_monitor_active(&self) -> bool {
        self.imp().git_monitor.borrow().is_some()
    }

    pub(super) fn set_git_monitor(&self, directory: Option<PathBuf>) {
        let imp = self.imp();
        if *imp.git_dir.borrow() == directory {
            return;
        }
        if let Some(monitor) = imp.git_monitor.take() {
            monitor.cancel();
        }
        *imp.git_dir.borrow_mut() = directory.clone();
        let Some(directory) = directory else { return };
        let monitor = gio::File::for_path(&directory)
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE);
        if let Ok(monitor) = monitor {
            let weak = self.downgrade();
            monitor.connect_changed(move |_, file, other, _| {
                if basename_is_metadata(Some(file)) || basename_is_metadata(other) {
                    if let Some(panel) = weak.upgrade() {
                        panel.refresh_git();
                    }
                }
            });
            *imp.git_monitor.borrow_mut() = Some(monitor);
        }
    }

    /// Watch the active file's folder, so an editor's atomic replacement keeps working.
    pub(super) fn watch_current_file(&self, path: Option<&Path>) {
        let imp = self.imp();
        if let Some(monitor) = imp.file_monitor.take() {
            monitor.cancel();
        }
        let Some(path) = path.filter(|p| !p.as_os_str().is_empty()) else {
            return;
        };
        let path = impro_core::fsops::absolute(path);
        let Some(parent) = path.parent() else { return };
        let monitor = gio::File::for_path(parent)
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE);
        if let Ok(monitor) = monitor {
            let weak = self.downgrade();
            monitor.connect_changed(move |_, file, other, _| {
                let hit = file.path().as_deref() == Some(path.as_path())
                    || other.and_then(|f| f.path()).as_deref() == Some(path.as_path());
                if hit {
                    if let Some(panel) = weak.upgrade() {
                        panel.refresh_git();
                    }
                }
            });
            *imp.file_monitor.borrow_mut() = Some(monitor);
        }
    }

    pub(super) fn stop_git(&self) {
        let imp = self.imp();
        imp.git_closed.set(true);
        if let Some(source) = imp.git_timeout.take() {
            source.remove();
        }
        imp.git_cancel.borrow().cancel();
        self.set_git_monitor(None);
        self.watch_current_file(None);
        self.stop_notes();
    }
}
