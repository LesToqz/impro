mod common;
use common::*;
use gtk::prelude::*;
use impro_editor::Response;
use std::{cell::Cell, rc::Rc};

#[test]
fn chunked_unicode_load_completes_before_callbacks_and_save() {
    run(|| {
        let h = Harness::new();
        let path = h.dir.path().join("unicode.py");
        let source = "value = '😀 café'\r\n".repeat(10000);
        std::fs::write(&path, &source).unwrap();
        let expected = Rc::new(source.replace("\r\n", "\n"));
        let observed_prefix = Rc::new(Cell::new(false));
        let duplicate_done = Rc::new(Cell::new(false));
        let saved = Rc::new(Cell::new(false));
        let prefix = observed_prefix.clone();
        let duplicate = duplicate_done.clone();
        let save_done = saved.clone();
        let wanted = expected.clone();
        h.panel.connect_current_file_changed(move |panel, path| {
            if let Some(path) = path {
                let tab = panel.current_tab().unwrap();
                assert!(tab.buffer.char_count() < wanted.chars().count() as i32);
                assert!(!tab.buffer.is_modified());
                prefix.set(true);
                let wanted = wanted.clone();
                let duplicate = duplicate.clone();
                panel.open_file_with(path, move |tab| {
                    let tab = tab.unwrap();
                    assert_eq!(tab.text(), *wanted);
                    assert_eq!(tab.history_len(), 0);
                    assert!(!tab.buffer.is_modified());
                    assert_eq!(tab.ending(), "CRLF");
                    assert_eq!(tab.cursor(), wanted.chars().count() as i32);
                    duplicate.set(true);
                });
                let save_done = save_done.clone();
                panel.save_tab(&tab, move |ok| {
                    assert!(ok);
                    save_done.set(true);
                });
            }
        });
        h.panel.open_file(&path);
        poll(|| !h.panel.is_busy() && saved.get() && duplicate_done.get());
        assert!(observed_prefix.get());
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
        let tab = h.panel.current_tab().unwrap();
        assert_eq!(tab.text(), *expected);
        assert_eq!(tab.history_len(), 0);
        tab.buffer.begin_user_action();
        tab.buffer.insert_at_cursor("x");
        tab.buffer.end_user_action();
        assert_eq!(tab.history_len(), 1);
        tab.undo(false);
        assert_eq!(tab.text(), *expected);
        assert!(!tab.buffer.is_modified());
    });
}

#[test]
fn closing_a_published_prefix_releases_the_buffer() {
    run(|| {
        let h = Harness::new();
        h.answer(Response::Discard);
        let path = h.dir.path().join("large.py");
        std::fs::write(&path, "value = 123\n".repeat(100000)).unwrap();
        let closed = Rc::new(Cell::new(false));
        let seen = closed.clone();
        h.panel.connect_current_file_changed(move |panel, path| {
            if path.is_some() {
                let tab = panel.current_tab().unwrap();
                let weak = tab.buffer.downgrade();
                panel.close_tab(&tab, |ok| assert!(ok));
                let seen = seen.clone();
                glib::idle_add_local_full(glib::Priority::LOW, move || {
                    if weak.upgrade().is_none() {
                        seen.set(true);
                        glib::ControlFlow::Break
                    } else {
                        glib::ControlFlow::Continue
                    }
                });
            }
        });
        h.panel.open_file_with(&path, |tab| assert!(tab.is_none()));
        poll(|| !h.panel.is_busy() && closed.get());
        assert!(h.panel.tabs().is_empty());
    });
}
