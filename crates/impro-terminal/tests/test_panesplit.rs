#![cfg(unix)]
mod common;
use common::*;
use gtk::prelude::*;
use impro_terminal::panesplit::*;
#[test]
fn test_apportion_and_scale() {
    assert_eq!(sizes_from_ratios(&[1., 2., 1.], 800, 160), [200, 400, 200]);
    assert_eq!(sizes_from_ratios(&[1., 2., 1.], 1600, 160), [400, 800, 400]);
    assert_eq!(sizes_from_ratios(&[1.; 3], 1000, 0), [334, 333, 333]);
    assert!(sizes_from_ratios(&[], 1000, 0).is_empty());
}
#[test]
fn test_minima_and_collapsed_layouts() {
    assert_eq!(sizes_from_ratios(&[0., 1., 4.], 800, 160), [160, 160, 480]);
    assert_eq!(sizes_from_ratios(&[1., 10., 100.], 101, 160), [34, 34, 33]);
    assert_eq!(sizes_from_ratios(&[1., 1.], -5, 160), [0, 0]);
    assert_eq!(sizes_from_ratios(&[0., 0.], 321, 160), [161, 160]);
    for extent in 0..600 {
        let s = sizes_from_ratios(&[0.01, 0.3, 0.69], extent, 160);
        assert_eq!(s.iter().sum::<i32>(), extent);
        assert!(*s.iter().min().unwrap() >= 160.min(extent / 3));
    }
}
#[test]
fn test_move_only_pair_and_clamp() {
    for (index, delta, expected) in [
        (0, 50., [250, 250, 400]),
        (0, -10000., [160, 340, 400]),
        (1, 10000., [200, 540, 160]),
    ] {
        let mut s = [200, 300, 400];
        move_boundary(&mut s, index, delta, 160);
        assert_eq!(s, expected);
    }
    let mut s = [20, 30];
    move_boundary(&mut s, 0, 10000., 160);
    assert_eq!(s, [25, 25]);
}
#[test]
fn test_add_resets_and_remove_redistributes_to_neighbours() {
    let mut l = SplitLayout::default();
    for i in 0..3 {
        l.insert(i);
    }
    assert_eq!(l.ratios, [1.; 3]);
    l.ratios = vec![200., 300., 400.];
    l.remove(1);
    assert_eq!(l.ratios, [300., 600.]);
    l.remove(0);
    assert_eq!(l.ratios, [900.]);
    l.insert(0);
    assert_eq!(l.ratios, [1., 1.]);
    l.remove(1);
    l.remove(0);
    assert!(l.ratios.is_empty());
    l.ratios = vec![1., 2., 3., 4.];
    l.remove(1);
    assert_eq!(l.ratios, [1.5, 4.5, 4.]);
}
#[test]
fn test_pointer_keyboard_reset_both_axes_two_and_three_panes() {
    run(|| {
        for stacked in [false, true] {
            for count in [2, 3] {
                let d = Dock::new(stacked, 0);
                for n in 0..count {
                    let kind = if n == 0 {
                        "graph"
                    } else if n == 1 {
                        "gitgraph"
                    } else {
                        "terminal"
                    };
                    if kind == "terminal" {
                        d.panel.new_terminal();
                    } else {
                        d.panel.add_custom_pane(kind, &gtk::Entry::new(), None);
                    }
                }
                pump(60);
                let body = d.panel.body();
                let before = body.pane_sizes();
                assert_eq!(
                    before.iter().sum::<i32>() + HANDLE_SIZE * (count - 1),
                    if stacked {
                        body.allocated_height()
                    } else {
                        body.allocated_width()
                    }
                );
                body.move_handle(0, 40.);
                pump(20);
                let after = body.pane_sizes();
                assert_eq!(after[0], before[0] + 40);
                assert_eq!(after[1], before[1] - 40);
                body.equalize();
                assert_eq!(body.ratios(), vec![1.; count as usize]);
            }
        }
    });
}
#[test]
fn test_hover_and_drag_states_are_instant() {
    run(|| {
        let d = Dock::new(false, 2);
        pump(40);
        let handle = d.panel.body().handles()[0].clone();
        assert!(handle.can_focus());
        assert_eq!(handle.allocated_width(), HANDLE_SIZE);
        assert!(handle.style_context().has_class("impro-pane-handle"));
    });
}
#[test]
fn test_proportional_resize_maximise_restore_and_handle_lifetime() {
    run(|| {
        let d = Dock::new(false, 3);
        pump(80);
        let body = d.panel.body();
        body.move_handle(0, 55.);
        let ratios = body.ratios();
        let old = body.handles()[0].downgrade();
        d.panel.toggle_maximize();
        pump(30);
        assert!(body.handles().iter().all(|h| !h.is_visible()));
        d.panel.restore();
        assert_eq!(body.ratios(), ratios);
        d.panel.close_current();
        pump(30);
        assert!(old.upgrade().is_none());
        assert_eq!(body.handles().len(), 1);
    });
}
#[test]
fn test_graph_live_resize_state_maximise_close_add_and_fade() {
    run(|| {
        for stacked in [false, true] {
            let d = Dock::new(stacked, 1);
            let e = gtk::Entry::new();
            e.set_text("graph state survives resizing");
            d.panel.add_custom_pane("graph", &e, None);
            d.panel
                .add_custom_pane("gitgraph", &gtk::Entry::new(), None);
            pump(60);
            d.panel.body().move_handle(0, 40.);
            d.panel.body().move_handle(1, -40.);
            assert_eq!(e.text(), "graph state survives resizing");
            let before = d.panel.body().ratios();
            d.panel.toggle_maximize();
            d.panel.restore();
            assert_eq!(d.panel.body().ratios(), before);
            assert_eq!(d.panel.graph_views(), [e.clone().upcast::<gtk::Widget>()]);
            d.panel.set_animation_ms(100);
            d.panel.close_kind("graph");
            poll(|| d.panel.closing_panes().is_empty());
            assert_eq!(d.panel.pane_count(), 2);
        }
    });
}
#[test]
fn test_terminal_gitgraph_graph_dividers_and_restore() {
    run(|| {
        for stacked in [false, true] {
            let d = Dock::new(stacked, 1);
            d.panel.add_gitgraph(Some(gtk::Entry::new().upcast_ref()));
            d.panel.add_graph(Some(gtk::Entry::new().upcast_ref()));
            pump(70);
            let body = d.panel.body();
            assert_eq!(body.handles().len(), 2);
            body.move_handle(0, 35.);
            body.move_handle(1, -35.);
            let before = body.ratios();
            d.panel.toggle_maximize_pane(d.panel.panes()[1].id);
            assert!(body.handles().iter().all(|h| !h.is_visible()));
            d.panel.restore();
            assert_eq!(body.ratios(), before);
            assert!(body.handles().iter().all(|h| h.is_visible()));
        }
    });
}
#[test]
fn test_pointer_release_position_double_click_and_keyboard() {
    run(|| {
        for stacked in [false, true] {
            let d = Dock::new(stacked, 2);
            pump(80);
            let body = d.panel.body();
            let handle = body.handles()[0].clone();
            let sizes = body.pane_sizes();
            pointer(&handle, gdk::EventType::ButtonPress, 1000.);
            assert!(handle.state_flags().contains(gtk::StateFlags::ACTIVE));
            pointer(&handle, gdk::EventType::MotionNotify, 1020.);
            assert_eq!(body.pane_sizes()[0], sizes[0] + 20);
            pointer(&handle, gdk::EventType::ButtonRelease, 1040.);
            assert_eq!(body.pane_sizes()[0], sizes[0] + 40);
            assert!(!handle.has_grab());
            assert!(!handle.state_flags().contains(gtk::StateFlags::ACTIVE));
            pointer(&handle, gdk::EventType::DoubleButtonPress, 1040.);
            assert_eq!(body.ratios(), [1., 1.]);
        }
    });
}
#[test]
fn test_two_real_terminals_in_each_dock() {
    run(|| {
        for stacked in [false, true] {
            let d = BashDock::new(stacked, 2, false, "C");
            d.ready();
            let handle = d.dock.panel.body().handles()[0].clone();
            pointer(&handle, gdk::EventType::ButtonPress, 1000.);
            pointer(&handle, gdk::EventType::MotionNotify, 900.);
            pointer(&handle, gdk::EventType::ButtonRelease, 900.);
            pump(180);
            d.assert_prompt();
            pointer(&handle, gdk::EventType::ButtonPress, 1000.);
            pointer(&handle, gdk::EventType::MotionNotify, 1180.);
            pointer(&handle, gdk::EventType::ButtonRelease, 1180.);
            pump(180);
            d.assert_prompt();
            d.assert_wraps();
        }
    });
}
#[test]
fn test_real_bash_boundary_cycles_both_docks() {
    run(|| {
        for stacked in [false, true] {
            let d = BashDock::new(stacked, 3, false, "C");
            d.ready();
            for index in [0, 1] {
                let h = d.dock.panel.body().handles()[index].clone();
                for delay in [4, 16] {
                    for end in [-50., 50.] {
                        let before = d.dock.panel.body().pane_sizes();
                        pointer(&h, gdk::EventType::ButtonPress, 1000.);
                        for step in 1..=20 {
                            pointer(
                                &h,
                                gdk::EventType::MotionNotify,
                                1000. + end * step as f64 / 20.,
                            );
                            pump(delay);
                        }
                        pointer(&h, gdk::EventType::ButtonRelease, 1000. + end);
                        pump(160);
                        let after = d.dock.panel.body().pane_sizes();
                        assert_eq!(before.iter().sum::<i32>(), after.iter().sum::<i32>());
                        for i in 0..3 {
                            if i != index && i != index + 1 {
                                assert_eq!(before[i], after[i]);
                            }
                        }
                        d.assert_prompt();
                    }
                }
            }
            d.assert_wraps();
        }
    });
}
#[test]
fn test_shell_born_after_uneven_layout_uses_final_grid() {
    run(|| {
        let d = BashDock::new(false, 1, false, "C.UTF-8");
        d.ready();
        d.dock.panel.new_terminal();
        pump(10);
        d.dock.panel.body().move_handle(0, 100.);
        d.ready();
        let p = d
            .dock
            .panel
            .panes()
            .into_iter()
            .max_by_key(|p| p.id)
            .unwrap();
        let t = p.terminal.as_ref().unwrap();
        let pid = p.pid().unwrap();
        let size = std::fs::read_to_string(d.directory.path().join(format!("size-{pid}"))).unwrap();
        assert_eq!(
            size.trim(),
            format!("{} {}", t.row_count(), t.column_count())
        );
        d.assert_prompt();
    });
}
#[test]
fn test_real_webkit_resize_keeps_page_state_and_pause_resume() {
    let exe = std::env::current_exe().unwrap();
    let qa = exe
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/webkit_host_qa");
    let result = std::process::Command::new(qa)
        .env("G_DEBUG", "fatal-criticals")
        .output()
        .expect("cargo test builds QA example; build --examples before selecting only this test");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
