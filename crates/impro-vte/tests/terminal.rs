use gtk::prelude::*;
use impro_vte::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
fn pump_until(f: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while !f() {
        assert!(Instant::now() < deadline, "VTE operation timed out");
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn test_terminal_spawn_feed_selection_configuration_and_signals() {
    gtk::init().unwrap();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    let terminal = Terminal::new();
    window.add(&terminal);
    window.show_all();
    terminal.set_font(&pango::FontDescription::from_string("Monospace 11"));
    terminal.set_size(80, 24);
    terminal.set_font_scale(1.2);
    assert_eq!(terminal.font_scale(), 1.2);
    terminal.set_scrollback_lines(1000);
    assert_eq!(terminal.scrollback_lines(), 1000);
    terminal.set_cursor_shape(CursorShape::Ibeam);
    terminal.set_cursor_blink_mode(CursorBlinkMode::Off);
    terminal.set_audible_bell(false);
    terminal.set_allow_hyperlink(true);
    assert!(terminal.allow_hyperlink());
    terminal.set_colors(Some(&gdk::RGBA::BLACK), Some(&gdk::RGBA::WHITE), &[]);
    let title = Rc::new(Cell::new(false));
    let t = title.clone();
    terminal.connect_window_title_changed(move |_| t.set(true));
    let content = Rc::new(Cell::new(false));
    let c = content.clone();
    terminal.connect_contents_changed(move |_| c.set(true));
    terminal.feed(b"hello terminal\r\n\x1b]0;impro test\x07");
    pump_until(|| content.get() && title.get());
    assert_eq!(terminal.window_title().as_deref(), Some("impro test"));
    assert!(terminal
        .text_format(Format::Text)
        .unwrap()
        .contains("hello terminal"));
    terminal.select_all();
    assert!(terminal.has_selection());
    terminal.copy_clipboard_format(Format::Text);
    terminal.unselect_all();
    assert!(!terminal.has_selection());
    let exited = Rc::new(Cell::new(false));
    let e = exited.clone();
    terminal.connect_child_exited(move |_, status| {
        assert_eq!(status, 0);
        e.set(true);
    });
    let spawned = Rc::new(RefCell::new(None));
    let s = spawned.clone();
    terminal.spawn_async(
        Some("/tmp"),
        &[
            "/bin/sh",
            "-c",
            "read answer; printf 'child=%s env=%s' \"$answer\" \"$IMPRO_VTE_TEST\"",
        ],
        &["IMPRO_VTE_TEST=alive"],
        glib::SpawnFlags::DEFAULT,
        -1,
        None,
        move |result| {
            *s.borrow_mut() = Some(result);
        },
    );
    pump_until(|| spawned.borrow().is_some());
    assert!(spawned.borrow().as_ref().unwrap().is_ok());
    assert!(terminal.pty().is_some());
    terminal.feed_child(b"reply\n");
    pump_until(|| exited.get());
    pump_until(|| {
        terminal
            .text_format(Format::Text)
            .is_some_and(|s| s.contains("child=reply env=alive"))
    });
    unsafe {
        window.destroy();
    }
}
