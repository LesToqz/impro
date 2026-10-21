//! Explorer decorations and worker lifecycle on a real GTK main context.
mod common;
use common::*;
use gtk::prelude::*;
use impro_core::themes;
use impro_explorer::explorer::{set_git_collector, ExplorerPanel};
use impro_explorer::gitstatus::{Kind, Snapshot, Status};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;
use std::time::Duration;

/// Tests swap `PATH`, so they never overlap with another Git user in this process.
struct Fix {
    temp: tempfile::TempDir,
    panel: ExplorerPanel,
    new: PathBuf,
    modified: PathBuf,
    nested: PathBuf,
}

impl Drop for Fix {
    fn drop(&mut self) {
        set_git_collector(None);
        destroy(&self.panel);
        impro_ui::apply_style(None).unwrap();
    }
}

impl Fix {
    fn root(&self) -> &Path {
        self.temp.path()
    }
    fn p(&self, name: &str) -> PathBuf {
        self.root().join(name)
    }
    fn git(&self, args: &[&str]) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(self.root())
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.invalid"])
            .args(args)
            .output()
            .unwrap()
            .status
            .success()
    }
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.p(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
    fn settled(&self) -> bool {
        let panel = self.panel.clone();
        poll(4000, move || !panel.git_running() && !panel.git_timeout_pending())
    }
    fn wait_status(&self, path: &Path, status: Option<Status>) -> bool {
        let (panel, path) = (self.panel.clone(), path.to_path_buf());
        poll(5000, move || panel.git_snapshot().files.get(&path).copied() == status)
    }
    fn text_cell(&self) -> (gtk::TreeViewColumn, gtk::CellRendererText) {
        let column = self.panel.tree().column(0).unwrap();
        let cell = column.cells()[1].clone().downcast::<gtk::CellRendererText>().unwrap();
        (column, cell)
    }
    fn assert_decoration(&self, path: &Path, kind: Option<Kind>, theme: &str) {
        let row = self.panel.find_row(path).unwrap_or_else(|| panic!("row {}", path.display()));
        let (column, cell) = self.text_cell();
        column.cell_set_cell_data(&self.panel.store(), &row, false, false);
        let text: Option<String> = cell.property("text");
        assert_eq!(text.as_deref(), path.file_name().and_then(|n| n.to_str()), "{}", path.display());
        let set: bool = cell.property("foreground-set");
        assert_eq!(set, kind.is_some(), "{} {kind:?}", path.display());
        if let Some(kind) = kind {
            let expected = gdk::RGBA::parse(&themes::get_theme(Some(theme)).ui[&format!("git_{}", kind.as_str())]).unwrap();
            let colour: gdk::RGBA = cell.property("foreground-rgba");
            assert_eq!(colour, expected, "{} {kind:?} in {theme}", path.display());
        }
    }
}

