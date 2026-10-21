mod common;
use common::*;
use gtk::prelude::*;
use impro_editor::{EditorPanel, Response};
use std::{cell::Cell, rc::Rc};
#[test]
fn current_file_callback_can_close_and_reopen() {
    run(|| {
        let h = Harness::new();
        let once = Rc::new(Cell::new(false));
        let o = once.clone();
        h.panel.connect_current_file_changed(move |p, path| {
            if path.is_some() && !o.replace(true) {
                p.close_current(|_| {});
            }
        });
        let path = h.dir.path().join("a.txt");
        std::fs::write(&path, "a").unwrap();
        h.panel.open_file(path);
        poll(|| !h.panel.is_busy());
        assert!(h.panel.tabs().is_empty());
    });
}
#[test]
fn view_shutdown_can_add_another_view() {
    run(|| {
        let h = Harness::new();
        let weak = h.panel.downgrade();
        h.panel.add_view_tab_with_shutdown(
            "old",
            "old",
            &gtk::Entry::new(),
            None,
            None,
            move || {
                if let Some(p) = weak.upgrade() {
                    p.add_view_tab("new", "new", &gtk::Entry::new(), None, None);
                }
            },
        );
        assert!(h.panel.close_view_tab("old"));
        assert!(h.panel.has_view_tab("new"));
    });
}
#[test]
fn duplicate_view_focus_can_reenter() {
    run(|| {
        let h = Harness::new();
        let a = gtk::Entry::new();
        h.panel.add_view_tab("a", "a", &a, None, None);
        h.panel
            .add_view_tab("b", "b", &gtk::Entry::new(), None, None);
        let weak = h.panel.downgrade();
        a.connect_focus_in_event(move |_, _| {
            if let Some(p) = weak.upgrade() {
                p.set_view_tab_title("a", "changed", None);
            }
            glib::Propagation::Proceed
        });
        assert!(!h
            .panel
            .add_view_tab("a", "a", &gtk::Entry::new(), None, None));
    });
}
#[test]
fn dirty_callback_can_close_tab() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "a");
        h.answer(Response::Discard);
        h.panel.connect_dirty_changed(|p, dirty| {
            if dirty {
                p.close_current(|_| {});
            }
        });
        t.buffer.insert_at_cursor("edit");
        assert!(h.panel.tabs().is_empty());
    });
}
#[test]
fn save_callback_can_rename_and_close() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "a");
        t.buffer.insert_at_cursor("edit");
        h.panel.connect_file_saved(|p, path| {
            p.on_path_renamed(path, path.with_extension("new"));
            p.close_current(|_| {});
        });
        assert!(h.save());
        assert!(h.panel.tabs().is_empty());
    });
}
#[test]
fn close_tab_releases_buffer_and_pending_jobs() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "x = 1\n");
        let buffer = t.buffer.downgrade();
        let view = t.view.downgrade();
        t.buffer.insert_at_cursor("y");
        h.answer(Response::Discard);
        h.close(&t);
        drop(t);
        pump(200);
        assert!(view.upgrade().is_none());
        assert!(buffer.upgrade().is_none());
    });
}
#[test]
fn shutdown_cancels_unfinished_open() {
    run(|| {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("large.py");
        std::fs::write(&p, "x = 1\n".repeat(150000)).unwrap();
        let panel = EditorPanel::default();
        panel.open_file(p);
        panel.shutdown();
        pump(100);
        assert!(!panel.is_busy());
        assert!(panel.tabs().is_empty());
    });
}
#[test]
fn edits_during_save_remain_dirty() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.txt", "a");
        replace(&t, "saved version");
        let done = Rc::new(Cell::new(false));
        let d = done.clone();
        h.panel.save_current(move |ok| {
            assert!(ok);
            d.set(true);
        });
        let edited = Rc::new(Cell::new(false));
        let e = edited.clone();
        let weak = Rc::downgrade(&t);
        glib::idle_add_local_once(move || {
            if let Some(t) = weak.upgrade() {
                t.buffer.insert_at_cursor(" later");
                e.set(true);
            }
        });
        poll(|| done.get());
        assert!(edited.get());
        let disk = std::fs::read_to_string(t.path()).unwrap();
        if disk != t.text() {
            assert!(t.buffer.is_modified());
        }
    });
}
#[test]
fn page_added_callback_can_close_new_file() {
    run(|| {
        let h = Harness::new();
        let weak = h.panel.downgrade();
        h.panel.notebook().connect_page_added(move |_, _, _| {
            if let Some(p) = weak.upgrade() {
                p.close_current(|_| {});
            }
        });
        let path = h.dir.path().join("a.txt");
        std::fs::write(&path, "a").unwrap();
        h.panel.open_file(path);
        poll(|| !h.panel.is_busy());
        pump(30);
        assert!(h.panel.tabs().is_empty());
    });
}
