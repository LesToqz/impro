//! Explorer search wired to an editor stand-in in a real window under Xvfb.
//!
//! The original wires the explorer to its editor inside `MainWindow`. The window and the
//! editor belong to other ports, so a small harness plays their part through exactly the
//! public explorer API the window uses: signals in, `set_current_file`, `set_text_provider`,
//! `focus_search`, `focus_tree` and `set_root` out.
mod common;
use common::*;
use gdk::keys::constants as key;
use gtk::prelude::*;
use impro_explorer::explorer::ExplorerPanel;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

struct Tab {
    path: PathBuf,
    buffer: gtk::TextBuffer,
    view: gtk::TextView,
}

/// What `MainWindow` and `EditorPanel` do with the explorer.
struct Harness {
    temp: tempfile::TempDir,
    window: gtk::Window,
    explorer: ExplorerPanel,
    editor_box: gtk::Box,
    tabs: Rc<RefCell<Vec<Tab>>>,
    current: Rc<RefCell<Option<usize>>>,
    selection: Rc<RefCell<Option<(usize, usize, usize)>>>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        destroy(&self.window);
    }
}

impl Harness {
    fn root(&self) -> PathBuf {
        self.temp.path().canonicalize().unwrap()
    }
    fn p(&self, name: &str) -> PathBuf {
        self.root().join(name)
    }
    fn search(&self) -> impro_explorer::SearchPanel {
        self.explorer.search()
    }
    fn groups(&self) -> Vec<PathBuf> {
        let store = self.search().store();
        let mut out = Vec::new();
        if let Some(iter) = store.iter_first() {
            loop {
                let path = impro_explorer::util::string(&store, &iter, 3);
                if impro_explorer::util::string(&store, &iter, 7) == "group" && !path.is_empty() {
                    out.push(PathBuf::from(path));
                }
                if !store.iter_next(&iter) {
                    break;
                }
            }
        }
        out
    }
    fn search_for(&self, query: &str) {
        self.search().entry().set_text(query);
        let panel = self.search();
        pump_until("search finished", move || {
            !panel.is_searching() && panel.results().total_matches > 0 && !panel.timeout_pending()
        });
    }
    fn wait_scan(&self) {
        let panel = self.search();
        pump_until("scan finished", move || {
            !panel.is_searching() && !panel.timeout_pending()
        });
    }

    // ---- editor stand-in ---------------------------------------------------------------
    fn open_file(&self, path: &Path) {
        let existing = self.tabs.borrow().iter().position(|t| t.path == path);
        let index = match existing {
            Some(index) => index,
            None => {
                let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
                buffer.set_text(&std::fs::read_to_string(path).unwrap_or_default());
                buffer.set_modified(false);
                let view = gtk::TextView::with_buffer(&buffer);
                self.editor_box.pack_start(&view, true, true, 0);
                view.show();
                self.tabs.borrow_mut().push(Tab {
                    path: path.to_path_buf(),
                    buffer,
                    view,
                });
                self.tabs.borrow().len() - 1
            }
        };
        self.switch_to(index);
    }
    fn switch_to(&self, index: usize) {
        let changed = *self.current.borrow() != Some(index);
        *self.current.borrow_mut() = Some(index);
        if changed {
            let path = self.tabs.borrow()[index].path.clone();
            self.explorer.set_current_file(Some(&path));
        }
    }
    fn current_path(&self) -> Option<PathBuf> {
        let index = (*self.current.borrow())?;
        Some(self.tabs.borrow()[index].path.clone())
    }
    fn open_file_at(&self, path: &Path, line: i32, column: i32, length: i32) {
        self.open_file(path);
        let index = self.current.borrow().unwrap();
        let tabs = self.tabs.borrow();
        let tab = &tabs[index];
        let line = line.clamp(0, tab.buffer.line_count() - 1);
        let start = tab.buffer.iter_at_line_offset(line, column.max(0));
        let mut end = start;
        end.forward_chars(length.max(0));
        tab.buffer.select_range(&start, &end);
        *self.selection.borrow_mut() =
            Some((line as usize, column as usize, (column + length) as usize));
        tab.view.grab_focus();
    }
    fn focus_current(&self) -> bool {
        let Some(index) = *self.current.borrow() else {
            return false;
        };
        self.tabs.borrow()[index].view.grab_focus();
        true
    }
    fn buffer_text(&self, path: &Path) -> Option<String> {
        let tabs = self.tabs.borrow();
        let tab = tabs.iter().find(|t| t.path == path)?;
        Some(
            tab.buffer
                .text(&tab.buffer.start_iter(), &tab.buffer.end_iter(), true)
                .map(|t| t.to_string())
                .unwrap_or_default(),
        )
    }
    /// `MainWindow.search_in_files`.
    fn search_in_files(&self) {
        self.explorer.show();
        let seed = self
            .current
            .borrow()
            .and_then(|i| {
                let tabs = self.tabs.borrow();
                let buffer = &tabs[i].buffer;
                buffer.selection_bounds().map(|(a, b)| {
                    buffer
                        .text(&a, &b, true)
                        .map(|t| t.to_string())
                        .unwrap_or_default()
                })
            })
            .unwrap_or_default();
        let seed = if seed.trim().is_empty()
            || seed.contains(['\n', '\r'])
            || seed.chars().count() > 200
        {
            String::new()
        } else {
            seed
        };
        self.explorer.focus_search(Some(&seed));
    }
}

