#![cfg(unix)]
mod common;
use common::*;
use gtk::prelude::*;
fn check(stacked: bool, maximized: bool, before_map: bool) {
    let d = BashDock::new(stacked, 1, false, "C.UTF-8");
    if before_map {
        d.dock.window.hide();
        pump(100);
        d.dock.window.show_all();
    }
    if maximized {
        d.dock.window.resize(1800, 1000);
    }
    d.ready();
    d.assert_prompt();
    let t = d.dock.terminal(0);
    let grids = d.grids.borrow();
    let grid = grids.iter().find(|(term, _, _)| term == &t).unwrap();
    assert_eq!((grid.1, grid.2), (t.column_count(), t.row_count()));
    let pid = d.dock.panel.panes()[0].pid().unwrap();
    let size = std::fs::read_to_string(d.directory.path().join(format!("size-{pid}"))).unwrap();
    assert_eq!(
        size.lines().collect::<Vec<_>>(),
        [format!("{} {}", t.row_count(), t.column_count())]
    );
    drop(grids);
    d.assert_wraps();
}
#[test]
fn test_initial_default_window() {
    run(|| check(false, false, false));
}
#[test]
fn test_initial_maximized_after_show() {
    run(|| check(false, true, false));
}
#[test]
fn test_startup_before_first_map() {
    run(|| check(false, false, true));
}
#[test]
fn test_right_dock_default() {
    run(|| check(true, false, false));
}
#[test]
fn test_right_dock_maximized() {
    run(|| check(true, true, false));
}
#[test]
fn test_six_panes_both_docks_at_supported_window_widths() {
    run(|| {
        for width in [1200, 1500] {
            for stacked in [false, true] {
                let d = BashDock::new(stacked, 6, false, "C.UTF-8");
                d.dock.window.resize(width, 900);
                d.ready();
                assert_eq!(d.dock.panel.pane_count(), 6);
                assert_eq!(d.dock.window.size().0, width);
                for p in d.dock.panel.panes() {
                    let t = p.terminal.as_ref().unwrap();
                    assert!(t.column_count() >= 8);
                    assert!(t.row_count() >= 2);
                    assert!(p.page.allocated_width() <= p.frame.allocated_width());
                    assert!(p.page.allocated_height() <= p.frame.allocated_height());
                    assert!(!p.content.children()[1].is_visible());
                }
                assert!(!d.dock.panel.new_terminal());
            }
        }
    });
}
#[test]
fn test_later_add_and_split_in_both_docks() {
    run(|| {
        for stacked in [false, true] {
            let d = BashDock::new(stacked, 1, false, "C");
            d.ready();
            for _ in 0..2 {
                d.dock.panel.new_terminal();
                d.ready();
                d.assert_prompt();
            }
            d.assert_wraps();
        }
    });
}
#[test]
fn test_first_terminal_opened_from_hidden_without_autostart() {
    run(|| {
        let d = BashDock::new(false, 0, false, "C");
        assert_eq!(d.dock.panel.pane_count(), 0);
        d.dock.panel.hide_dock();
        d.dock.panel.show_dock();
        d.ready();
        d.assert_prompt();
        let pid = d.dock.panel.panes()[0].pid();
        d.dock.panel.hide_dock();
        d.dock.panel.show_dock();
        d.ready();
        assert_eq!(pid, d.dock.panel.panes()[0].pid());
    });
}
#[test]
fn test_early_keyboard_input_is_preserved() {
    run(|| {
        let d = BashDock::new(false, 1, false, "C.UTF-8");
        let t = d.dock.terminal(0);
        assert!(t.pty().is_none());
        t.emit_by_name::<()>("commit", &[&"a", &1u32]);
        d.dock.ready();
        poll(|| t.cursor_position() == ((PROMPT.len() + 1) as i64, 0));
        assert_eq!(text(&t).trim(), format!("{PROMPT}a"));
    });
}
#[test]
fn test_unmapped_fallback_and_unattached_panel() {
    run(|| {
        let p = impro_terminal::TerminalPanel::new("/tmp", true, false, 11);
        poll(|| p.panes()[0].pid().is_some());
        unsafe {
            p.destroy();
        }
        let d = Dock::new(false, 1);
        d.window.hide();
        poll(|| d.panel.panes()[0].pid().is_some());
        assert!(!d.panel.is_mapped());
    });
}
#[test]
fn test_stalled_initial_layout_still_starts() {
    run(|| {
        let d = BashDock::new(false, 1, false, "C");
        d.dock.panel.set_initial_dock_extent(false, 1000000);
        d.ready();
        d.assert_prompt();
    });
}
#[test]
fn test_close_before_spawn_cancels_startup() {
    run(|| {
        let d = BashDock::new(false, 1, false, "C");
        let p = d.dock.panel.panes()[0].clone();
        assert!(p.pid().is_none());
        d.dock.panel.close_current();
        pump(1100);
        assert!(p.pid().is_none());
        assert!(p.cancellable().is_cancelled());
        assert!(d.grids.borrow().is_empty());
    });
}
