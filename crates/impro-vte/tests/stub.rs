//! Windows stub backend, exercised on any platform: `cargo test -p impro-vte --features stub-backend`.
#![cfg(feature = "stub-backend")]
use gtk::prelude::*;
use impro_vte::stub::{Terminal, NOTICE};
use impro_vte::{CursorBlinkMode, CursorShape, Format};
use std::{cell::Cell, rc::Rc};

fn pump() {
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
}

// One test only: GTK may be initialised on a single thread of the test process.
#[test]
fn test_stub_terminal() {
    gtk::init().unwrap();
    widget_signals_and_defaults();
    spawn_reports_failure_through_the_callback();
}

fn widget_signals_and_defaults() {
    let t = Terminal::new();
    // The widget is a normal GTK container showing the notice.
    let label = t
        .children()
        .into_iter()
        .find_map(|c| c.downcast::<gtk::Label>().ok())
        .expect("notice label");
    assert_eq!(label.text(), NOTICE);
    assert!(t.vadjustment().is_some() && t.hadjustment().is_some());

    // Every signal can be connected; none ever fires by itself.
    let fired = Rc::new(Cell::new(0));
    macro_rules! count {
        ($connect:ident) => {{
            let fired = fired.clone();
            t.$connect(move |_| fired.set(fired.get() + 1));
        }};
    }
    count!(connect_selection_changed);
    count!(connect_contents_changed);
    count!(connect_window_title_changed);
    count!(connect_current_directory_uri_changed);
    let exited = fired.clone();
    t.connect_child_exited(move |_, _| exited.set(exited.get() + 1));
    let committed = fired.clone();
    t.connect_commit(move |_, _| committed.set(committed.get() + 1));
    t.feed(b"hello");
    t.feed_child(b"hello");
    t.select_all();
    pump();
    assert_eq!(fired.get(), 0);

    // Signals still work when emitted explicitly (host code relies on connect_local names).
    t.emit_by_name::<()>("child-exited", &[&7i32]);
    t.emit_by_name::<()>("commit", &[&"x", &1u32]);
    assert_eq!(fired.get(), 2);

    // Round-tripping settings and inert queries.
    t.set_size(100, 30);
    assert_eq!((t.column_count(), t.row_count()), (100, 30));
    t.set_scrollback_lines(500);
    assert_eq!(t.scrollback_lines(), 500);
    t.set_font_scale(1.5);
    assert_eq!(t.font_scale(), 1.5);
    t.set_cursor_shape(CursorShape::Ibeam);
    assert_eq!(t.cursor_shape(), CursorShape::Ibeam);
    t.set_cursor_blink_mode(CursorBlinkMode::Off);
    assert_eq!(t.cursor_blink_mode(), CursorBlinkMode::Off);
    t.set_rewrap_on_resize(false);
    assert!(!t.rewrap_on_resize());
    t.set_colors(None, None, &[]);
    assert!(t.char_width() > 0 && t.char_height() > 0);
    assert!(t.pty().is_none() && !t.has_selection());
    assert!(t.text_format(Format::Text).is_none());
    assert_eq!(t.cursor_position(), (0, 0));
}

fn spawn_reports_failure_through_the_callback() {
    let t = Terminal::new();
    let result = Rc::new(Cell::new(None::<bool>));
    let r = result.clone();
    t.spawn_async(
        None,
        &["cmd.exe"],
        &[],
        glib::SpawnFlags::DEFAULT,
        -1,
        None::<&gio::Cancellable>,
        move |outcome| {
            let error = outcome.expect_err("stub never spawns");
            r.set(Some(error.matches(gio::IOErrorEnum::NotSupported)));
        },
    );
    assert_eq!(result.get(), None, "completion is asynchronous");
    pump();
    assert_eq!(result.get(), Some(true));
}
