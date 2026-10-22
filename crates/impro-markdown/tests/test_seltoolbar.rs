//! Real GTK selection formatting, visibility and keyboard integration
//! (port of `test_seltoolbar`).
mod support;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_editor::EditorTab;
use impro_markdown::seltoolbar::{self, SelectionToolbar};
use std::{cell::Cell, path::PathBuf, rc::Rc};
use support::*;

struct Fx {
    h: Harness,
    tab: Rc<EditorTab>,
    path: PathBuf,
}
impl Fx {
    fn new() -> Self {
        impro_ui::motion::set_reduce_motion(true);
        let h = Harness::sized(780, 600);
        let text = format!("A note\n\nAn editable paragraph with selected words.\n{}", "\nMore text.".repeat(100));
        let tab = h.note("note.md", &text);
        let path = tab.path();
        tab.view.grab_focus();
        let fx = Self { h, tab, path };
        pump(200);
        fx
    }
    fn toolbar(&self) -> SelectionToolbar {
        seltoolbar::of(&self.tab.view).expect("selection toolbar")
    }
    fn text(&self) -> String {
        text_of(&self.tab.buffer)
    }
    fn select_range(&self, start: i32, end: i32) {
        self.tab
            .buffer
            .select_range(&self.tab.buffer.iter_at_offset(end), &self.tab.buffer.iter_at_offset(start));
    }
    fn select(&self, word: &str) {
        let text = self.text();
        let start = text[..text.find(word).unwrap()].chars().count() as i32;
        self.select_range(start, start + word.chars().count() as i32);
        pump(200);
    }
    fn selected(&self) -> String {
        let (a, b) = self.tab.buffer.selection_bounds().unwrap();
        self.tab.buffer.text(&a, &b, true).unwrap().to_string()
    }
    fn visible(&self) -> bool {
        self.toolbar().popover().is_visible()
    }
    fn key(&self, key: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        let handled = emit_key(&self.tab.view, key, state);
        pump(200);
        handled
    }
}
impl Drop for Fx {
    fn drop(&mut self) {
        impro_ui::motion::set_reduce_motion(false);
    }
}
fn active(button: &gtk::Button) -> bool {
    button.style_context().has_class("active")
}

#[test]
fn test_debounce_collapse_escape_and_new_selection() {
    run(|| {
        let fx = Fx::new();
        fx.select("selected");
        assert!(fx.visible());
        assert_eq!(fx.toolbar().popover().position(), gtk::PositionType::Top);
        assert_eq!(fx.toolbar().popover().opacity(), 1.0);
        assert_eq!(fx.toolbar().popover().margin_top(), 0);
        assert!(fx.key(k::Escape, gdk::ModifierType::empty()));
        assert!(!fx.visible());
        fx.tab.buffer.place_cursor(&fx.tab.buffer.start_iter());
        fx.select("selected");
        assert!(fx.visible());
        fx.select("words");
        assert!(fx.visible());
        fx.tab.buffer.place_cursor(&fx.tab.buffer.end_iter());
        pump(10);
        assert!(!fx.visible());
        fx.select("selected");
        fx.toolbar().dismiss();
        fx.select("paragraph");
        assert!(fx.visible());
    });
}

#[test]
fn test_mouse_drag_waits_for_release_and_keyboard_debounces() {
    run(|| {
        let fx = Fx::new();
        let window = TextViewExt::window(&fx.tab.view, gtk::TextWindowType::Text).unwrap();
        let rect = fx.tab.view.iter_location(&fx.tab.buffer.iter_at_offset(40));
        let (x, y) = fx
            .tab
            .view
            .buffer_to_window_coords(gtk::TextWindowType::Text, rect.x(), rect.y());
        let press = button_event_at(gdk::EventType::ButtonPress, &window, f64::from(x), f64::from(y), 1);
        let _: bool = fx.tab.view.emit_by_name("button-press-event", &[&press]);
        fx.select("selected");
        assert!(!fx.visible());
        let release = button_event_at(gdk::EventType::ButtonRelease, &window, f64::from(x), f64::from(y), 1);
        let _: bool = fx.tab.view.emit_by_name("button-release-event", &[&release]);
        pump(50);
        assert!(!fx.visible());
        pump(200);
        assert!(fx.visible());
        fx.toolbar().hide();
        fx.select_range(12, 15);
        pump(80);
        fx.select_range(12, 16);
        pump(80);
        assert!(!fx.visible());
        pump(200);
        assert!(fx.visible());
    });
}

