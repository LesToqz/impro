//! Signals fired while the models mutate must never double-borrow panel state.
mod common;
use common::*;
use gtk::prelude::*;
use impro_explorer::explorer::ExplorerPanel;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

fn setup() -> (tempfile::TempDir, ExplorerPanel) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for dir in ["a/b/c", "d", "e/f"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "a/b/c/x.txt",
        "a/y.txt",
        "d/z.txt",
        "e/f/w.txt",
        "top.txt",
        "other.py",
    ] {
        std::fs::write(root.join(file), "needle").unwrap();
    }
    let panel = ExplorerPanel::new(root);
    wait_idle(&panel);
    (temp, panel)
}

#[test]
fn test_model_signal_handlers_may_call_back_into_the_panel() {
    run(|| {
        let (temp, panel) = setup();
        let calls = Rc::new(Cell::new(0u32));
        let store = panel.store();
        let (p, c) = (panel.clone(), calls.clone());
        store.connect_row_inserted(move |_, _, _| {
            c.set(c.get() + 1);
            let _ = p.expanded_paths();
            let _ = p.selected_path();
            let _ = p.target_folder();
            let _ = p.find_row(&PathBuf::from("/nonexistent"));
            p.tree().queue_draw();
        });
        let (p, c) = (panel.clone(), calls.clone());
        store.connect_row_deleted(move |_, _| {
            c.set(c.get() + 1);
            let _ = p.expanded_paths();
            let _ = p.is_busy();
        });
        let (p, c) = (panel.clone(), calls.clone());
        panel.tree().connect_row_expanded(move |_, _, _| {
            c.set(c.get() + 1);
            let _ = p.expanded_paths();
        });
        let root = temp.path().to_path_buf();
        expand(&panel, &root.join("a"));
        expand(&panel, &root.join("a/b"));
        panel.set_current_file(Some(&root.join("e/f/w.txt")));
        wait_idle(&panel);
        panel.refresh();
        wait_idle(&panel);
        std::fs::write(root.join("a/new.txt"), "x").unwrap();
        std::fs::remove_file(root.join("d/z.txt")).unwrap();
        panel.refresh();
        wait_idle(&panel);
        assert!(calls.get() > 10);
        assert!(panel.find_row(&root.join("a/new.txt")).is_some());
        destroy(&panel);
    });
}

#[test]
fn test_file_activated_handler_may_reroot_refresh_and_select() {
    run(|| {
        let (temp, panel) = setup();
        let root = temp.path().to_path_buf();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (p, s, r) = (panel.clone(), seen.clone(), root.clone());
        panel.connect_file_activated(move |_, path| {
            s.borrow_mut().push(path.to_path_buf());
            p.set_current_file(Some(path));
            p.refresh();
            p.select(&r.join("top.txt"));
            p.refresh_git();
            p.refresh_notes();
        });
        let column = panel.tree().column(0).unwrap();
        let row = panel.find_row(&root.join("top.txt")).unwrap();
        let path = impro_explorer::util::path_of(&panel.store(), &row);
        panel.tree().row_activated(&path, &column);
        wait_idle(&panel);
        assert_eq!(seen.borrow().len(), 1);
        // A handler that opens another folder from inside the signal.
        let (p, other) = (panel.clone(), root.join("d"));
        let handler = panel.connect_file_activated(move |_, _| p.set_root(&other));
        let row = panel.find_row(&root.join("other.py")).unwrap();
        let path = impro_explorer::util::path_of(&panel.store(), &row);
        panel.tree().row_activated(&path, &column);
        wait_idle(&panel);
        assert_eq!(panel.root(), root.join("d"));
        panel.disconnect(handler);
        assert_eq!(names(&panel, &root.join("d")), ["z.txt"]);
        destroy(&panel);
    });
}

#[test]
fn test_path_signals_may_refresh_delete_and_rename_again() {
    run(|| {
        let (temp, panel) = setup();
        let root = temp.path().to_path_buf();
        let renamed = Rc::new(Cell::new(0));
        let (p, n) = (panel.clone(), renamed.clone());
        panel.connect_path_renamed(move |_, _, new| {
            n.set(n.get() + 1);
            p.refresh();
            p.set_current_file(Some(new));
            let _ = p.expanded_paths();
        });
        let deleted = Rc::new(Cell::new(0));
        let (p, n) = (panel.clone(), deleted.clone());
        panel.connect_path_deleted(move |_, _| {
            n.set(n.get() + 1);
            p.refresh();
        });
        let new = rename(&panel, &root.join("top.txt"), "renamed.txt").unwrap();
        assert_eq!(renamed.get(), 1);
        delete(&panel, &new, false).unwrap();
        assert_eq!(deleted.get(), 1);
        wait_idle(&panel);
        assert!(panel.find_row(&new).is_none());
        destroy(&panel);
    });
}

