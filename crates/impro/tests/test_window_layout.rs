mod common;
use common::*;
use gtk::prelude::*;

#[test]
fn test_default_layout_and_title() {
    run(|| {
        let h = Harness::new();
        let w = &h.w;
        assert_eq!(w.window.title().as_deref(), Some("impro"));
        assert_eq!(w.titlebar.title_label().text(), "impro");
        assert_eq!(w.window.default_size(), (1200, 750));
        assert_eq!(w.sidebar.position(), 300);
        assert_eq!(w.dock.orientation(), gtk::Orientation::Horizontal);
        assert_eq!(w.dock_bottom.orientation(), gtk::Orientation::Vertical);
        assert_eq!(w.dock.child1(), Some(w.dock_bottom.clone().upcast()));
        assert_eq!(
            w.dock_bottom.child2(),
            Some(w.terminal_bottom.clone().upcast())
        );
        assert!(w.titlebar.explorer_button().is_active());
        assert!(w.titlebar.bottom_dock_button().is_active());
        assert!(w.terminal_bottom.is_visible());
        assert!(!w.terminal_right.is_visible());
        assert_eq!(w.terminal_bottom.terminal_count(), 1);
        assert_eq!(w.terminal_right.pane_count(), 0);
        assert!(!w.terminal_bottom.stacked());
        assert!(w.terminal_right.stacked());
        assert!(w.settings_dialog().is_none());
        assert!(w.graph_views().is_empty());
        assert!(w.gitgraph_views().is_empty());
        assert!(w.command_palette().is_none());
    });
}
#[test]
fn test_both_docks_allocated() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_right();
        poll(|| h.w.layout_ready());
        frame(&h.w.window);
        poll(|| h.w.terminal_right.allocated_width() >= 380);
        frame(&h.w.window);
        let w = &h.w;
        let width = w.terminal_right.allocated_width();
        assert!(
            (width - (w.dock.allocated_width() as f64 * 0.4).round() as i32).abs() <= 1
                || width == 380
        );
        assert!(w.editor.allocated_width() >= 300);
        assert!(
            (w.terminal_bottom.allocated_height()
                - (w.dock_bottom.allocated_height() as f64 * 0.3).round() as i32)
                .abs()
                <= 1
        );
        assert!(w.max_allocation_us() < 4000);
    });
}
#[test]
fn test_hidden_dock_stays_hidden_after_show_all() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_bottom();
        h.w.toggle_terminal_right();
        h.w.window.show_all();
        assert!(!h.w.terminal_bottom.is_visible());
        assert!(h.w.terminal_right.is_visible());
    });
}
#[test]
fn test_last_closed_hides_each_dock_and_show_respawns() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_right();
        for panel in [&h.w.terminal_bottom, &h.w.terminal_right] {
            panel.close_current();
            poll(|| !panel.is_visible());
            assert_eq!(panel.pane_count(), 0);
            h.w.show_dock(panel);
            assert!(panel.is_visible());
            assert_eq!(panel.terminal_count(), 1);
        }
    });
}
#[test]
fn test_header_fullscreen_restores_visibility_positions_and_survives_show_all() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_right();
        poll(|| h.w.layout_ready());
        frame(&h.w.window);
        for panel in [&h.w.terminal_bottom, &h.w.terminal_right] {
            panel.new_terminal();
            frame(&h.w.window);
            let positions = [
                h.w.sidebar.position(),
                h.w.dock.position(),
                h.w.dock_bottom.position(),
            ];
            panel.toggle_maximize();
            h.w.window.show_all();
            assert!(h.w.is_terminal_fullscreen());
            assert!(!h.w.explorer.is_visible());
            if panel == &h.w.terminal_bottom {
                assert!(!h.w.editor.is_visible());
            } else {
                assert!(!h.w.dock_bottom.is_visible());
            }
            panel.restore();
            frame(&h.w.window);
            assert_eq!(
                [
                    h.w.sidebar.position(),
                    h.w.dock.position(),
                    h.w.dock_bottom.position()
                ],
                positions
            );
            assert!(h.w.editor.is_visible());
            assert!(h.w.explorer.is_visible());
        }
    });
}
#[test]
fn test_fullscreen_command_target_and_empty_layout() {
    run(|| {
        let h = Harness::new();
        h.w.hide_dock(&h.w.terminal_bottom);
        h.w.toggle_terminal_fullscreen();
        assert!(h.w.terminal_bottom.is_maximized());
        h.w.restore_terminal_fullscreen();
        h.w.toggle_terminal_right();
        h.w.terminal_right.focus_terminal();
        h.w.toggle_terminal_fullscreen();
        assert!(h.w.terminal_right.is_maximized());
        h.w.restore_terminal_fullscreen();
    });
}
#[test]
fn test_layout_commands_restore_fullscreen_first() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_fullscreen();
        h.w.toggle_explorer();
        assert!(!h.w.is_terminal_fullscreen());
        assert!(!h.w.explorer.is_visible());
        h.w.toggle_terminal_fullscreen();
        h.w.search_in_files();
        assert!(!h.w.is_terminal_fullscreen());
    });
}
#[test]
fn test_last_close_in_fullscreen_restores_and_hides_dock() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_fullscreen();
        h.w.terminal_bottom.close_current();
        poll(|| !h.w.terminal_bottom.is_visible());
        assert!(!h.w.is_terminal_fullscreen());
        assert!(h.w.editor.is_visible());
    });
}
#[test]
fn test_each_dock_limit_and_split_toast() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_right();
        for panel in [&h.w.terminal_bottom, &h.w.terminal_right] {
            panel.set_max_panes(2);
            panel.new_terminal();
            panel.new_terminal();
            assert_eq!(panel.pane_count(), 2);
            assert!(panel.toast().widget().is_visible());
            panel.focus_terminal();
            h.w.split_terminal();
            assert_eq!(panel.pane_count(), 2);
        }
    });
}
#[test]
fn test_right_dock_stacks_and_bottom_dock_is_side_by_side() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_right();
        for panel in [&h.w.terminal_bottom, &h.w.terminal_right] {
            panel.new_terminal();
        }
        frame(&h.w.window);
        for panel in [&h.w.terminal_bottom, &h.w.terminal_right] {
            let p = panel.panes();
            let (ax, ay) = p[0].page.translate_coordinates(panel, 0, 0).unwrap();
            let (bx, by) = p[1].page.translate_coordinates(panel, 0, 0).unwrap();
            if panel.stacked() {
                assert_eq!(ax, bx);
                assert!(ay < by);
            } else {
                assert_eq!(ay, by);
                assert!(ax < bx);
            }
        }
    });
}
#[test]
fn test_collapse_then_fullscreen_restores_positions() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_terminal_right();
        poll(|| h.w.layout_ready());
        frame(&h.w.window);
        h.w.dock_bottom.set_position(0);
        h.w.dock.set_position(0);
        frame(&h.w.window);
        h.w.terminal_right.toggle_maximize();
        h.w.terminal_right.restore();
        frame(&h.w.window);
        assert_eq!(h.w.dock.position(), 0);
        assert_eq!(h.w.dock_bottom.position(), 0);
        h.w.dock.set_position(450);
        h.w.dock_bottom.set_position(300);
        frame(&h.w.window);
        assert!(h.w.editor.is_mapped());
    });
}
#[test]
fn test_explorer_toggle() {
    run(|| {
        let h = Harness::new();
        h.w.toggle_explorer();
        assert!(!h.w.explorer.is_visible());
        h.w.window.show_all();
        assert!(!h.w.explorer.is_visible());
        h.w.toggle_explorer();
        assert!(h.w.explorer.is_visible());
    });
}
#[test]
fn test_folder_change() {
    run(|| {
        let h = Harness::new();
        let other = tempfile::tempdir().unwrap();
        h.w.set_root(other.path());
        poll(|| h.w.root() == other.path());
        assert_eq!(h.w.terminal_bottom.cwd(), other.path());
        assert_eq!(h.w.terminal_right.cwd(), other.path());
        assert_eq!(h.w.window.title().as_deref(), Some("impro"));
    });
}