#[test]
fn test_toggle_active_chain_and_one_step_undo() {
    run(|| {
        let fx = Fx::new();
        fx.select("selected");
        let before = fx.text();
        let buttons = fx.toolbar().buttons();
        buttons["bold"].clicked();
        pump(200);
        assert!(fx.text().contains("**selected**"));
        assert_eq!(fx.selected(), "selected");
        assert!(active(&buttons["bold"]));
        buttons["italic"].clicked();
        pump(200);
        assert!(fx.text().contains("***selected***"));
        for command in ["bold", "italic"] {
            assert!(active(&buttons[command]), "{command}");
        }
        assert_eq!(fx.selected(), "selected");
        buttons["bold"].clicked();
        pump(200);
        assert!(fx.text().contains("*selected*"));
        assert!(!fx.text().contains("**selected**"));
        assert!(!active(&buttons["bold"]));
        fx.tab.undo(false);
        assert!(fx.text().contains("***selected***"));
        fx.tab.undo(false);
        assert!(fx.text().contains("**selected**"));
        assert!(!fx.text().contains("***selected***"));
        fx.tab.undo(false);
        assert_eq!(fx.text(), before);
    });
}

#[test]
fn test_other_inline_formats_link_copy_and_turn_into() {
    run(|| {
        let fx = Fx::new();
        fx.select("selected");
        let buttons = fx.toolbar().buttons();
        for (command, expected) in [
            ("strike", "~~selected~~"),
            ("inline_code", "`selected`"),
            ("link", "[selected](url)"),
        ] {
            buttons[command].clicked();
            pump(200);
            assert!(fx.text().contains(expected), "{command}");
            assert_eq!(fx.selected(), "selected");
            assert!(active(&buttons[command]), "{command}");
            buttons[command].clicked();
            pump(200);
            assert!(!fx.text().contains(expected), "{command}");
        }
        buttons["copy"].clicked();
        let clipboard = gtk::Clipboard::default(&gdk::Display::default().unwrap()).unwrap();
        assert_eq!(clipboard.wait_for_text().unwrap(), "selected");
        buttons["turn"].clicked();
        pump(200);
        assert!(fx.toolbar().menu().is_visible());
        let ids: Vec<_> = fx.toolbar().menu_buttons().iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, ["text", "h1", "h2", "h3", "bullet", "numbered", "todo", "quote"]);
        let h2 = fx
            .toolbar()
            .menu_buttons()
            .into_iter()
            .find(|(id, _)| *id == "h2")
            .unwrap()
            .1;
        h2.clicked();
        pump(200);
        assert!(fx.text().contains("\n## An editable paragraph"));
        assert_eq!(fx.selected(), "selected");
        assert!(!fx.toolbar().menu().is_visible());
    });
}

#[test]
fn test_focus_loss_tab_switch_typing_scroll_and_slash() {
    run(|| {
        let fx = Fx::new();
        fx.select("selected");
        let _: bool = fx
            .tab
            .view
            .emit_by_name("focus-out-event", &[&bare_event(gdk::EventType::FocusChange)]);
        assert!(!fx.visible());
        fx.select("words");
        assert!(fx.visible());
        fx.tab.source_scroll.vadjustment().set_value(1000.0);
        pump(200);
        assert!(!fx.visible());
        fx.tab.source_scroll.vadjustment().set_value(0.0);
        fx.select("paragraph");
        assert!(fx.visible());
        fx.tab.buffer.begin_user_action();
        fx.tab.buffer.delete_selection(true, true);
        fx.tab.buffer.insert_at_cursor("replacement");
        fx.tab.buffer.end_user_action();
        pump(200);
        assert!(!fx.visible());
        fx.select("selected");
        let other = fx.h.path(".impro/Notes/other.md");
        std::fs::write(&other, "other").unwrap();
        fx.h.panel.open_file(&other);
        poll(|| !fx.h.panel.is_busy());
        pump(200);
        assert!(!fx.visible());
        fx.h.panel.open_file(&fx.path);
        poll(|| !fx.h.panel.is_busy());
        fx.tab.view.grab_focus();
        fx.select("words");
        impro_markdown::slashmenu::for_view(&fx.tab.view).unwrap().popover().show();
        pump(200);
        assert!(!fx.visible());
    });
}