#[test]
fn test_destroying_the_panel_inside_a_signal_is_safe() {
    run(|| {
        let (temp, panel) = setup();
        let root = temp.path().to_path_buf();
        let window = common::window(&panel, 300, 500);
        let (w, done) = (window.clone(), Rc::new(Cell::new(false)));
        let d = done.clone();
        panel.connect_file_activated(move |_, _| {
            d.set(true);
            destroy(&w);
        });
        let column = panel.tree().column(0).unwrap();
        let row = panel.find_row(&root.join("top.txt")).unwrap();
        let path = impro_explorer::util::path_of(&panel.store(), &row);
        panel.refresh();
        panel.tree().row_activated(&path, &column);
        assert!(done.get());
        // Worker results still in flight must be dropped quietly.
        pump(200);
        panel.refresh_git();
        panel.refresh_notes();
        panel.refresh();
        pump(300);
    });
}

#[test]
fn test_search_signal_handlers_may_clear_reroot_and_research() {
    run(|| {
        let (temp, panel) = setup();
        let root = temp.path().to_path_buf();
        let search = panel.search();
        let hits = Rc::new(Cell::new(0));
        let (p, h, r) = (panel.clone(), hits.clone(), root.clone());
        panel.connect_search_result_activated(move |_, _, _, _, _| {
            h.set(h.get() + 1);
            p.clear_search();
            p.search().set_root(&r.join("a"));
            p.search().search_now(Some("needle"));
            p.search().set_current_file(None);
        });
        search.search_now(Some("needle"));
        wait_idle(&panel);
        let store = search.store();
        let group = store.iter_first().unwrap();
        let child = store.iter_children(Some(&group)).unwrap();
        let path = impro_explorer::util::path_of(&store, &child);
        search
            .tree()
            .row_activated(&path, &search.tree().column(0).unwrap());
        assert_eq!(hits.get(), 1);
        wait_idle(&panel);
        // An escaped handler that restarts a search while the key handler is still running.
        let (p, e) = (panel.clone(), Rc::new(Cell::new(0)));
        let count = e.clone();
        panel.connect_search_escaped(move |_| {
            count.set(count.get() + 1);
            p.focus_search(Some("needle"));
            p.focus_tree();
        });
        press(
            &search.entry(),
            gdk::keys::constants::Escape,
            gdk::ModifierType::empty(),
        );
        wait_idle(&panel);
        assert_eq!(e.get(), 1);
        destroy(&panel);
    });
}

#[test]
fn test_bursts_of_changes_while_scans_are_in_flight_converge() {
    run(|| {
        let (temp, panel) = setup();
        let root = temp.path().to_path_buf();
        // Hammer refresh, hidden toggles and folder ordering without waiting in between.
        for round in 0..20 {
            std::fs::write(root.join(format!("burst{round}.txt")), "x").unwrap();
            panel.refresh();
            panel.set_show_hidden(round % 2 == 0);
            if round % 5 == 0 {
                panel.set_folders_first(round % 10 != 0);
            }
            pump(2);
        }
        panel.set_show_hidden(true);
        panel.set_folders_first(true);
        wait_idle(&panel);
        let listed = names(&panel, &root);
        for round in 0..20 {
            assert!(
                listed.contains(&format!("burst{round}.txt")),
                "burst{round}"
            );
        }
        let mut expected: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        expected.sort_by_key(|n| (!root.join(n).is_dir(), n.to_lowercase(), n.clone()));
        assert_eq!(listed, expected);
        destroy(&panel);
    });
}

#[test]
fn test_live_text_provider_may_cancel_its_own_search() {
    run(|| {
        let (_temp, panel) = setup();
        let calls = Rc::new(Cell::new(0));
        let search = panel.search();
        let weak = search.downgrade();
        let count = calls.clone();
        search.set_text_provider(Some(Rc::new(move |_| {
            count.set(count.get() + 1);
            if let Some(search) = weak.upgrade() {
                search.clear();
                search.set_text_provider(None);
            }
            Some("needle".into())
        })));
        search.search_now(Some("needle"));
        pump_until("provider cancels query", || calls.get() > 0);
        wait_idle(&panel);
        assert_eq!(calls.get(), 1);
        assert_eq!(search.query(), "");
        assert_eq!(search.results().total_matches, 0);
        destroy(&panel);
    });
}
