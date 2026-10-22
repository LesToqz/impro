mod common;
use common::*;
use gdk::keys::constants as k;
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[test]
fn test_append_duplicate_title_and_icon() {
    run(|| {
        let h = Harness::new();
        h.open("a.py", "one");
        h.open("b.py", "two");
        let entry = gtk::Entry::new();
        assert!(h.panel.add_view_tab(
            "commit",
            "commit",
            &entry,
            Some("Commit details"),
            Some("document-open-symbolic")
        ));
        let other = gtk::Entry::new();
        assert!(h
            .panel
            .add_view_tab("other", "other", &other, None, Some("impro-missing-icon")));
        h.open("last.py", "last");
        assert_eq!(h.panel.notebook().n_pages(), 5);
        assert_eq!(h.panel.tabs().len(), 3);
        assert!(!h
            .panel
            .add_view_tab("commit", "ignored", &gtk::Label::new(None), None, None));
        assert_eq!(h.panel.current_view_key().as_deref(), Some("commit"));
        assert_eq!(
            h.panel.view_tab_widget("commit"),
            Some(entry.clone().upcast())
        );
        h.panel
            .set_view_tab_title("commit", "abc123", Some("new tooltip"));
        let tabs = h.panel.view_tabs();
        let tab = tabs.iter().find(|t| t.key == "commit").unwrap();
        assert_eq!(tab.header.label.text(), "abc123");
        assert_eq!(
            tab.header.widget.tooltip_text().as_deref(),
            Some("new tooltip")
        );
        assert_eq!(tab.header.label.ellipsize(), pango::EllipsizeMode::End);
        assert_eq!(tab.header.inner.children().len(), 4);
        assert_eq!(
            tabs.iter()
                .find(|t| t.key == "other")
                .unwrap()
                .header
                .inner
                .children()
                .len(),
            3
        );
        assert!(h.panel.view_tab_widget("missing").is_none());
    });
}
#[test]
fn test_close_paths_signals_shutdown_placeholder() {
    run(|| {
        let h = Harness::new();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        h.panel.connect_view_tab_closed(move |p, key| {
            s.borrow_mut()
                .push((key.to_string(), p.has_view_tab(key), p.notebook().n_pages()))
        });
        for method in ["button", "middle", "api", "keyboard"] {
            let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let calls = Rc::new(Cell::new(0));
            let c = calls.clone();
            let w = widget.downgrade();
            h.panel
                .add_view_tab_with_shutdown(method, method, &widget, None, None, move || {
                    assert!(w.upgrade().unwrap().parent().is_some());
                    c.set(c.get() + 1);
                });
            match method {
                "button" => h.panel.view_tabs()[0].header.close.clicked(),
                "middle" => {
                    assert!(h.panel.view_tabs()[0]
                        .header
                        .widget
                        .emit_by_name::<bool>("button-press-event", &[&button_event(2)]));
                }
                "keyboard" => {
                    assert!(key(&h.panel, k::w, true, false));
                }
                _ => {
                    assert!(h.panel.close_view_tab(method));
                }
            }
            assert_eq!(calls.get(), 1);
            assert!(widget.parent().is_none());
            assert_eq!(seen.borrow().last(), Some(&(method.to_string(), false, 0)));
            assert_eq!(
                h.panel.stack().visible_child_name().as_deref(),
                Some("empty")
            );
            assert!(!h.panel.close_view_tab(method));
        }
    });
}
#[test]
fn test_mixed_successors_status_and_current_signal() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.py", "one");
        h.panel.search_bar().set_search_mode(true);
        h.panel
            .add_view_tab("commit", "commit", &gtk::Entry::new(), None, None);
        assert!(h.panel.current_path().is_none());
        assert!(h.panel.status().items.iter().all(|l| l.text().is_empty()));
        assert!(!h.panel.search_bar().is_search_mode());
        let last = h.open("last.py", "last");
        assert!(h.close(&last));
        assert_eq!(h.panel.current_view_key().as_deref(), Some("commit"));
        h.panel.close_view_tab("commit");
        assert_eq!(h.panel.current_path(), Some(a.path()));
        h.panel
            .add_view_tab("next", "next", &gtk::Entry::new(), None, None);
        h.panel.notebook().set_current_page(Some(0));
        h.close(&a);
        assert_eq!(h.panel.current_view_key().as_deref(), Some("next"));
    });
}
#[test]
fn test_cycle_mixed_reordered_pages() {
    run(|| {
        let h = Harness::new();
        let a = h.open("a.py", "one");
        let view = gtk::Entry::new();
        h.panel.add_view_tab("commit", "commit", &view, None, None);
        let b = h.open("b.py", "two");
        key(&h.panel, k::Tab, true, false);
        assert_eq!(h.panel.current_path(), Some(a.path()));
        key(&h.panel, k::Tab, true, false);
        assert_eq!(h.panel.current_view_key().as_deref(), Some("commit"));
        key(&h.panel, k::Tab, true, false);
        assert_eq!(h.panel.current_path(), Some(b.path()));
        key(&h.panel, k::ISO_Left_Tab, true, true);
        h.panel.notebook().reorder_child(&view, Some(0));
        key(&h.panel, k::Page_Down, true, false);
        assert_eq!(h.panel.current_path(), Some(a.path()));
    });
}
#[test]
fn test_file_commands_ignore_view() {
    run(|| {
        let h = Harness::new();
        let file = h.open("note.md", "# Title");
        h.panel
            .add_view_tab("commit", "commit", &gtk::Entry::new(), None, None);
        assert!(h.save());
        assert!(h.save_all());
        assert!(h.panel.current_path().is_none());
        assert!(h.panel.selected_text().is_empty());
        let size = h.panel.font_size();
        h.panel.zoom(3);
        assert_eq!(h.panel.font_size(), size);
        h.panel.edit("enter");
        h.panel.find(false, false);
        assert_eq!(file.text(), "# Title");
    });
}
#[test]
fn test_adaptive_width_and_shared_hover() {
    run(|| {
        let h = Harness::new();
        for i in 0..8 {
            h.panel.add_view_tab(
                &format!("c{i}"),
                "a very long commit title",
                &gtk::Entry::new(),
                None,
                None,
            );
        }
        pump(40);
        for t in h.panel.view_tabs() {
            assert_eq!(t.header.widget.width_request(), 112);
            assert!(h.panel.notebook().tab_is_reorderable(&t.page));
        }
    });
}
#[test]
fn test_focus_nested_widgets_and_real_keyboard() {
    run(|| {
        let h = Harness::new();
        let file = h.open("a.py", "one\ntwo");
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let entry = gtk::Entry::new();
        let text = gtk::TextView::new();
        container.add(&entry);
        container.add(&text);
        h.panel
            .add_view_tab("commit", "commit", &container, None, None);
        text.grab_focus();
        h.panel.focus_current();
        assert!(text.is_focus());
        h.panel.notebook().set_current_page(Some(0));
        h.panel.focus_current();
        assert!(file.view.is_focus());
        h.panel.notebook().set_current_page(Some(1));
        pump(30);
        entry.grab_focus();
        assert!(real_key(&h.window, &entry, k::Tab));
        assert_eq!(h.panel.current_path(), Some(file.path()));
        assert!(real_key(&h.window, &file.view, k::Tab));
        assert_eq!(h.panel.current_view_key().as_deref(), Some("commit"));
        entry.grab_focus();
        assert!(real_key(&h.window, &entry, k::w));
        assert!(!h.panel.has_view_tab("commit"));
    });
}
#[test]
fn test_destroy_panel_shuts_down_once() {
    run(|| {
        let h = Harness::new();
        let calls = Rc::new(Cell::new(0));
        let c = calls.clone();
        h.panel.add_view_tab_with_shutdown(
            "commit",
            "commit",
            &gtk::Entry::new(),
            None,
            None,
            move || c.set(c.get() + 1),
        );
        h.panel.shutdown();
        h.panel.shutdown();
        assert_eq!(calls.get(), 1);
    });
}