fn setup() -> Fix {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    impro_ui::apply_theme(themes::get_theme(Some("impro-dark")), None, None, None).unwrap();
    let setup = |args: &[&str]| {
        assert!(Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.invalid"])
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    };
    setup(&["init", "-q", "--initial-branch=base"]);
    std::fs::write(root.join("clean.txt"), "base\n").unwrap();
    setup(&["add", "."]);
    setup(&["commit", "-qm", "base"]);
    let new = root.join("untracked.py");
    std::fs::write(&new, "new\n").unwrap();
    let modified = root.join("clean.txt");
    std::fs::write(&modified, "changed\n").unwrap();
    let nested = root.join("collapsed/inner/nested.py");
    std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
    std::fs::write(&nested, "new\n").unwrap();
    let panel = ExplorerPanel::new(&root);
    let fix = Fix { temp, panel, new, modified, nested };
    assert!(fix.wait_status(&fix.new, Some(Status::NEW)), "initial scan");
    wait_idle(&fix.panel);
    fix
}

#[test]
fn test_name_colours_and_undecorated_row_reset() {
    run(|| {
        let f = setup();
        f.assert_decoration(&f.new, Some(Kind::New), "impro-dark");
        f.assert_decoration(&f.modified, Some(Kind::Modified), "impro-dark");
        let clean = f.write("unchanged.txt", "clean");
        assert!(f.git(&["add", "unchanged.txt"]));
        assert!(f.git(&["commit", "-qm", "clean row"]));
        f.panel.refresh();
        assert!(f.settled());
        wait_idle(&f.panel);
        f.assert_decoration(&clean, None, "impro-dark");
        assert_eq!(f.panel.store().n_columns(), 3);
    });
}

#[test]
fn test_long_name_uses_right_edge_without_a_badge_gap() {
    run(|| {
        let f = setup();
        let path = f.write(&format!("{}.py", "a_very_long_untracked_filename_".repeat(4)), "x");
        f.panel.refresh();
        assert!(f.wait_status(&path, Some(Status::NEW)));
        let window = gtk::OffscreenWindow::new();
        window.set_default_size(300, 400);
        window.add(&f.panel);
        window.show_all();
        let tree = f.panel.tree();
        let t = tree.clone();
        pump_until("tree allocated", move || t.allocated_width() > 1);
        wait_idle(&f.panel);
        let (column, cell) = f.text_cell();
        let cells = column.cells();
        assert_eq!(cells.len(), 2);
        assert!(cells[0].is::<gtk::CellRendererPixbuf>());
        assert_eq!(cells[1], cell.clone().upcast::<gtk::CellRenderer>());
        let row = f.panel.find_row(&path).unwrap();
        column.cell_set_cell_data(&f.panel.store(), &row, false, false);
        assert_eq!(cell.ellipsize(), pango::EllipsizeMode::End);
        let (start, width) = column.cell_get_position(&cell).unwrap();
        assert_eq!(start + width, column.width());
        assert!(column.width() < 300);
        f.assert_decoration(&path, Some(Kind::New), "impro-dark");
        window.remove(&f.panel);
        destroy(&window);
    });
}

#[test]
fn test_ignored_files_keep_normal_name_and_foreground() {
    run(|| {
        let f = setup();
        f.write(".gitignore", "*.ignored\n");
        let ignored = f.write("file.ignored", "x");
        f.panel.refresh();
        assert!(f.settled());
        wait_idle(&f.panel);
        f.assert_decoration(&ignored, None, "impro-dark");
    });
}

#[test]
fn test_explicit_refresh_after_staging() {
    run(|| {
        let f = setup();
        assert!(f.git(&["add", "untracked.py"]));
        f.panel.refresh_git();
        assert!(f.wait_status(&f.new, Some(Status::NEW)));
        f.assert_decoration(&f.new, Some(Kind::New), "impro-dark");
    });
}

#[test]
fn test_metadata_monitor_detects_staging_and_commit() {
    run(|| {
        let f = setup();
        assert!(f.panel.git_monitor_active());
        assert!(f.git(&["add", "untracked.py"]));
        assert!(f.wait_status(&f.new, Some(Status::NEW)));
        assert!(f.git(&["commit", "-qm", "new file"]));
        assert!(f.wait_status(&f.new, None));
        f.assert_decoration(&f.new, None, "impro-dark");
    });
}

#[test]
fn test_collapsed_folder_and_lazily_loaded_descendants() {
    run(|| {
        let f = setup();
        let folder = f.p("collapsed");
        assert!(!f.panel.is_loaded(&folder));
        f.assert_decoration(&folder, Some(Kind::New), "impro-dark");
        assert!(f.panel.find_row(&f.nested).is_none());
        f.panel.set_current_file(Some(&f.nested));
        wait_idle(&f.panel);
        f.assert_decoration(&f.nested, Some(Kind::New), "impro-dark");
        f.assert_decoration(&folder.join("inner"), Some(Kind::New), "impro-dark");
        f.assert_decoration(f.root(), Some(Kind::Modified), "impro-dark");
    });
}

#[test]
fn test_live_theme_switch_changes_foregrounds() {
    run(|| {
        let f = setup();
        for theme in ["dark-plus", "impro-light", "solarized-light", "impro-dark"] {
            impro_ui::apply_theme(themes::get_theme(Some(theme)), None, None, None).unwrap();
            f.assert_decoration(&f.new, Some(Kind::New), theme);
            f.assert_decoration(&f.modified, Some(Kind::Modified), theme);
        }
    });
}

#[test]
fn test_no_repo_root_switch_clears_immediately_and_drops_old_result() {
    run(|| {
        let f = setup();
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("plain.txt");
        std::fs::File::create(&path).unwrap();
        let old = (*f.panel.git_snapshot()).clone();
        let generation = f.panel.git_generation();
        f.panel.set_root(outside.path());
        wait_idle(&f.panel);
        f.assert_decoration(&path, None, "impro-dark");
        f.panel.deliver_git_snapshot(generation, old);
        assert!(f.panel.git_snapshot().files.is_empty());
        assert!(f.settled());
        f.assert_decoration(&path, None, "impro-dark");
        assert!(!f.panel.git_monitor_active());
    });
}

#[test]
fn test_missing_git_clears_existing_decorations() {
    run(|| {
        let f = setup();
        let saved = std::env::var_os("PATH");
        std::env::set_var("PATH", "");
        f.panel.refresh_git();
        let panel = f.panel.clone();
        let cleared = poll(4000, move || panel.git_snapshot().files.is_empty());
        let settled = f.settled();
        match saved {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        assert!(cleared && settled);
        f.assert_decoration(&f.new, None, "impro-dark");
        assert!(!f.panel.git_monitor_active());
    });
}

#[test]
fn test_renamed_and_conflicted_rows_keep_status_colours() {
    run(|| {
        let f = setup();
        assert!(f.git(&["checkout", "--", "clean.txt"]));
        assert!(f.git(&["mv", "clean.txt", "renamed.txt"]));
        f.panel.refresh();
        let renamed = f.p("renamed.txt");
        assert!(f.wait_status(&renamed, Some(Status::MODIFIED)));
        wait_idle(&f.panel);
        f.assert_decoration(&renamed, Some(Kind::Modified), "impro-dark");
        assert!(f.git(&["commit", "-qm", "rename"]));
        assert!(f.git(&["checkout", "-qb", "other"]));
        f.write("renamed.txt", "other\n");
        assert!(f.git(&["commit", "-qam", "other"]));
        assert!(f.git(&["checkout", "-q", "base"]));
        f.write("renamed.txt", "base change\n");
        assert!(f.git(&["commit", "-qam", "base change"]));
        f.git(&["merge", "other"]);
        f.panel.refresh_git();
        assert!(f.wait_status(&renamed, Some(Status::CONFLICT)));
        f.assert_decoration(&renamed, Some(Kind::Conflict), "impro-dark");
        f.assert_decoration(f.root(), Some(Kind::Conflict), "impro-dark");
    });
}

#[test]
fn test_disable_clears_colours_and_reenable_scans() {
    run(|| {
        let f = setup();
        let generation = f.panel.git_generation();
        let old = (*f.panel.git_snapshot()).clone();
        f.panel.set_git_enabled(false);
        f.assert_decoration(&f.new, None, "impro-dark");
        f.panel.deliver_git_snapshot(generation, old);
        f.assert_decoration(&f.new, None, "impro-dark");
        assert!(!f.panel.git_timeout_pending());
        f.panel.set_git_enabled(true);
        assert!(f.wait_status(&f.new, Some(Status::NEW)));
        f.assert_decoration(&f.new, Some(Kind::New), "impro-dark");
    });
}

#[test]
fn test_file_monitor_tracks_repeated_atomic_disk_replacements() {
    run(|| {
        let f = setup();
        f.panel.set_current_file(Some(&f.p("clean.txt")));
        assert!(f.git(&["checkout", "--", "clean.txt"]));
        assert!(f.wait_status(&f.modified, None));
        for text in ["first replacement", "second replacement"] {
            let replacement = f.p("replacement");
            std::fs::write(&replacement, text).unwrap();
            std::fs::rename(&replacement, f.p("clean.txt")).unwrap();
            assert!(f.wait_status(&f.modified, Some(Status::MODIFIED)), "{text}");
            assert!(f.git(&["checkout", "--", "clean.txt"]));
            assert!(f.wait_status(&f.modified, None), "{text}");
        }
    });
}

#[test]
fn test_create_rename_delete_and_refresh_button_schedule_scans() {
    run(|| {
        let f = setup();
        let created = create(&f.panel, f.root(), "created.py", false).unwrap();
        assert!(f.wait_status(&created, Some(Status::NEW)));
        wait_idle(&f.panel);
        f.assert_decoration(&created, Some(Kind::New), "impro-dark");
        let renamed = rename(&f.panel, &created, "renamed.py").unwrap();
        assert!(f.wait_status(&renamed, Some(Status::NEW)));
        assert!(f.wait_status(&created, None));
        delete(&f.panel, &renamed, false).unwrap();
        assert!(f.wait_status(&renamed, None));
        f.write("button.py", "x");
        f.panel.header_button("Refresh").unwrap().clicked();
        assert!(f.wait_status(&f.p("button.py"), Some(Status::NEW)));
    });
}

#[test]
fn test_bursts_coalesce_worker_is_off_main_thread_and_stale_result_dropped() {
    run(|| {
        let f = setup();
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let state: Arc<Mutex<Vec<ThreadId>>> = Arc::default();
        let new = f.new.clone();
        let latest = Snapshot {
            files: [(new.clone(), Status::NEW)].into_iter().collect(),
            ..Snapshot::default()
        };
        {
            let (gate, state, latest, new) = (gate.clone(), state.clone(), latest.clone(), new.clone());
            set_git_collector(Some(Arc::new(move |_| {
                let first = {
                    let mut calls = state.lock().unwrap();
                    calls.push(std::thread::current().id());
                    calls.len() == 1
                };
                if first {
                    let (lock, cv) = &*gate;
                    let _ = cv.wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(3), |open| !*open).unwrap();
                    return Snapshot {
                        files: [(new.clone(), Status::CONFLICT)].into_iter().collect(),
                        ..Snapshot::default()
                    };
                }
                latest.clone()
            })));
        }
        f.panel.refresh_git();
        let calls = state.clone();
        pump_until("first scan entered", move || !calls.lock().unwrap().is_empty());
        assert_ne!(state.lock().unwrap()[0], std::thread::current().id());
        for _ in 0..30 {
            f.panel.refresh_git();
        }
        // Main-loop work proceeds while Git is deliberately blocked.
        f.assert_decoration(&f.new, Some(Kind::New), "impro-dark");
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        let panel = f.panel.clone();
        let wanted = latest.clone();
        pump_until("latest snapshot", move || *panel.git_snapshot() == wanted && !panel.git_running());
        assert_eq!(state.lock().unwrap().len(), 2);
        f.assert_decoration(&f.new, Some(Kind::New), "impro-dark");
    });
}

#[test]
fn test_destroy_cancels_pending_and_running_delivery() {
    run(|| {
        let f = setup();
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let entered = Arc::new(Mutex::new(false));
        let finished = Arc::new(Mutex::new(false));
        {
            let (gate, entered, finished) = (gate.clone(), entered.clone(), finished.clone());
            set_git_collector(Some(Arc::new(move |_| {
                *entered.lock().unwrap() = true;
                let (lock, cv) = &*gate;
                let _ = cv.wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(3), |open| !*open).unwrap();
                *finished.lock().unwrap() = true;
                Snapshot::default()
            })));
        }
        f.panel.refresh_git();
        let flag = entered.clone();
        pump_until("scan entered", move || *flag.lock().unwrap());
        destroy(&f.panel);
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        let flag = finished.clone();
        pump_until("scan finished", move || *flag.lock().unwrap());
        pump(60);
        assert!(f.panel.git_closed());
        assert!(!f.panel.git_timeout_pending());
        f.panel.refresh_git();
        assert!(!f.panel.git_timeout_pending());
    });
}

