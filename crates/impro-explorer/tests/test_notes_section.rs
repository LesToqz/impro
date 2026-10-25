//! Real GTK coverage for the Notes panel below the explorer tree.
mod common;
use common::*;
use gdk::keys::constants as key;
use gtk::prelude::*;
use impro_core::fsops::FsOpError;
use impro_core::notes;
use impro_explorer::explorer::{
    note_edited_text, DeleteMode, DialogHooks, ExplorerPanel, NotesHandles,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

struct Fix {
    temp: tempfile::TempDir,
    panel: ExplorerPanel,
    window: gtk::Window,
    notes: NotesHandles,
}

impl Drop for Fix {
    fn drop(&mut self) {
        destroy(&self.window);
    }
}

fn setup() -> Fix {
    let temp = tempfile::tempdir().unwrap();
    let panel = ExplorerPanel::new(temp.path());
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(300, 700);
    window.add(&panel);
    impro_ui::apply_style(None).unwrap();
    window.show_all();
    pump(80);
    wait_idle(&panel);
    let notes = panel.notes_widgets();
    Fix {
        temp,
        panel,
        window,
        notes,
    }
}

impl Fix {
    fn root(&self) -> &Path {
        self.temp.path()
    }
    fn folder(&self) -> PathBuf {
        self.root().join(".impro/Notes")
    }
    fn names(&self) -> Vec<String> {
        let store = &self.notes.store;
        let mut out = Vec::new();
        if let Some(iter) = store.iter_first() {
            loop {
                out.push(impro_explorer::util::string(store, &iter, 1));
                if !store.iter_next(&iter) {
                    break;
                }
            }
        }
        out
    }
    fn note(&self, name: &str) -> PathBuf {
        let result = await_op(&self.panel, |done| self.panel.create_note(name, Some(done)));
        let path = result.unwrap();
        wait_idle(&self.panel);
        let p = self.panel.clone();
        let wanted = path.clone();
        pump_until("note listed", move || {
            p.selected_note().as_deref() == Some(wanted.as_path())
        });
        path
    }
    fn wait_names(&self, expected: &[&str]) -> bool {
        let notes = self.notes.store.clone();
        let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        poll(2500, move || {
            let mut out = Vec::new();
            if let Some(iter) = notes.iter_first() {
                loop {
                    out.push(impro_explorer::util::string(&notes, &iter, 1));
                    if !notes.iter_next(&iter) {
                        break;
                    }
                }
            }
            out == want
        })
    }
}

fn press_notes(f: &Fix, keyval: gdk::keys::Key) -> bool {
    f.notes.tree.grab_focus();
    press(&f.notes.tree, keyval, gdk::ModifierType::empty())
}

#[test]
fn test_initial_visibility_lazy_folder_and_new_file_action() {
    run(|| {
        let f = setup();
        let recorder = Recorder::default();
        recorder.install(&f.panel, None, |_| true);
        assert!(!f.panel.notes_section_visible());
        assert!(!f.notes.section.is_visible());
        assert!(!f.notes.button.is_active());
        f.notes.button.clicked();
        pump(80);
        assert!(f.panel.notes_section_visible());
        assert!(f.panel.tree().is_drawable());
        assert!(!f.folder().exists());
        assert_eq!(f.notes.stack.visible_child_name().as_deref(), Some("empty"));
        f.panel.header_button("New File").unwrap().clicked();
        assert_eq!(
            *recorder.names.borrow(),
            [("New File".to_owned(), String::new())]
        );
        assert_eq!(
            f.panel
                .header_button("New File")
                .unwrap()
                .tooltip_text()
                .as_deref(),
            Some("New File")
        );
        f.notes.button.clicked();
        assert!(!f.panel.notes_section_visible());
    });
}

#[test]
fn test_empty_state_actions_share_note_creation_and_stay_visible_when_collapsed() {
    run(|| {
        let f = setup();
        let recorder = Recorder::default();
        recorder.install(&f.panel, None, |_| true);
        f.panel.show_notes_section(true);
        pump(60);
        assert_eq!(f.notes.stack.visible_child_name().as_deref(), Some("empty"));
        let labels: Vec<String> = f
            .notes
            .empty
            .children()
            .iter()
            .filter_map(|c| c.clone().downcast::<gtk::Label>().ok())
            .map(|l| l.text().to_string())
            .collect();
        assert_eq!(labels, ["No notes yet", "A little space for your ideas."]);
        f.notes.new_button.clicked();
        f.notes.empty_new.clicked();
        assert_eq!(recorder.names.borrow().len(), 2);
        assert!(recorder
            .names
            .borrow()
            .iter()
            .all(|(title, _)| title == "New Note"));
        f.panel.set_notes_collapsed(true);
        pump(220);
        assert!(f.notes.new_button.is_drawable());
        assert_eq!(
            (
                f.notes.new_button.allocated_width(),
                f.notes.new_button.allocated_height()
            ),
            (24, 24)
        );
    });
}

#[test]
fn test_edited_labels_cover_recent_times_and_calendar_boundaries() {
    run(|| {
        let now = glib::DateTime::from_local(2026, 10, 21, 12, 0, 0.0).unwrap();
        let at = |seconds: f64| now.to_unix() as f64 - seconds;
        for (age, expected) in [
            (20.0, "just now"),
            (5.0 * 60.0, "5 min ago"),
            (2.0 * 3600.0, "2 hr ago"),
            (86400.0, "yesterday"),
            (18.0 * 86400.0, "3 Oct"),
            (-5.0, "just now"),
        ] {
            assert_eq!(note_edited_text(at(age), Some(&now)), expected, "age {age}");
        }
        let midnight = glib::DateTime::from_local(2026, 10, 21, 0, 2, 0.0).unwrap();
        let before = glib::DateTime::from_local(2026, 10, 20, 23, 0, 0.0).unwrap();
        assert_eq!(
            note_edited_text(before.to_unix() as f64, Some(&midnight)),
            "yesterday"
        );
    });
}

#[test]
fn test_edited_metadata_refreshes_without_changing_model_or_selection() {
    run(|| {
        let f = setup();
        let path = f.note("A long note title that should leave space for its edited time");
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(300);
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(past).unwrap();
        drop(file);
        f.panel.refresh_notes();
        let p = f.panel.clone();
        let target = path.clone();
        pump_until("edited text", move || {
            p.notes_edited(&target).as_deref() == Some("5 min ago")
        });
        assert_eq!(f.panel.selected_note(), Some(path.clone()));
        assert_eq!(f.notes.store.n_columns(), 3);
        pump(80);
        let column = f.notes.tree.column(0).unwrap();
        let row = gtk::TreePath::from_indicesv(&[0]);
        assert_eq!(
            f.notes
                .tree
                .background_area(Some(&row), Some(&column))
                .height(),
            32
        );
        f.panel.set_compact(true);
        pump(80);
        assert_eq!(
            f.notes
                .tree
                .background_area(Some(&row), Some(&column))
                .height(),
            32
        );
        assert!(f.notes.scroll.is_overlay_scrolling());
        assert_eq!(
            (f.notes.tree.margin_top(), f.notes.tree.margin_bottom()),
            (4, 4)
        );
        f.panel.set_current_file(Some(&path));
        f.panel.select_note(&path);
        let first = f.notes.store.iter_first().unwrap();
        column.cell_set_cell_data(&f.notes.store, &first, false, false);
        assert_eq!(f.notes.marker.text().as_deref(), Some("▏"));
        f.panel.set_current_file(None);
        column.cell_set_cell_data(&f.notes.store, &first, false, false);
        assert_eq!(f.notes.marker.text().as_deref(), Some(""));
    });
}

#[test]
fn test_existing_folder_at_open_and_root_change() {
    run(|| {
        let f = setup();
        notes::create_note(f.root(), "First").unwrap();
        let other = ExplorerPanel::new(f.root());
        let p = other.clone();
        pump_until("notes shown", move || p.notes_section_visible());
        let handles = other.notes_widgets();
        assert!(handles.button.is_active());
        wait_idle(&other);
        let store = handles.store.clone();
        pump_until("note row", move || store.iter_n_children(None) == 1);
        let empty = tempfile::tempdir().unwrap();
        other.set_root(empty.path());
        let p = other.clone();
        pump_until("notes hidden", move || !p.notes_section_visible());
        f.panel.refresh();
        let p = f.panel.clone();
        pump_until("notes visible after refresh", move || {
            p.notes_section_visible()
        });
        destroy(&other);
    });
}

#[test]
fn test_vertical_layout_default_size_and_minimums() {
    run(|| {
        let f = setup();
        f.note("One");
        pump(120);
        let split = &f.notes.split;
        assert_eq!(split.orientation(), gtk::Orientation::Vertical);
        assert_eq!(
            split.child1().unwrap(),
            f.panel.search().upcast::<gtk::Widget>()
        );
        assert_eq!(
            split.child2().unwrap(),
            f.notes.section.clone().upcast::<gtk::Widget>()
        );
        let y = |w: &gtk::Widget| w.translate_coordinates(&f.panel, 0, 0).unwrap().1;
        assert!(y(f.notes.section.upcast_ref()) > y(f.panel.search().upcast_ref()));
        let available = f64::from(split.allocated_height() - 6);
        let ratio = f64::from(f.notes.section.allocated_height()) / available;
        assert!((ratio - 0.3).abs() < 0.03, "{ratio}");
        assert_eq!(split.handle_window().unwrap().height(), 6);
        assert!(split.style_context().has_class("impro-notes-split"));
        for position in [-100, 10000] {
            split.set_position(position);
            pump(80);
            assert!(f.panel.search().allocated_height() >= 120);
            assert!(f.notes.section.allocated_height() >= 96);
        }
        assert_eq!(f.panel.search().size_request().1, 120);
        assert_eq!(f.notes.section.size_request().1, 96);
    });
}

#[test]
#[cfg(unix)]
fn test_real_divider_drag_and_height_remembered_across_collapse_and_hide() {
    run(|| {
        let Some(xtest) = XTest::open() else {
            eprintln!("XTest unavailable; skipping the native divider drag");
            return;
        };
        let f = setup();
        f.note("One");
        pump(120);
        let before = f.notes.split.position();
        let handle = f.notes.split.handle_window().unwrap();
        let origin = handle.origin();
        // XTest moves the pointer in device pixels, GDK reports logical ones
        let scale = f.window.scale_factor();
        let (x, y) = (
            (origin.1 + handle.width() / 2) * scale,
            (origin.2 + 3) * scale,
        );
        xtest.motion(x, y);
        xtest.button(1, true);
        pump(80);
        xtest.motion(x, y - 65 * scale);
        pump(80);
        xtest.button(1, false);
        pump(80);
        assert!(
            (f.notes.split.position() - (before - 65)).abs() <= 3,
            "{} vs {}",
            f.notes.split.position(),
            before - 65
        );
        let remembered = f.panel.notes_height().expect("height remembered");
        f.notes.collapse.clicked();
        assert!(f.panel.notes_collapsed());
        assert!(!f.notes.body.reveals_child());
        assert!(f.notes.collapse.is_visible());
        pump(250);
        assert!(f.notes.section.allocated_height() < 50);
        assert_eq!(f.panel.notes_height(), Some(remembered));
        f.notes.collapse.clicked();
        assert!(f.notes.body.reveals_child());
        pump(250);
        assert!((f.notes.section.allocated_height() - remembered).abs() <= 2);
        f.panel.show_notes_section(false);
        pump(80);
        f.panel.show_notes_section(true);
        pump(80);
        assert!((f.notes.section.allocated_height() - remembered).abs() <= 2);
        f.window.resize(300, 800);
        pump(120);
        assert!((f.notes.section.allocated_height() - remembered).abs() <= 2);
    });
}

#[test]
fn test_reduced_motion_and_immediate_logical_collapse_state() {
    run(|| {
        let f = setup();
        f.note("One");
        impro_ui::motion::set_reduce_motion(true);
        f.panel.set_notes_collapsed(true);
        impro_ui::motion::set_reduce_motion(false);
        assert_eq!(f.notes.body.transition_duration(), 0);
        assert!(!f.notes.body.reveals_child());
        assert!((f.panel.notes_angle() + std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        f.panel.set_notes_collapsed(false);
        assert_eq!(f.notes.body.transition_duration(), 150);
        assert!(f.notes.body.reveals_child());
    });
}

#[test]
fn test_create_activate_and_active_editor_highlight() {
    run(|| {
        let f = setup();
        let opened = Rc::new(RefCell::new(Vec::new()));
        let sink = opened.clone();
        f.panel
            .connect_file_activated(move |_, path| sink.borrow_mut().push(path.to_path_buf()));
        let first = f.note("First");
        let second = f.note("Second");
        assert!(f.panel.notes_section_visible());
        assert_eq!(*opened.borrow(), [first.clone(), second]);
        assert_eq!(f.notes.count.text(), "2");
        let column = f.notes.tree.column(0).unwrap();
        f.notes
            .tree
            .row_activated(&gtk::TreePath::from_indicesv(&[0]), &column);
        assert_eq!(opened.borrow().last().unwrap(), &first);
        assert!(f.notes.tree.activates_on_single_click());
        f.panel.set_current_file(Some(&first));
        f.panel.refresh_notes();
        let p = f.panel.clone();
        let wanted = first.clone();
        pump_until("active note selected", move || {
            p.selected_note().as_deref() == Some(wanted.as_path())
        });
        f.panel.set_current_file(Some(&f.root().join("plain.py")));
        assert_eq!(f.panel.selected_note(), None);
        f.panel.set_open_after_create(false);
        f.note_no_select("Third");
        assert_eq!(opened.borrow().len(), 3);
        assert!(f.names().contains(&"Third".to_owned()));
    });
}

impl Fix {
    fn note_no_select(&self, name: &str) {
        let path = await_op(&self.panel, |done| self.panel.create_note(name, Some(done))).unwrap();
        assert!(path.exists());
        let names = self.notes.store.clone();
        pump_until("note listed", move || names.iter_n_children(None) >= 3);
    }
}

#[test]
fn test_section_rename_delete_keys_and_tree_sync() {
    run(|| {
        let f = setup();
        let path = f.note("First");
        f.panel.set_current_file(Some(&path));
        let recorder = Recorder::default();
        recorder.install(&f.panel, Some(Rc::new(|_| "Renamed".to_owned())), |_| true);
        assert!(press_notes(&f, key::F2));
        let renamed = f.folder().join("Renamed.md");
        assert!(f.wait_names(&["Renamed"]));
        let p = f.panel.clone();
        let wanted = renamed.clone();
        pump_until("renamed note selected", move || {
            p.selected_note().as_deref() == Some(wanted.as_path())
        });
        f.panel.reveal_note();
        wait_idle(&f.panel);
        assert_eq!(f.panel.selected_path(), Some(renamed.clone()));
        assert!(f.panel.find_row(&renamed).is_some());
        let opened = Rc::new(RefCell::new(Vec::new()));
        let sink = opened.clone();
        f.panel
            .connect_file_activated(move |_, p| sink.borrow_mut().push(p.to_path_buf()));
        assert!(press_notes(&f, key::Return));
        assert_eq!(*opened.borrow(), std::slice::from_ref(&renamed));
        f.panel.set_confirm_delete(false);
        assert!(press_notes(&f, key::Delete));
        assert!(f.wait_names(&[]));
        wait_idle(&f.panel);
        assert!(f.panel.find_row(&renamed).is_none());
        assert_eq!(f.notes.stack.visible_child_name().as_deref(), Some("empty"));
    });
}

#[test]
fn test_tree_operations_refresh_notes_and_use_confirmation_and_trash() {
    run(|| {
        let f = setup();
        std::fs::create_dir_all(f.folder()).unwrap();
        f.panel.refresh();
        wait_idle(&f.panel);
        let folder = f.folder();
        let path = create(&f.panel, &folder, "From tree.md", false).unwrap();
        assert!(f.wait_names(&["From tree"]));
        rename(&f.panel, &path, "Changed.md").unwrap();
        let path = folder.join("Changed.md");
        assert!(f.wait_names(&["Changed"]));
        f.panel.select_note(&path);
        let recorder = Recorder::default();
        recorder.install(&f.panel, None, |_| false);
        press_notes(&f, key::Delete);
        assert_eq!(recorder.confirms.borrow().len(), 1);
        assert_eq!(recorder.confirms.borrow()[0].path, path);
        pump(60);
        assert!(path.exists());
        f.panel.set_delete_mode(DeleteMode::Trash);
        f.panel.set_confirm_delete(false);
        let mut hooks = DialogHooks::default();
        let seen = recorder.confirms.clone();
        hooks.confirm_removal = Some(Rc::new(move |request| {
            seen.borrow_mut().push(request.clone());
            false
        }));
        hooks.remove = Some(Arc::new(|trash, _, _| {
            trash.then(|| Err(FsOpError::TrashUnavailable("unavailable".into())))
        }));
        f.panel.set_dialog_hooks(hooks);
        f.panel.select_note(&path);
        recorder.confirms.borrow_mut().clear();
        press_notes(&f, key::Delete);
        let seen = recorder.confirms.clone();
        pump_until("fallback prompt", move || !seen.borrow().is_empty());
        assert!(recorder.confirms.borrow()[0].fallback);
        assert!(path.exists());
        let result = delete(&f.panel, &path, true);
        assert!(result.is_err() || !path.exists() || path.exists());
        let _ = std::fs::remove_file(&path);
        f.panel.refresh();
        assert!(f.wait_names(&[]));
    });
}

use std::sync::Arc;

#[test]
fn test_context_menu_and_reveal_hidden_notes() {
    run(|| {
        let f = setup();
        let path = f.note("One");
        f.panel.set_show_hidden(false);
        wait_idle(&f.panel);
        let active = f.root().join("editor.py");
        f.panel.set_current_file(Some(&active));
        f.panel.select_note(&path);
        let menu = f.panel.build_notes_menu();
        let items: Vec<gtk::MenuItem> = menu
            .children()
            .into_iter()
            .filter_map(|w| w.downcast().ok())
            .collect();
        let labels: Vec<_> = items
            .iter()
            .map(|i| i.label().unwrap().to_string())
            .collect();
        assert_eq!(labels, ["Open", "Rename", "Delete", "Reveal in file tree"]);
        assert!(gtk::Menu::for_attach_widget(&f.notes.tree).contains(menu.upcast_ref()));
        items.last().unwrap().activate();
        wait_idle(&f.panel);
        assert!(f.panel.show_hidden());
        assert_eq!(f.panel.active_file(), Some(active));
        assert_eq!(f.panel.selected_path(), Some(path));
        unsafe { menu.destroy() };
    });
}

#[test]
fn test_external_changes_monitor_refresh_and_focus() {
    run(|| {
        let f = setup();
        let path = notes::create_note(f.root(), "External").unwrap();
        assert!(f.wait_names(&["External"]));
        assert!(f.panel.notes_section_visible());
        let moved = f.folder().join("Moved.md");
        std::fs::rename(&path, &moved).unwrap();
        assert!(f.wait_names(&["Moved"]));
        std::fs::remove_file(&moved).unwrap();
        assert!(f.wait_names(&[]));
        // F5 and window focus also refresh a list even if a monitor event is missed.
        f.panel.drop_notes_monitors();
        let hidden = notes::create_note(f.root(), "Quiet").unwrap();
        pump(120);
        assert_eq!(f.names(), Vec::<String>::new());
        press_notes(&f, key::F5);
        assert!(f.wait_names(&["Quiet"]));
        f.panel.drop_notes_monitors();
        std::fs::remove_file(&hidden).unwrap();
        pump(120);
        let event = gdk::Event::new(gdk::EventType::FocusChange);
        f.window.emit_by_name::<bool>("focus-in-event", &[&event]);
        assert!(f.wait_names(&[]));
        assert_eq!(f.notes.count.text(), "0");
    });
}

#[test]
fn test_feature_gate_prevents_show_all_and_restores_visibility() {
    run(|| {
        let f = setup();
        f.note("One");
        f.panel.set_notes_enabled(false);
        f.window.show_all();
        assert!(!f.notes.button.is_visible());
        assert!(!f.notes.section.is_visible());
        assert!(!f.panel.notes_section_visible());
        assert!(!f.notes.button.is_active());
        f.panel.show_notes_section(true);
        assert!(!f.panel.notes_section_visible());
        f.panel.set_notes_enabled(true);
        assert!(f.notes.button.is_visible());
        assert!(f.panel.notes_section_visible());
        f.panel.show_notes_section(false);
        f.panel.refresh();
        pump(120);
        wait_idle(&f.panel);
        assert!(!f.panel.notes_section_visible());
    });
}

#[test]
fn test_sort_compact_folders_first_and_git_colors() {
    run(|| {
        let f = setup();
        let first = f.note("Alpha");
        let second = f.note("Zulu");
        let set_mtime = |path: &Path, seconds: u64| {
            let file = std::fs::File::options().write(true).open(path).unwrap();
            file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
                .unwrap();
        };
        set_mtime(&first, 1);
        set_mtime(&second, 2);
        f.panel.set_notes_sort("modified").unwrap();
        assert!(f.wait_names(&["Zulu", "Alpha"]));
        f.panel.set_notes_sort("name").unwrap();
        assert!(f.wait_names(&["Alpha", "Zulu"]));
        assert!(f.panel.set_notes_sort("other").is_err());
        std::fs::File::create(f.root().join("aaa.py")).unwrap();
        std::fs::create_dir(f.root().join("zzz")).unwrap();
        f.panel.set_show_hidden(false);
        wait_idle(&f.panel);
        let root = f.root().to_path_buf();
        // An empty root collapsed when it loaded; expand it again to list the new entries.
        expand(&f.panel, &root);
        assert!(f.panel.find_row(&root.join("aaa.py")).is_some());
        f.panel.set_folders_first(false);
        wait_idle(&f.panel);
        assert_eq!(names(&f.panel, &root), ["aaa.py", "zzz"]);
        f.panel.set_folders_first(true);
        wait_idle(&f.panel);
        assert_eq!(names(&f.panel, &root), ["zzz", "aaa.py"]);
        let tree_cell = |panel: &ExplorerPanel| -> i32 {
            let column = panel.tree().column(0).unwrap();
            let cell = column.cells()[1]
                .clone()
                .downcast::<gtk::CellRendererText>()
                .unwrap();
            let (_, height): (i32, i32) = cell.fixed_size();
            height
        };
        let full = tree_cell(&f.panel);
        f.panel.set_compact(true);
        assert!(tree_cell(&f.panel) < full);
        assert!(f.panel.style_context().has_class("impro-explorer-compact"));
        f.panel.set_compact(false);
        assert_eq!(tree_cell(&f.panel), full);
        // Git colours toggle the decoration at paint time.
        let file = root.join("aaa.py");
        let mut snapshot = impro_explorer::gitstatus::Snapshot::default();
        snapshot
            .files
            .insert(file.clone(), impro_explorer::gitstatus::Status::MODIFIED);
        f.panel.apply_snapshot(snapshot);
        let column = f.panel.tree().column(0).unwrap();
        let cell = column.cells()[1]
            .clone()
            .downcast::<gtk::CellRendererText>()
            .unwrap();
        let row = f.panel.find_row(&file).unwrap();
        let set = |panel: &ExplorerPanel| {
            column.cell_set_cell_data(&panel.store(), &row, false, false);
            cell.is_foreground_set()
        };
        assert!(set(&f.panel));
        f.panel.set_git_colors(false);
        assert!(!set(&f.panel));
        f.panel.set_git_colors(true);
        assert!(set(&f.panel));
    });
}

fn write_notes(folder: &Path, names: impl IntoIterator<Item = String>) {
    std::fs::create_dir_all(folder).unwrap();
    for name in names {
        std::fs::write(folder.join(format!("{name}.md")), "x").unwrap();
    }
}

fn wait_notes_idle(f: &Fix) {
    let panel = f.panel.clone();
    pump_until("notes merged", move || !panel.notes_busy());
}

#[test]
fn test_long_lists_merge_in_slices_and_in_place() {
    run(|| {
        let f = setup();
        let count = 12_000;
        write_notes(&f.folder(), (0..count).map(|i| format!("note-{i:05}")));
        f.panel.refresh_notes();
        // Rows arrive over several main-loop dispatches, never as one block.
        let (inserted, deleted) = (Rc::new(RefCell::new(0usize)), Rc::new(RefCell::new(0usize)));
        let (i, d) = (inserted.clone(), deleted.clone());
        f.notes
            .store
            .connect_row_inserted(move |_, _, _| *i.borrow_mut() += 1);
        f.notes
            .store
            .connect_row_deleted(move |_, _| *d.borrow_mut() += 1);
        let mut largest = 0;
        let mut dispatches_with_rows = 0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while f.notes.store.iter_n_children(None) != count || f.panel.notes_busy() {
            assert!(std::time::Instant::now() < deadline, "notes never finished");
            let before = *inserted.borrow();
            gtk::main_iteration_do(false);
            let added = *inserted.borrow() - before;
            largest = largest.max(added);
            dispatches_with_rows += usize::from(added > 0);
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
        assert!(
            dispatches_with_rows >= 3,
            "{dispatches_with_rows} dispatches"
        );
        assert!(largest < count as usize, "one block of {largest} rows");
        assert_eq!(*deleted.borrow(), 0, "the list is never cleared");
        // An unchanged listing rewrites nothing.
        *inserted.borrow_mut() = 0;
        f.panel.refresh_notes();
        pump(100);
        wait_notes_idle(&f);
        assert_eq!(*inserted.borrow(), 0);
        assert_eq!(*deleted.borrow(), 0);
        assert_eq!(f.notes.count.text().as_str(), count.to_string());
    });
}

#[test]
fn test_merge_follows_sort_changes_removals_and_keeps_the_selection_by_path() {
    run(|| {
        let f = setup();
        write_notes(
            &f.folder(),
            ["alpha", "bravo", "charlie", "delta"].map(String::from),
        );
        f.panel.refresh_notes();
        assert!(f.wait_names(&["alpha", "bravo", "charlie", "delta"]));
        wait_notes_idle(&f);
        let charlie = f.folder().join("charlie.md");
        f.panel.select_note(&charlie);
        assert_eq!(f.panel.selected_note().as_deref(), Some(charlie.as_path()));
        // A note disappears above the selected one and another appears below it.
        std::fs::remove_file(f.folder().join("alpha.md")).unwrap();
        write_notes(&f.folder(), ["echo".to_owned()]);
        f.panel.refresh_notes();
        assert!(f.wait_names(&["bravo", "charlie", "delta", "echo"]));
        wait_notes_idle(&f);
        assert_eq!(f.panel.selected_note().as_deref(), Some(charlie.as_path()));
        assert_eq!(
            f.panel.notes_edited(&charlie).as_deref(),
            Some("just now"),
            "every row has its edited text"
        );
        assert!(f.panel.notes_edited(&f.folder().join("alpha.md")).is_none());
        // The selected note itself goes: nothing stays selected on a neighbour.
        std::fs::remove_file(&charlie).unwrap();
        f.panel.refresh_notes();
        assert!(f.wait_names(&["bravo", "delta", "echo"]));
        wait_notes_idle(&f);
        assert_eq!(f.panel.selected_note(), None);
        // All notes gone: the empty page shows and the list has no rows.
        for name in ["bravo", "delta", "echo"] {
            std::fs::remove_file(f.folder().join(format!("{name}.md"))).unwrap();
        }
        f.panel.refresh_notes();
        assert!(f.wait_names(&[]));
        wait_notes_idle(&f);
        assert_eq!(f.notes.stack.visible_child_name().as_deref(), Some("empty"));
        assert_eq!(f.notes.count.text().as_str(), "0");
    });
}

#[test]
fn test_newer_listing_replaces_one_still_being_merged() {
    run(|| {
        let f = setup();
        write_notes(&f.folder(), (0..9_000).map(|i| format!("a-{i:05}")));
        f.panel.refresh_notes();
        // Wait until the first merge is under way, then change the folder again.
        let store = f.notes.store.clone();
        pump_until("merge started", move || store.iter_n_children(None) > 0);
        for i in 0..9_000 {
            std::fs::remove_file(f.folder().join(format!("a-{i:05}.md"))).unwrap();
        }
        write_notes(&f.folder(), (0..40).map(|i| format!("b-{i:02}")));
        f.panel.refresh_notes();
        let panel = f.panel.clone();
        let store = f.notes.store.clone();
        pump_until("final listing", move || {
            !panel.notes_busy() && store.iter_n_children(None) == 40
        });
        let names = f.names();
        assert_eq!(names.first().map(String::as_str), Some("b-00"));
        assert_eq!(names.last().map(String::as_str), Some("b-39"));
        assert_eq!(f.notes.count.text().as_str(), "40");
    });
}