fn setup() -> Harness {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    std::fs::write(
        root.join("a.py"),
        "alpha needle one\nline two\n  needle two\n",
    )
    .unwrap();
    std::fs::write(root.join("b.py"), "needle in b\n").unwrap();
    std::fs::write(root.join("notes.md"), "# Title\n\nsome needle here\n").unwrap();
    let big: String = (0..300).map(|i| format!("line {i}\n")).collect::<String>()
        + "the needle far below\n"
        + &"tail\n".repeat(100);
    std::fs::write(root.join("big.py"), big).unwrap();
    let explorer = ExplorerPanel::new(&root);
    let editor_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(900, 600);
    let layout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    layout.pack_start(&explorer, false, false, 0);
    layout.pack_start(&editor_box, true, true, 0);
    explorer.set_size_request(300, -1);
    window.add(&layout);
    window.show_all();
    window.present();
    let w = window.clone();
    pump_until("window active", move || w.is_active());
    let harness = Harness {
        temp,
        window,
        explorer,
        editor_box,
        tabs: Rc::default(),
        current: Rc::default(),
        selection: Rc::default(),
    };
    wait_idle(&harness.explorer);
    // Wiring as in the window.
    let (tabs, current) = (harness.tabs.clone(), harness.current.clone());
    harness
        .explorer
        .set_text_provider(Some(Rc::new(move |path| {
            let tabs = tabs.borrow();
            let _ = &current;
            let tab = tabs.iter().find(|t| t.path == path)?;
            Some(
                tab.buffer
                    .text(&tab.buffer.start_iter(), &tab.buffer.end_iter(), true)
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
            )
        })));
    harness
}

