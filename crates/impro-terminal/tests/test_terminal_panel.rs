#![cfg(unix)]
mod common;
use common::*;
use gtk::prelude::*;
use impro_terminal::*;
use std::{cell::Cell, rc::Rc};
#[test]
fn test_initial_pane_and_appearance() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        let t = d.terminal(0);
        assert_eq!(d.panel.header_labels(), ["bash1"]);
        assert_eq!(t.scrollback_lines(), 10000);
        assert_eq!(t.font().unwrap().to_string(), "Monospace 11");
        command(&t, "printf '%s:%s\\n' \"$TERM\" \"$COLORTERM\"");
        poll(|| text(&t).contains("xterm-256color:truecolor"));
    });
}
#[test]
fn test_new_and_close_current() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        let p = d.panel.panes()[1].clone();
        let pid = p.pid().unwrap();
        d.panel.focus_pane(p.id);
        d.panel.close_current();
        assert_eq!(d.panel.pane_count(), 1);
        poll(|| dead(pid));
    });
}
#[test]
fn test_close_last_emits_once_without_respawn() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        d.panel.connect_last_closed(move |_| c.set(c.get() + 1));
        d.panel.close_current();
        d.panel.close_current();
        assert_eq!(count.get(), 1);
        assert_eq!(d.panel.pane_count(), 0);
    });
}
#[test]
fn test_complete_page_is_gated_and_control_can_opt_out() {
    run(|| {
        let d = Dock::new(false, 1);
        let p = d.panel.panes()[0].clone();
        assert_eq!(
            p.gate.as_ref().unwrap().child(),
            Some(p.page.clone().upcast())
        );
        assert_eq!(p.frame.parent(), Some(d.panel.body().upcast()));
        assert!(!p.frame.can_focus());
        d.panel.set_resize_gate(false);
        d.panel.new_terminal();
        assert!(d.panel.panes()[1].gate.is_none());
    });
}
#[test]
fn test_empty_panel_has_only_overlay_and_no_notebook() {
    run(|| {
        let d = Dock::new(false, 0);
        assert_eq!(
            d.panel.children(),
            [d.panel.overlay().upcast::<gtk::Widget>()]
        );
        assert!(d.panel.body().children().is_empty());
        assert!(d.panel.header_labels().is_empty());
    });
}
#[test]
fn test_pane_structure_header_above_terminal() {
    run(|| {
        let d = Dock::new(false, 1);
        let p = d.panel.panes()[0].clone();
        assert_eq!(p.page.orientation(), gtk::Orientation::Vertical);
        assert_eq!(
            p.page.children(),
            [
                p.header.widget.clone().upcast::<gtk::Widget>(),
                p.content.clone().upcast()
            ]
        );
        assert_eq!(p.content.children().len(), 3);
    });
}
#[test]
fn test_equal_horizontal_panes_and_insertion() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        let old = d.panel.panes()[1].id;
        let first = d.panel.panes()[0].id;
        d.panel.focus_pane(first);
        d.panel.new_terminal();
        assert_eq!(d.panel.panes()[2].id, old);
        pump(80);
        let s = d.panel.body().pane_sizes();
        assert!(s.iter().max().unwrap() - s.iter().min().unwrap() <= 1);
    });
}
#[test]
fn test_stacked_panes_are_rows_with_horizontal_dividers() {
    run(|| {
        let d = Dock::new(true, 3);
        d.ready();
        assert_eq!(d.panel.body().orientation(), gtk::Orientation::Vertical);
        let ps = d.panel.panes();
        assert!(ps[0].frame.allocation().y() < ps[1].frame.allocation().y());
        assert_eq!(d.panel.body().handles().len(), 2);
    });
}
#[test]
fn test_set_cwd_only_changes_new_shells() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        d.panel.set_cwd("/");
        d.panel.new_terminal();
        d.ready();
        for (i, path) in [(0, "/tmp"), (1, "/")] {
            let t = d.terminal(i);
            command(&t, "printf 'CWD=%s\\n' \"$PWD\"");
            poll(|| text(&t).contains(&format!("CWD={path}")));
        }
    });
}
#[test]
fn test_invalid_cwd_uses_home() {
    run(|| {
        let d = Dock::new(false, 0);
        d.panel.set_cwd("/missing/directory");
        d.panel.new_terminal();
        d.ready();
        let t = d.terminal(0);
        command(&t, "printf 'CWD=%s\\n' \"$PWD\"");
        let home = std::env::var("HOME").unwrap();
        poll(|| text(&t).contains(&format!("CWD={home}")));
    });
}
#[test]
fn test_shell_exit_removes_panes_and_emits_only_for_last() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        let count = Rc::new(Cell::new(0));
        let c = count.clone();
        d.panel.connect_last_closed(move |_| c.set(c.get() + 1));
        command(&d.terminal(1), "exit");
        poll(|| d.panel.pane_count() == 1);
        assert_eq!(count.get(), 0);
        command(&d.terminal(0), "exit");
        poll(|| d.panel.pane_count() == 0);
        assert_eq!(count.get(), 1);
    });
}
fn background(max: bool) {
    let d = Dock::new(false, 2);
    d.ready();
    let p = d.panel.panes()[1].clone();
    if max {
        d.panel.toggle_maximize_pane(p.id);
    }
    d.window.remove(&d.panel);
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let editor = gtk::Entry::new();
    b.pack_start(&editor, false, false, 0);
    b.pack_start(&d.panel, true, true, 0);
    d.window.add(&b);
    d.window.show_all();
    editor.grab_focus();
    command(p.terminal.as_ref().unwrap(), "exit");
    poll(|| d.panel.pane_count() == 1);
    assert_eq!(d.window.focused_widget(), Some(editor.upcast()));
}
#[test]
fn test_background_shell_exit_preserves_external_focus() {
    run(|| background(false));
}
#[test]
fn test_background_maximized_shell_exit_preserves_external_focus() {
    run(|| background(true));
}
#[test]
fn test_stale_focus_idle_preserves_later_pane_selection() {
    run(|| {
        let d = Dock::new(false, 1);
        d.panel.new_terminal();
        let id = d.panel.panes()[0].id;
        d.panel.focus_pane(id);
        d.ready();
        assert_eq!(d.panel.current_pane().unwrap().id, id);
    });
}
#[test]
fn test_reparent_keeps_shell_and_output() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        let pid = d.panel.panes()[0].pid();
        d.window.remove(&d.panel);
        let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
        b.add(&d.panel);
        d.window.add(&b);
        d.window.show_all();
        let t = d.terminal(0);
        command(&t, "printf '%s%s\\n' 'reparent-' 'ok'");
        poll(|| text(&t).contains("reparent-ok"));
        assert_eq!(d.panel.panes()[0].pid(), pid);
    });
}
#[test]
fn test_switch_focus_and_zoom_shortcuts() {
    run(|| {
        let d = Dock::new(false, 1);
        let t = d.terminal(0);
        let m = gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK;
        for (key, expected) in [
            (gdk::keys::constants::plus, 1.1),
            (gdk::keys::constants::minus, 1.0),
            (gdk::keys::constants::_0, 1.0),
        ] {
            assert_eq!(d.panel.handle_key(&t, key, m), glib::Propagation::Stop);
            assert!((t.font_scale() - expected).abs() < 1e-9);
        }
        for _ in 0..40 {
            d.panel.handle_key(&t, gdk::keys::constants::plus, m);
        }
        assert_eq!(t.font_scale(), 3.0);
        for _ in 0..40 {
            d.panel.handle_key(&t, gdk::keys::constants::minus, m);
        }
        assert_eq!(t.font_scale(), 0.5);
    });
}
#[test]
fn test_focus_navigation_and_split_close_keys() {
    run(|| {
        let d = Dock::new(false, 1);
        let m = gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK;
        d.panel
            .handle_key(&d.terminal(0), gdk::keys::constants::t, m);
        assert_eq!(d.panel.pane_count(), 2);
        let t = d.terminal(1);
        d.panel.handle_key(&t, gdk::keys::constants::w, m);
        assert_eq!(d.panel.pane_count(), 1);
    });
}
#[test]
fn test_stacked_focus_moves_with_up_down_only_when_stacked() {
    run(|| {
        for stacked in [false, true] {
            let d = Dock::new(stacked, 2);
            d.ready();
            let m = gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::MOD1_MASK;
            d.panel.focus_pane(d.panel.panes()[0].id);
            let r = d
                .panel
                .handle_key(&d.terminal(0), gdk::keys::constants::Down, m);
            assert_eq!(
                r,
                if stacked {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            );
            if stacked {
                assert_eq!(d.window.focused_widget(), Some(d.terminal(1).upcast()));
            }
        }
    });
}
#[test]
fn test_apply_palette_changes_existing_and_new_terminals() {
    run(|| {
        let d = Dock::new(false, 1);
        let ansi = std::array::from_fn::<_, 16, _>(|_| "#112233".to_string());
        d.panel
            .apply_palette("#112233", "#445566", &ansi, Some("#aabbcc"))
            .unwrap();
        d.panel.new_terminal();
        d.ready();
        for p in d.panel.panes() {
            let color = p
                .gate
                .as_ref()
                .unwrap()
                .style_context()
                .style_property_for_state("background-color", gtk::StateFlags::NORMAL)
                .get::<gdk::RGBA>()
                .unwrap();
            assert!((color.red() - 68.0 / 255.0).abs() < 0.001);
            assert!((color.green() - 85.0 / 255.0).abs() < 0.001);
        }
    });
}
#[test]
fn test_cursor_and_scrollback_apis_reach_existing_and_new_terminals() {
    run(|| {
        let d = Dock::new(false, 1);
        d.panel.set_cursor_shape("ibeam").unwrap();
        d.panel.set_cursor_blink(false);
        d.panel.set_scrollback(25000);
        d.panel.new_terminal();
        for p in d.panel.panes() {
            let t = p.terminal.as_ref().unwrap();
            assert_eq!(t.scrollback_lines(), 25000);
            assert_eq!(t.cursor_shape(), impro_vte::CursorShape::Ibeam);
            assert_eq!(t.cursor_blink_mode(), impro_vte::CursorBlinkMode::Off);
        }
        for (v, e) in [(5, 1000), (-1, 1000), (1000000000, 100000)] {
            d.panel.set_scrollback(v);
            assert_eq!(d.terminal(0).scrollback_lines(), e);
        }
        assert!(d.panel.set_cursor_shape("beam").is_err());
    });
}
#[test]
fn test_focus_events_toggle_style_class() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        let p = d.panel.panes();
        d.panel.focus_pane(p[0].id);
        pump(20);
        assert!(p[0].header.widget.style_context().has_class("active"));
        d.panel.focus_pane(p[1].id);
        assert!(!p[0].header.widget.style_context().has_class("active"));
        assert!(p[1].header.widget.style_context().has_class("active"));
    });
}
#[test]
fn test_copy_paste_shortcuts() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        let t = d.terminal(0);
        t.feed(b"clipboard_ok\r\n");
        pump(40);
        t.select_all();
        let m = gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK;
        d.panel.handle_key(&t, gdk::keys::constants::c, m);
        let clipboard = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD);
        assert!(clipboard.wait_for_text().unwrap().contains("clipboard_ok"));
        clipboard.set_text("printf '%s%s\\n' 'paste_' 'ok'");
        d.panel.handle_key(&t, gdk::keys::constants::v, m);
        pump(50);
        t.feed_child(b"\n");
        poll(|| text(&t).contains("paste_ok"));
    });
}
#[test]
fn test_context_menu_split_and_kill() {
    run(|| {
        let d = Dock::new(false, 1);
        let menu = d.panel.context_menu(&d.terminal(0));
        let children = menu.children();
        let labels: Vec<_> = children
            .iter()
            .filter_map(|w| w.downcast_ref::<gtk::MenuItem>())
            .filter_map(|i| i.label())
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(
            labels,
            [
                "Copy",
                "Paste",
                "Select All",
                "Clear",
                "Split Terminal",
                "Kill Terminal"
            ]
        );
        children[5]
            .clone()
            .downcast::<gtk::MenuItem>()
            .unwrap()
            .activate();
        assert_eq!(d.panel.pane_count(), 2);
        children[6]
            .clone()
            .downcast::<gtk::MenuItem>()
            .unwrap()
            .activate();
        assert_eq!(d.panel.pane_count(), 1);
        unsafe {
            menu.destroy();
        }
    });
}
#[test]
fn test_destroy_terminates_children() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        let pids: Vec<_> = d.panel.panes().iter().filter_map(|p| p.pid()).collect();
        unsafe {
            d.panel.destroy();
        }
        poll(|| pids.iter().all(|p| dead(*p)));
        assert_eq!(d.panel.pane_count(), 0);
    });
}
#[test]
fn test_destroy_during_spawn() {
    run(|| {
        let d = Dock::new(false, 1);
        let p = d.panel.panes()[0].clone();
        unsafe {
            d.panel.destroy();
        }
        pump(1100);
        assert!(p.cancellable().is_cancelled());
        assert!(p.pid().is_none());
    });
}
#[test]
fn test_pick_shell() {
    run(|| {
        let old = std::env::var_os("SHELL");
        std::env::set_var("SHELL", "/bin/sh");
        assert_eq!(pick_shell(), std::path::Path::new("/bin/sh"));
        std::env::set_var("SHELL", "/missing/shell");
        assert_eq!(pick_shell(), std::path::Path::new("/bin/bash"));
        if let Some(old) = old {
            std::env::set_var("SHELL", old);
        } else {
            std::env::remove_var("SHELL");
        }
    });
}
#[test]
fn test_safe_cwd() {
    run(|| {
        let old = std::env::var_os("HOME");
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp.path());
        assert_eq!(safe_cwd(tmp.path()), tmp.path());
        assert_eq!(safe_cwd("/missing"), tmp.path());
        std::env::set_var("HOME", "/missing");
        assert_eq!(safe_cwd("/missing"), std::path::Path::new("/"));
        if let Some(old) = old {
            std::env::set_var("HOME", old);
        } else {
            std::env::remove_var("HOME");
        }
    });
}
#[test]
fn test_reentrant_limit_and_close_callbacks() {
    run(|| {
        let d = Dock::new(false, 1);
        d.panel.set_max_panes(1);
        d.panel.connect_limit_reached(|p| p.close_current());
        assert!(!d.panel.new_terminal());
        assert_eq!(d.panel.pane_count(), 0);
        let weak = d.panel.downgrade();
        d.panel.connect_last_closed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.new_terminal();
            }
        });
        d.panel.new_terminal();
        d.panel.close_current();
        assert_eq!(d.panel.pane_count(), 1);
    });
}
#[test]
fn test_reentrant_maximize_shutdown_and_custom_factory() {
    run(|| {
        let d = Dock::new(false, 0);
        d.panel.set_factory("graph", {
            let weak = d.panel.downgrade();
            move || {
                if let Some(p) = weak.upgrade() {
                    p.set_max_panes(1);
                }
                gtk::Entry::new().upcast()
            }
        });
        assert!(d.panel.add_graph(None));
        d.panel.connect_maximize_toggled(|p, on| {
            if on {
                p.close_kind("graph");
            }
        });
        d.panel.toggle_maximize();
        assert_eq!(d.panel.pane_count(), 0);
    });
}
#[test]
fn test_kill_releases_widgets_and_handlers() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        let weak = {
            let p = d.panel.panes()[0].clone();
            p.frame.downgrade()
        };
        let terminal = d.terminal(0).downgrade();
        d.panel.close_current();
        poll(|| weak.upgrade().is_none() && terminal.upgrade().is_none());
    });
}
#[test]
fn test_discrete_layout_commits_without_waiting_for_pace() {
    run(|| {
        let d = Dock::new(false, 2);
        d.ready();
        for p in d.panel.panes() {
            p.gate.as_ref().unwrap().set_pace_ms(10000);
        }
        d.panel.toggle_maximize();
        poll(|| {
            let p = d.panel.current_pane().unwrap();
            p.gate.as_ref().unwrap().committed() == p.gate.as_ref().unwrap().target()
                && p.frame.allocated_width() > 1100
        });
        d.panel.restore();
        poll(|| {
            d.panel
                .panes()
                .iter()
                .all(|p| p.gate.as_ref().unwrap().committed() == p.gate.as_ref().unwrap().target())
        });
    });
}
#[test]
fn test_graph_lifecycle_numbering_focus_and_terminal_settings() {
    run(|| {
        for stacked in [false, true] {
            let d = Dock::new(stacked, 1);
            let e = gtk::Entry::new();
            d.panel.add_graph(Some(e.upcast_ref()));
            d.panel.new_terminal();
            assert_eq!(d.panel.header_labels(), ["bash1", "Graph view", "bash2"]);
            assert_eq!(d.panel.graph_count(), 1);
            d.panel.set_font_size(13);
            d.panel.set_scrollback(20000);
            assert_eq!(d.panel.graph_views(), [e.upcast::<gtk::Widget>()]);
            d.panel.close_kind("graph");
            assert!(!d.panel.has_graph());
            assert_eq!(d.panel.header_labels(), ["bash1", "bash2"]);
        }
    });
}
#[test]
fn test_chooser_factory_is_lazy_and_duplicate_graph_focuses_without_limit() {
    run(|| {
        let d = Dock::new(false, 0);
        let calls = Rc::new(Cell::new(0));
        let c = calls.clone();
        d.panel.set_factory("graph", move || {
            c.set(c.get() + 1);
            gtk::Entry::new().upcast()
        });
        assert_eq!(calls.get(), 0);
        assert!(d.panel.add_graph(None));
        d.panel.set_max_panes(1);
        assert!(!d.panel.add_graph(None));
        assert_eq!(calls.get(), 1);
        assert!(!d.panel.toast().widget().is_visible());
    });
}
#[test]
fn test_chooser_activation_keeps_new_pane_focus() {
    run(|| {
        let d = Dock::new(false, 1);
        d.ready();
        d.panel.set_factory("graph", || gtk::Entry::new().upcast());
        d.panel.add_button().open();
        d.panel.add_button().choose("graph");
        pump(40);
        assert_eq!(
            d.window.focused_widget(),
            d.panel.graph_views().first().cloned()
        );
    });
}
#[test]
fn test_last_graph_close_and_destroyed_reject() {
    run(|| {
        let d = Dock::new(false, 0);
        d.panel.add_graph(Some(gtk::Entry::new().upcast_ref()));
        let calls = Rc::new(Cell::new(0));
        let c = calls.clone();
        d.panel.connect_last_closed(move |_| c.set(c.get() + 1));
        d.panel.close_kind("graph");
        assert_eq!(calls.get(), 1);
        unsafe {
            d.panel.destroy();
        }
        assert!(!d.panel.add_graph(Some(gtk::Entry::new().upcast_ref())));
    });
}
#[test]
fn test_gitgraph_lifecycle_limits_focus_and_numbering() {
    run(|| {
        let d = Dock::new(false, 1);
        d.panel.add_gitgraph(Some(gtk::Entry::new().upcast_ref()));
        d.panel.add_graph(Some(gtk::Entry::new().upcast_ref()));
        assert_eq!(
            d.panel.header_labels(),
            ["bash1", "Git graph", "Graph view"]
        );
        d.panel.set_max_panes(3);
        assert!(!d.panel.new_terminal());
        assert_eq!(d.panel.toast().widget().text(), "Cannot add (limit 3)");
        assert!(d.panel.has_gitgraph());
        d.panel.close_kind("gitgraph");
        assert_eq!(d.panel.gitgraph_count(), 0);
    });
}
#[test]
fn test_gitgraph_animation_releases_limit_and_shuts_down_immediately() {
    run(|| {
        let d = Dock::new(false, 0);
        let calls = Rc::new(Cell::new(0));
        let c = calls.clone();
        d.panel.add_custom_pane(
            "gitgraph",
            &gtk::Entry::new(),
            Some(Box::new(move || c.set(c.get() + 1))),
        );
        pump(40);
        d.panel.set_animation_ms(100);
        d.panel.set_max_panes(1);
        d.panel.close_current();
        assert_eq!(calls.get(), 1);
        assert!(d.panel.new_terminal());
        poll(|| d.panel.closing_panes().is_empty());
        assert_eq!(calls.get(), 1);
    });
}
#[test]
fn test_reentrant_parent_signals_respect_limit_and_destroy() {
    run(|| {
        let d = Dock::new(false, 0);
        d.panel.set_max_panes(1);
        let nested = Rc::new(Cell::new(None));
        let result = nested.clone();
        let weak = d.panel.downgrade();
        let view = gtk::Entry::new();
        view.connect_parent_set(move |view, _| {
            if view.parent().is_some() {
                if let Some(p) = weak.upgrade() {
                    result.set(Some(p.new_terminal()));
                }
            }
        });
        assert!(d.panel.add_graph(Some(view.upcast_ref())));
        assert_eq!(nested.get(), Some(false));
        assert_eq!(d.panel.pane_count(), 1);
        d.panel.close_current();
        d.panel.set_max_panes(6);
        let unique = Rc::new(Cell::new(None));
        let result = unique.clone();
        let weak = d.panel.downgrade();
        let view = gtk::Entry::new();
        view.connect_parent_set(move |view, _| {
            if view.parent().is_some() {
                if let Some(p) = weak.upgrade() {
                    result.set(Some(p.add_graph(Some(gtk::Entry::new().upcast_ref()))));
                }
            }
        });
        assert!(d.panel.add_graph(Some(view.upcast_ref())));
        assert_eq!(unique.get(), Some(false));
        assert_eq!(d.panel.graph_count(), 1);
        d.panel.close_current();
        let weak = d.panel.downgrade();
        let view = gtk::Entry::new();
        view.connect_parent_set(move |view, _| {
            if view.parent().is_some() {
                if let Some(p) = weak.upgrade() {
                    unsafe {
                        p.destroy();
                    }
                }
            }
        });
        assert!(!d.panel.add_graph(Some(view.upcast_ref())));
        assert_eq!(d.panel.pane_count(), 0);
    });
}