#[test]
fn test_both_dock_drags_preserve_six_prompts() {
    run(|| {
        let h = Harness::new();
        h.shell(false);
        h.shell(true);
        for panel in [&h.w.terminal_bottom, &h.w.terminal_right] {
            panel.new_terminal();
            panel.new_terminal();
        }
        let panes: Vec<_> =
            h.w.terminal_bottom
                .panes()
                .into_iter()
                .chain(h.w.terminal_right.panes())
                .collect();
        poll(|| panes.iter().all(|p| p.pid().is_some()));
        for pane in &panes {
            stable_prompt(pane);
        }
        for (bottom, right) in [(300, 400), (450, 600), (350, 450), (550, 700), (300, 400)] {
            h.w.dock_bottom.set_position(bottom);
            h.w.dock.set_position(right);
            frame(&h.w.window);
            poll(|| {
                panes
                    .iter()
                    .all(|p| p.gate.as_ref().is_none_or(|g| !g.has_pending_sources()))
            });
        }
        for pane in panes {
            assert_eq!(
                terminal_text(&pane)
                    .lines()
                    .filter(|line| *line == "history-once")
                    .count(),
                1
            );
            assert!(terminal_text(&pane).contains("impro-check> "));
        }
        assert!(h.w.max_allocation_us() < 4000);
    });
}
