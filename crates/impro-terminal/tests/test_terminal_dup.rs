#![cfg(unix)]
mod common;
use common::*;
use gtk::prelude::*;
fn slow(stacked: bool, locale: &str) {
    let d = BashDock::new(stacked, 1, true, locale);
    d.ready();
    for w in [1100, 800, 500, 320, 250, 420, 800, 1200] {
        d.dock.window.resize(w, 750);
        pump(8);
    }
    d.resize(1200, 750);
    d.assert_prompt();
    let pid = d.dock.panel.panes()[0].pid();
    d.dock.panel.hide_dock();
    pump(80);
    d.dock.panel.show_dock();
    pump(220);
    assert_eq!(pid, d.dock.panel.panes()[0].pid());
    d.assert_prompt();
    if locale == "C" {
        d.assert_wraps();
    }
}
#[test]
fn test_right_dock_user_locale() {
    run(|| slow(true, "C.UTF-8"));
}
#[test]
fn test_bottom_dock_user_locale() {
    run(|| slow(false, "C.UTF-8"));
}
#[test]
fn test_both_docks_c_locale() {
    run(|| {
        slow(true, "C");
        slow(false, "C");
    });
}
#[test]
fn test_gate_waits_for_shell_output_not_vte_resize_signal() {
    run(|| {
        let d = BashDock::new(false, 1, false, "C");
        d.ready();
        let p = d.dock.panel.panes()[0].clone();
        let g = p.gate.as_ref().unwrap();
        g.set_pace_ms(150);
        let pid = p.pid().unwrap();
        unsafe {
            libc::kill(pid, libc::SIGSTOP);
        }
        d.dock.window.resize(800, 750);
        pump(25);
        assert_eq!(g.waiting_count(), 1);
        p.terminal
            .as_ref()
            .unwrap()
            .emit_by_name::<()>("contents-changed", &[]);
        assert_eq!(g.waiting_count(), 1);
        poll(|| g.waiting_count() == 0);
        unsafe {
            libc::kill(pid, libc::SIGCONT);
        }
        pump(150);
        d.assert_prompt();
    });
}