#[test]
fn test_hidden_filter_and_search_have_no_git_renderer() {
    run(|| {
        let f = setup();
        let hidden = f.write(".hidden.py", "x");
        f.panel.refresh();
        assert!(f.wait_status(&hidden, Some(Status::NEW)));
        wait_idle(&f.panel);
        f.assert_decoration(&hidden, Some(Kind::New), "impro-dark");
        f.panel.set_show_hidden(false);
        wait_idle(&f.panel);
        assert!(f.panel.find_row(&hidden).is_none());
        f.panel.focus_search(Some("changed"));
        wait_idle(&f.panel);
        let (_, text_cell) = f.text_cell();
        let search_cells: Vec<gtk::CellRenderer> = f
            .panel
            .search()
            .tree()
            .columns()
            .iter()
            .flat_map(|c| c.cells())
            .collect();
        assert!(!search_cells.contains(text_cell.upcast_ref()));
    });
}

#[test]
fn test_unchanged_snapshot_does_not_repaint_but_changes_do() {
    run(|| {
        let f = setup();
        let window = gtk::OffscreenWindow::new();
        window.set_default_size(300, 400);
        window.add(&f.panel);
        window.show_all();
        pump(80);
        let draws = std::rc::Rc::new(std::cell::Cell::new(0));
        let counter = draws.clone();
        f.panel.tree().connect_draw(move |_, _| {
            counter.set(counter.get() + 1);
            glib::Propagation::Proceed
        });
        pump(80);
        let same = (*f.panel.git_snapshot()).clone();
        let before = draws.get();
        f.panel.apply_snapshot(same);
        pump(120);
        assert_eq!(draws.get(), before, "an identical snapshot must not repaint");
        let mut changed = (*f.panel.git_snapshot()).clone();
        changed.files.remove(&f.new);
        f.panel.apply_snapshot(changed);
        pump(120);
        assert!(draws.get() > before, "a changed decoration repaints");
        window.remove(&f.panel);
        destroy(&window);
    });
}