#[test]
fn test_readonly_source_non_notes_and_enabled_gate() {
    run(|| {
        let fx = Fx::new();
        fx.select("selected");
        fx.tab.view.set_editable(false);
        pump(200);
        assert!(!fx.visible());
        fx.tab.view.set_editable(true);
        fx.select("words");
        seltoolbar::set_enabled(&fx.h.panel, false);
        pump(200);
        assert!(!fx.visible());
        let before = fx.text();
        fx.key(k::b, gdk::ModifierType::CONTROL_MASK);
        assert_eq!(fx.text(), before);
        seltoolbar::set_enabled(&fx.h.panel, true);
        fx.select("paragraph");
        assert!(fx.visible());
        fx.tab.set_markdown_mode("source");
        assert!(seltoolbar::of(&fx.tab.view).is_none());
        for name in ["readme.md", "code.py"] {
            let tab = fx.h.open(name, "selected words");
            assert!(seltoolbar::of(&tab.view).is_none(), "{name}");
        }
    });
}

#[test]
fn test_long_selection_stays_visible_while_selected_text_intersects_viewport() {
    run(|| {
        let fx = Fx::new();
        let (a, b) = fx.tab.buffer.bounds();
        fx.tab.buffer.select_range(&b, &a);
        pump(200);
        assert!(fx.visible());
        fx.tab.source_scroll.vadjustment().set_value(800.0);
        pump(200);
        assert!(fx.visible());
    });
}

#[test]
fn test_first_line_uses_space_below_and_buttons_are_compact() {
    run(|| {
        let fx = Fx::new();
        fx.select("A note");
        assert_eq!(fx.toolbar().popover().position(), gtk::PositionType::Bottom);
        let sizes: std::collections::BTreeSet<(i32, i32)> = fx
            .toolbar()
            .buttons()
            .values()
            .map(|b| (b.allocated_width(), b.allocated_height()))
            .collect();
        assert_eq!(sizes, [(28, 28)].into_iter().collect());
    });
}

#[test]
fn test_shortcuts_and_cleanup_pending_debounce() {
    run(|| {
        let fx = Fx::new();
        fx.select("selected");
        let handled = emit_key(&fx.h.window, k::b, gdk::ModifierType::CONTROL_MASK);
        pump(200);
        assert!(handled);
        assert!(fx.text().contains("**selected**"));
        assert!(fx.key(k::i, gdk::ModifierType::CONTROL_MASK));
        assert!(fx.text().contains("***selected***"));
        let toolbar = fx.toolbar();
        toolbar.queue();
        fx.tab.set_markdown_mode("source");
        pump(200);
        assert!(toolbar.is_destroyed());
        assert!(!toolbar.timeout_pending());
    });
}

#[test]
fn test_real_window_shortcut_precedes_explorer_accelerator_and_settings_apply_live() {
    run(|| {
        let fx = Fx::new();
        // The window's own accelerator (the explorer toggle) must lose to the note shortcut.
        let toggled = Rc::new(Cell::new(0));
        let group = gtk::AccelGroup::new();
        let counter = toggled.clone();
        group.connect_accel_group(
            *k::b,
            gdk::ModifierType::CONTROL_MASK,
            gtk::AccelFlags::VISIBLE,
            move |_, _, _, _| {
                counter.set(counter.get() + 1);
                true
            },
        );
        fx.h.window.add_accel_group(&group);
        fx.select_range(8, 16);
        pump(200);
        assert!(fx.visible());
        let handled = emit_key(&fx.h.window, k::b, gdk::ModifierType::CONTROL_MASK);
        pump(200);
        assert!(handled);
        assert_eq!(toggled.get(), 0);
        assert!(fx.text().contains("**"));
        // The three note settings gate the toolbar live and reopen with a new selection.
        for _setting in ["notes_selection_toolbar", "notes_toolbar", "feature_notes"] {
            fx.h.panel.set_selection_toolbar(false);
            pump(200);
            assert!(!fx.visible());
            fx.h.panel.set_selection_toolbar(true);
            fx.tab.buffer.place_cursor(&fx.tab.buffer.end_iter());
            fx.select_range(8, 16);
            pump(200);
            assert!(fx.visible());
        }
    });
}