fn wire(h: &Rc<Harness>) {
    let weak = Rc::downgrade(h);
    h.explorer
        .connect_search_result_activated(move |_, path, line, column, length| {
            if let Some(h) = weak.upgrade() {
                h.open_file_at(path, line, column, length);
            }
        });
    let weak = Rc::downgrade(h);
    h.explorer.connect_file_activated(move |_, path| {
        if let Some(h) = weak.upgrade() {
            h.open_file(path);
        }
    });
    let weak = Rc::downgrade(h);
    h.explorer.connect_search_escaped(move |_| {
        if let Some(h) = weak.upgrade() {
            if !h.focus_current() {
                h.explorer.focus_tree();
            }
        }
    });
    let weak = Rc::downgrade(h);
    h.explorer.connect_path_renamed(move |_, old, new| {
        let Some(h) = weak.upgrade() else { return };
        let mut touched = false;
        for tab in h.tabs.borrow_mut().iter_mut() {
            if tab.path == old {
                tab.path = new.to_path_buf();
                touched = true;
            }
        }
        if touched && h.current_path().as_deref() == Some(new) {
            h.explorer.set_current_file(Some(new));
        }
    });
    let weak = Rc::downgrade(h);
    h.explorer.connect_path_deleted(move |_, path| {
        let Some(h) = weak.upgrade() else { return };
        h.tabs.borrow_mut().retain(|t| t.path != path);
        *h.current.borrow_mut() = if h.tabs.borrow().is_empty() {
            None
        } else {
            Some(0)
        };
        h.explorer.set_current_file(h.current_path().as_deref());
    });
    let weak = Rc::downgrade(h);
    h.window.connect_key_press_event(move |_, event| {
        let Some(h) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let ctrl_shift = gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK;
        if (event.keyval() == key::F || event.keyval() == key::f)
            && event.state().contains(ctrl_shift)
        {
            h.search_in_files();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
}

fn setup_wired() -> Rc<Harness> {
    let h = Rc::new(setup());
    wire(&h);
    h
}

fn press_window(h: &Harness, keyval: gdk::keys::Key, state: gdk::ModifierType) -> bool {
    let event = window_key_event(&h.window, keyval, state);
    h.window.emit_by_name::<bool>("key-press-event", &[&event])
}

#[test]
fn test_search_box_is_first_under_header() {
    run(|| {
        let h = setup_wired();
        let children = h.explorer.children();
        assert_eq!(children.len(), 2);
        assert!(children[0]
            .style_context()
            .has_class("impro-explorer-header"));
        let split = children[1].clone().downcast::<gtk::Paned>().unwrap();
        assert_eq!(split.orientation(), gtk::Orientation::Vertical);
        assert_eq!(split.child1().unwrap(), h.search().upcast::<gtk::Widget>());
        assert_eq!(
            split.child2().unwrap(),
            h.explorer.notes_widgets().section.upcast::<gtk::Widget>()
        );
        let row = h.search().children()[0]
            .clone()
            .downcast::<gtk::Box>()
            .unwrap();
        assert_eq!(
            row.children()[0],
            h.search().entry().upcast::<gtk::Widget>()
        );
        assert_eq!(
            h.search().children()[1],
            h.search().stack().upcast::<gtk::Widget>()
        );
        assert_eq!(
            h.search().stack().visible_child_name().as_deref(),
            Some("content")
        );
        assert!(h.explorer.tree().is_ancestor(&h.search()));
    });
}

#[test]
fn test_typing_shows_results_and_escape_restores_tree() {
    run(|| {
        let h = setup_wired();
        h.open_file(&h.p("a.py"));
        h.explorer.focus_search(None);
        assert!(h.search().entry().is_focus());
        h.search_for("needle");
        assert_eq!(
            h.search().stack().visible_child_name().as_deref(),
            Some("results")
        );
        assert!(!h
            .search()
            .stack()
            .child_by_name("content")
            .unwrap()
            .is_mapped());
        press(&h.search().entry(), key::Escape, gdk::ModifierType::empty());
        assert_eq!(h.search().query(), "");
        assert_eq!(
            h.search().stack().visible_child_name().as_deref(),
            Some("content")
        );
        // With a tab open, Esc hands focus back to the editor.
        let focus = h.window.focused_widget().unwrap();
        assert!(focus.is_ancestor(&h.editor_box));
    });
}

#[test]
fn test_escape_without_tabs_focuses_tree() {
    run(|| {
        let h = setup_wired();
        h.explorer.focus_search(None);
        h.search_for("needle");
        press(&h.search().entry(), key::Escape, gdk::ModifierType::empty());
        assert_eq!(h.search().query(), "");
        assert_eq!(
            h.window.focused_widget().unwrap(),
            h.explorer.tree().upcast::<gtk::Widget>()
        );
    });
}

#[test]
fn test_active_tab_group_first_and_switching_resorts() {
    run(|| {
        let h = setup_wired();
        h.open_file(&h.p("b.py"));
        h.open_file(&h.p("a.py"));
        h.search_for("needle");
        let w = h.clone();
        pump_until("four groups", move || w.groups().len() == 4);
        assert_eq!(h.groups()[0], h.p("a.py"));
        assert_eq!(
            h.groups()[1..],
            [h.p("b.py"), h.p("big.py"), h.p("notes.md")]
        );
        h.open_file(&h.p("b.py"));
        let w = h.clone();
        pump_until("b first", move || w.groups().first() == Some(&w.p("b.py")));
        assert_eq!(h.groups()[1], h.p("a.py"));
        h.switch_to(1);
        let w = h.clone();
        pump_until("a first", move || w.groups().first() == Some(&w.p("a.py")));
        h.wait_scan();
        assert_eq!(h.groups()[0], h.p("a.py"));
        assert_eq!(h.groups().len(), 4);
        let first = h.search().store().iter_first().unwrap();
        assert!(
            impro_explorer::util::string(&h.search().store(), &first, 2).contains("current file")
        );
    });
}

#[test]
fn test_unsaved_edits_are_searched() {
    run(|| {
        let h = setup_wired();
        h.open_file(&h.p("b.py"));
        let (buffer, modified) = {
            let tabs = h.tabs.borrow();
            (tabs[0].buffer.clone(), tabs[0].buffer.is_modified())
        };
        assert!(!modified);
        buffer.set_text("zebra unsaved\nneedle edited\n");
        assert_eq!(
            h.buffer_text(&h.p("b.py")).as_deref(),
            Some("zebra unsaved\nneedle edited\n")
        );
        assert_eq!(h.buffer_text(&h.p("a.py")), None);
        h.search_for("zebra");
        assert_eq!(h.groups(), [h.p("b.py")]);
        assert_eq!(h.search().results().groups[0].matches[0].line, 0);
        assert_eq!(
            std::fs::read_to_string(h.p("b.py")).unwrap(),
            "needle in b\n"
        );
    });
}

#[test]
fn test_activating_result_opens_tab_and_selects_exact_match() {
    run(|| {
        let h = setup_wired();
        h.open_file(&h.p("b.py"));
        h.search_for("needle");
        let store = h.search().store();
        let a = h.p("a.py").to_string_lossy().into_owned();
        let group = store.iter_first().unwrap();
        while impro_explorer::util::string(&store, &group, 3) != a {
            assert!(store.iter_next(&group));
        }
        let child = store.iter_nth_child(Some(&group), 1).unwrap();
        assert_eq!(
            (
                impro_explorer::util::int(&store, &child, 4),
                impro_explorer::util::int(&store, &child, 5),
                impro_explorer::util::int(&store, &child, 6)
            ),
            (2, 2, 6)
        );
        let path = impro_explorer::util::path_of(&store, &child);
        h.search()
            .tree()
            .row_activated(&path, &h.search().tree().column(0).unwrap());
        assert_eq!(h.tabs.borrow().len(), 2);
        assert_eq!(h.current_path(), Some(h.p("a.py")));
        {
            let tabs = h.tabs.borrow();
            let buffer = &tabs[1].buffer;
            let (start, end) = buffer.selection_bounds().unwrap();
            assert_eq!(buffer.text(&start, &end, true).as_deref(), Some("needle"));
            assert_eq!(
                (start.line(), start.line_offset(), end.line_offset()),
                (2, 2, 8)
            );
            assert!(tabs[1].view.is_focus());
        }
        // Activating again focuses the existing tab instead of duplicating it.
        h.search().emit_by_name::<()>(
            "result-activated",
            &[
                &h.p("b.py").to_string_lossy().into_owned(),
                &0i32,
                &0i32,
                &6i32,
            ],
        );
        assert_eq!(h.tabs.borrow().len(), 2);
        assert_eq!(h.current_path(), Some(h.p("b.py")));
    });
}

#[test]
fn test_current_file_signal_and_rename() {
    run(|| {
        let h = setup_wired();
        h.open_file(&h.p("a.py"));
        h.open_file(&h.p("b.py"));
        assert_eq!(h.search().current_path(), Some(h.p("b.py")));
        h.open_file(&h.p("a.py"));
        assert_eq!(h.search().current_path(), Some(h.p("a.py")));
        let renamed = rename(&h.explorer, &h.p("a.py"), "renamed.py").unwrap();
        assert_eq!(h.search().current_path(), Some(renamed.clone()));
        assert_eq!(h.current_path(), Some(renamed));
    });
}

#[test]
fn test_ctrl_shift_f_focuses_entry_shows_explorer_and_prefills_selection() {
    run(|| {
        let h = setup_wired();
        h.open_file(&h.p("a.py"));
        {
            let tabs = h.tabs.borrow();
            let buffer = &tabs[0].buffer;
            buffer.select_range(
                &buffer.iter_at_line_offset(0, 6),
                &buffer.iter_at_line_offset(0, 12),
            );
        }
        h.explorer.hide();
        assert!(!h.explorer.is_visible());
        let ctrl_shift = gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK;
        assert!(press_window(&h, key::F, ctrl_shift));
        assert!(h.explorer.is_visible());
        assert!(h.search().entry().is_focus());
        assert_eq!(h.search().query(), "needle");
        assert_eq!(
            h.search().stack().visible_child_name().as_deref(),
            Some("results")
        );
        let w = h.clone();
        pump_until("current group first", move || {
            w.groups().first() == Some(&w.p("a.py"))
        });
        // Lowercase keyval (no Caps) behaves the same, and a multi-line selection is not used.
        h.search().clear();
        {
            let tabs = h.tabs.borrow();
            let buffer = &tabs[0].buffer;
            buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
        }
        assert!(press_window(&h, key::f, ctrl_shift));
        assert_eq!(h.search().query(), "");
        assert!(h.search().entry().is_focus());
        // An empty selection keeps the previous query, selected for overtyping.
        h.search().entry().set_text("keep");
        {
            let tabs = h.tabs.borrow();
            tabs[0].buffer.place_cursor(&tabs[0].buffer.start_iter());
        }
        press_window(&h, key::F, ctrl_shift);
        assert_eq!(h.search().query(), "keep");
    });
}

#[test]
fn test_explorer_shortcuts_do_not_act_from_search_entry_or_results() {
    run(|| {
        let h = setup_wired();
        std::fs::create_dir(h.p("keep")).unwrap();
        h.explorer.refresh();
        wait_idle(&h.explorer);
        h.explorer.select(&h.p("b.py"));
        assert_eq!(h.explorer.selected_path(), Some(h.p("b.py")));
        h.search_for("needle");
        let recorder = Recorder::default();
        recorder.install(&h.explorer, None, |_| false);
        for target in [
            h.search().entry().upcast::<gtk::Widget>(),
            h.search().tree().upcast(),
        ] {
            target.grab_focus();
            assert_eq!(h.window.focused_widget().unwrap(), target);
            if target == h.search().entry().upcast::<gtk::Widget>() {
                h.search().entry().set_position(-1);
            }
            let ctrl_shift = gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK;
            for (keyval, state) in [
                (key::Delete, gdk::ModifierType::empty()),
                (key::F2, gdk::ModifierType::empty()),
                (key::F5, gdk::ModifierType::empty()),
                (key::N, ctrl_shift),
                (key::n, ctrl_shift),
            ] {
                press_window(&h, keyval, state);
            }
        }
        assert!(recorder.names.borrow().is_empty());
        assert!(recorder.confirms.borrow().is_empty());
        assert!(h.p("b.py").exists());
        let mut entries: Vec<_> = std::fs::read_dir(h.root())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        assert_eq!(entries, ["a.py", "b.py", "big.py", "keep", "notes.md"]);
        assert_eq!(h.search().query(), "needle");
        // The tree keeps its shortcuts when it has focus.
        h.explorer.tree().grab_focus();
        press_window(&h, key::F2, gdk::ModifierType::empty());
        assert_eq!(recorder.names.borrow().len(), 1);
    });
}

#[test]
fn test_rename_and_delete_refresh_results() {
    run(|| {
        let h = setup_wired();
        h.open_file(&h.p("a.py"));
        h.search_for("needle");
        assert_eq!(h.groups().len(), 4);
        rename(&h.explorer, &h.p("b.py"), "c.py").unwrap();
        let w = h.clone();
        pump_until("c.py listed", move || w.groups().contains(&w.p("c.py")));
        h.wait_scan();
        let mut names: Vec<_> = h
            .groups()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["a.py", "big.py", "c.py", "notes.md"]);
        assert!(!h.groups().contains(&h.p("b.py")));
        // Renaming the open file keeps it first under its new name.
        rename(&h.explorer, &h.p("a.py"), "z.py").unwrap();
        h.wait_scan();
        assert_eq!(h.groups()[0], h.p("z.py"));
        assert!(!h.groups().contains(&h.p("a.py")));
        delete(&h.explorer, &h.p("notes.md"), false).unwrap();
        h.wait_scan();
        assert!(!h.groups().contains(&h.p("notes.md")));
        // A deleted file with an open tab is not offered either.
        delete(&h.explorer, &h.p("z.py"), false).unwrap();
        h.wait_scan();
        assert!(!h.groups().contains(&h.p("z.py")));
        let mut names: Vec<_> = h
            .groups()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["big.py", "c.py"]);
        // Refresh also re-runs the query for files created behind the app's back.
        std::fs::write(h.p("new.py"), "a needle\n").unwrap();
        h.explorer.refresh();
        let w = h.clone();
        pump_until("new.py listed", move || w.groups().contains(&w.p("new.py")));
    });
}

#[test]
fn test_open_folder_changes_search_root() {
    run(|| {
        let h = setup_wired();
        let other = h.root().parent().unwrap().join(format!(
            "{}-other",
            h.temp.path().file_name().unwrap().to_string_lossy()
        ));
        std::fs::create_dir(&other).unwrap();
        std::fs::write(other.join("only.py"), "needle only here\n").unwrap();
        h.search_for("needle");
        assert_eq!(h.groups().len(), 4);
        h.explorer.set_root(&other);
        assert_eq!(h.search().root(), other);
        let w = h.clone();
        let expected = other.join("only.py");
        pump_until("only.py found", move || {
            w.groups() == [expected.clone()]
                && !w.search().is_searching()
                && !w.search().timeout_pending()
        });
        std::fs::remove_dir_all(&other).unwrap();
    });
}

#[test]
fn test_explorer_font_size_reaches_result_rows() {
    run(|| {
        let h = setup_wired();
        h.explorer.set_font_size(14.0);
        for cell in h.search().cells() {
            assert_eq!(cell.size_points(), 14.0);
        }
        assert_eq!(h.search().font_size(), 14.0);
    });
}
