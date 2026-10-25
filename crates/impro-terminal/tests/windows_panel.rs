#![cfg(windows)]
use gtk::prelude::*;
use impro_terminal::TerminalPanel;
use std::time::{Duration, Instant};
fn wait(label: &str, f: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !f() {
        assert!(Instant::now() < deadline, "Windows dock timeout: {label}");
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn split_resize_close_and_input() {
    gtk::init().expect("Windows GTK display");
    impro_terminal::init();
    let panel = TerminalPanel::new(std::env::temp_dir(), false, false, 11);
    panel.set_animation_ms(0);
    panel.set_shell("cmd.exe");
    assert!(panel.new_terminal());
    assert!(panel.new_terminal());
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(1100, 650);
    window.add(&panel);
    window.show_all();
    wait("both children", || {
        panel.panes().iter().all(|p| p.pid().is_some())
    });
    for pane in panel.panes() {
        pane.terminal
            .as_ref()
            .unwrap()
            .feed_child(b"echo impro-dock-ok\r");
    }
    wait("input reaches both panes", || {
        panel.panes().iter().all(|p| {
            p.terminal
                .as_ref()
                .unwrap()
                .text_format(impro_vte::Format::Text)
                .unwrap_or_default()
                .contains("impro-dock-ok")
        })
    });
    window.resize(1300, 750);
    wait("resized grids", || {
        panel.panes().iter().all(|p| {
            let t = p.terminal.as_ref().unwrap();
            t.pty().is_some_and(|pty| {
                pty.size().unwrap() == (t.column_count() as i32, t.row_count() as i32)
            })
        })
    });
    panel.close_current();
    wait("one pane remains", || panel.panes().len() == 1);
    unsafe {
        window.destroy();
    }
}
