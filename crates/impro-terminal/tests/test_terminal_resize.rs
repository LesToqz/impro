mod common;
use common::*;
use gtk::prelude::*;
#[test]
fn test_user_scenario_bottom_dock_sidebar_drag() {
    run(|| {
        let d = BashDock::new(false, 3, false, "C");
        d.ready();
        for w in (650..1200).step_by(20) {
            d.dock.window.resize(w, 750);
            pump(8);
        }
        d.resize(1200, 750);
        d.assert_prompt();
        d.assert_wraps();
    });
}
#[test]
fn test_right_dock_shrink_then_expand() {
    run(|| {
        let d = BashDock::new(true, 2, false, "C");
        d.ready();
        d.resize(260, 750);
        d.resize(1200, 750);
        d.assert_prompt();
        d.assert_wraps();
    });
}
#[test]
fn test_repeated_cycles_fast_slow_and_bottom_divider() {
    run(|| {
        let d = BashDock::new(false, 3, false, "C");
        d.ready();
        for pause in [4, 16, 30] {
            for w in [1100, 850, 650, 850, 1200] {
                d.dock.window.resize(w, 750);
                pump(pause);
            }
            d.resize(1200, 750);
            d.assert_prompt();
        }
        for h in [220, 600, 750] {
            d.resize(1200, h);
            d.assert_prompt();
        }
        d.assert_wraps();
    });
}
#[test]
fn test_jumps_maximise_zoom_font_and_dock_toggle() {
    run(|| {
        let d = BashDock::new(false, 3, false, "C");
        d.ready();
        d.dock.panel.toggle_maximize();
        pump(200);
        d.assert_prompt();
        d.dock.panel.restore();
        pump(200);
        d.assert_prompt();
        d.dock.panel.set_font_size(14);
        pump(200);
        d.assert_prompt();
        d.dock.panel.hide_dock();
        pump(60);
        d.dock.panel.show_dock();
        pump(200);
        d.assert_prompt();
        d.assert_wraps();
    });
}
#[test]
fn test_gate_never_reflows_terminal_or_leaves_it_waiting() {
    run(|| {
        let d = BashDock::new(false, 2, false, "C");
        d.ready();
        let observed = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        for p in d.dock.panel.panes() {
            let o = observed.clone();
            p.terminal
                .as_ref()
                .unwrap()
                .connect_size_allocate(move |t, _| o.borrow_mut().push(t.rewrap_on_resize()));
        }
        for w in [900, 700, 1100, 1200] {
            d.resize(w, 750);
        }
        assert!(!observed.borrow().is_empty());
        assert!(observed.borrow().iter().all(|v| !*v));
        for p in d.dock.panel.panes() {
            assert!(p.terminal.as_ref().unwrap().rewrap_on_resize());
            let g = p.gate.as_ref().unwrap();
            assert_eq!(g.target(), g.committed());
            assert_eq!(g.waiting_count(), 0);
            assert!(!g.has_pending_sources());
        }
    });
}
