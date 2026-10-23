//! Pointer and keyboard behaviour of the canvas: hit testing, actions, hover,
//! dragging, camera, context menu.

mod gtk_common;

use gdk::keys::constants as keys;
use gtk::prelude::*;
use gtk_common::*;
use impro_graph::view::menu::{activatable_labels, CONTEXT_ITEMS};
use impro_graph::view::testing as t;
use impro_graph::OpenMode;
use serde_json::json;

fn tags_on() -> serde_json::Value {
    json!({"showTags": true})
}

#[test]
fn clicking_a_file_opens_it_and_ctrl_or_middle_click_uses_another_tab() {
    run(|| {
        let f = Fixture::new();
        let (x, y, _) = f.screen("a.md");
        t::click(&f.view, x, y, 1, none());
        t::click(&f.view, x, y, 1, gdk::ModifierType::CONTROL_MASK);
        t::click(&f.view, x, y, 2, none());
        let seen = f.seen();
        assert_eq!(
            seen.opened,
            [
                (f.path("a.md"), OpenMode::Current),
                (f.path("a.md"), OpenMode::NewTab),
                (f.path("a.md"), OpenMode::NewTab)
            ]
        );
    });
}

#[test]
fn tags_search_and_missing_targets_are_created() {
    run(|| {
        let f = Fixture::with(&sample_files(), tags_on());
        let (x, y, _) = f.screen("#topic");
        t::click(&f.view, x, y, 1, none());
        assert_eq!(f.seen().tags, ["topic"]);
        assert_eq!(impro_graph::view::tag_query("topic"), "tag:#topic");
        let (x, y, _) = f.screen("?Missing Note");
        t::click(&f.view, x, y, 1, none());
        assert_eq!(f.seen().created, ["Missing Note"]);
        assert!(f.seen().opened.is_empty());
    });
}

#[test]
fn a_click_allows_six_pixels_of_movement_and_more_becomes_a_drag() {
    run(|| {
        let f = Fixture::new();
        let (x, y, _) = f.screen("c.md");
        t::press(&f.view, x, y, 1, none());
        t::motion(&f.view, x + 4.0, y + 3.0); // 5 px
        t::release(&f.view, x + 4.0, y + 3.0, 1, none());
        assert_eq!(f.seen().opened.len(), 1, "5 px is still a click");
        let before = t::node_world(&f.view, "c.md").expect("world");
        t::press(&f.view, x, y, 1, none());
        t::motion(&f.view, x + 5.0, y + 5.0); // 7.07 px
        t::motion(&f.view, x + 60.0, y + 40.0);
        assert!(t::info(&f.view).dragging);
        t::release(&f.view, x + 60.0, y + 40.0, 1, none());
        assert_eq!(f.seen().opened.len(), 1, "a drag does not open the file");
        assert!(!t::info(&f.view).dragging);
        // the node followed the pointer (world units = pixels / zoom)
        t::pump(100);
        let after = t::node_world(&f.view, "c.md").expect("world");
        let zoom = t::info(&f.view).zoom as f32;
        assert!(
            ((after.0 - before.0) * zoom - 60.0).abs() < 25.0,
            "{before:?} -> {after:?}"
        );
        assert!(
            ((after.1 - before.1) * zoom - 40.0).abs() < 25.0,
            "{before:?} -> {after:?}"
        );
        f.settle();
    });
}

#[test]
fn hover_dims_unrelated_files_to_28_percent_and_leaving_restores_them() {
    run(|| {
        let f = Fixture::new();
        let (x, y, _) = f.screen("c.md");
        t::motion(&f.view, x, y);
        assert_eq!(t::info(&f.view).hover.as_deref(), Some("c.md"));
        f.settle();
        // c is referenced by hub, b (and itself is a leaf): those stay, others dim
        let alpha = |id: &str| t::node_look(&f.view, id).expect("node").0;
        assert!((alpha("c.md") - 1.0).abs() < 1e-2);
        assert!((alpha("b.md") - 1.0).abs() < 1e-2);
        assert!(
            (alpha("orphan.md") - 0.28).abs() < 1e-2,
            "{}",
            alpha("orphan.md")
        );
        assert!((alpha("src/main.py") - 0.28).abs() < 1e-2);
        // the hovered marker takes the accent (highlight) colour
        let accent = t::accent_hex(&f.view);
        assert_eq!(t::node_look(&f.view, "c.md").expect("node").1, accent);
        t::leave(&f.view);
        f.settle();
        assert!((alpha("orphan.md") - 1.0).abs() < 1e-2);
    });
}

#[test]
fn dragging_the_background_pans_and_a_fling_decays_to_rest() {
    run(|| {
        let f = Fixture::new();
        // an empty spot far from every marker
        let (x, y) = (5.0, 5.0);
        assert!(f.ids().iter().all(|id| {
            let (sx, sy, r) = f.screen(id);
            (sx - x).hypot(sy - y) > r + 2.0
        }));
        let pan0 = t::info(&f.view).pan;
        t::press(&f.view, x, y, 1, none());
        for i in 1..=8 {
            t::motion(&f.view, x + 10.0 * i as f64, y + 4.0 * i as f64);
        }
        let panned = t::info(&f.view).pan;
        assert!((panned.0 - pan0.0 - 80.0).abs() < 1e-6 && (panned.1 - pan0.1 - 32.0).abs() < 1e-6);
        t::release(&f.view, x + 80.0, y + 32.0, 1, none());
        // inertia continues, then stops by itself
        assert!(t::wait_for(2_000, || t::info(&f.view).pan.0 > panned.0 + 1.0));
        assert!(t::settle(&f.view, 15_000), "inertia must end");
        let rest = t::info(&f.view).pan;
        t::pump(200);
        assert_eq!(t::info(&f.view).pan, rest);
        assert!(
            !f.view.frame_stats().animating,
            "a resting camera needs no frames"
        );
    });
}

#[test]
fn the_wheel_zooms_about_the_pointer_and_clamps() {
    run(|| {
        let f = Fixture::new();
        let (px, py) = (200.0, 150.0);
        let before = t::info(&f.view);
        let world = (
            (px - before.pan.0) / before.zoom,
            (py - before.pan.1) / before.zoom,
        );
        t::wheel(&f.view, px, py, -480.0, false);
        assert!(t::settle(&f.view, 10_000));
        let after = t::info(&f.view);
        assert!(
            (after.zoom / before.zoom - 1.0f64.exp()).abs() < 1e-6,
            "exp(480/480)"
        );
        let on_screen = (
            world.0 * after.zoom + after.pan.0,
            world.1 * after.zoom + after.pan.1,
        );
        assert!((on_screen.0 - px).abs() < 1e-6 && (on_screen.1 - py).abs() < 1e-6);
        // zooming out returns to the earlier scale about the view centre
        t::wheel(&f.view, px, py, 480.0, false);
        assert!(t::settle(&f.view, 10_000));
        let back = t::info(&f.view);
        assert!((back.zoom - before.zoom).abs() < 1e-6);
        // limits
        t::wheel(&f.view, px, py, -1.0e6, false);
        assert!(t::settle(&f.view, 10_000));
        assert_eq!(t::info(&f.view).zoom, 6.0);
        t::wheel(&f.view, px, py, 1.0e6, false);
        assert!(t::settle(&f.view, 10_000));
        assert_eq!(t::info(&f.view).zoom, 0.04);
        // pinch (ctrl + small wheel) doubles the delta
        let z0 = t::info(&f.view).target_zoom;
        t::wheel(&f.view, px, py, -8.0, true);
        let z1 = t::info(&f.view).target_zoom;
        assert!((z1 / z0 - (16.0f64 / 480.0).exp()).abs() < 1e-9);
        // zoom is saved after it stops changing
        assert!(t::wait_for(3_000, || (f.view.prefs().scale - z1).abs() < 1e-9));
    });
}

#[test]
fn wheel_out_preserves_the_centre_and_wheel_in_preserves_the_pointer() {
    run(|| {
        let f = Fixture::new();
        for ctrl in [false, true] {
            for pixels in [16.0, -16.0] {
                let before = t::info(&f.view);
                let pointer = (180.0, 120.0);
                let anchor = if pixels > 0.0 {
                    (before.size.0 / 2.0, before.size.1 / 2.0)
                } else {
                    pointer
                };
                let world = (
                    (anchor.0 - before.pan.0) / before.zoom,
                    (anchor.1 - before.pan.1) / before.zoom,
                );
                t::wheel(&f.view, pointer.0, pointer.1, pixels, ctrl);
                assert!(t::settle(&f.view, 3_000));
                let after = t::info(&f.view);
                assert!((world.0 * after.zoom + after.pan.0 - anchor.0).abs() < 1e-6);
                assert!((world.1 * after.zoom + after.pan.1 - anchor.1).abs() < 1e-6);
            }
        }
    });
}

#[test]
fn keys_pan_zoom_activate_and_dismiss() {
    run(|| {
        let f = Fixture::new();
        let pan0 = t::info(&f.view).pan;
        t::key(&f.view, keys::Left, true);
        t::pump(120);
        t::key(&f.view, keys::Left, false);
        assert!(t::info(&f.view).pan.0 > pan0.0, "left arrow moves the view");
        let z0 = t::info(&f.view).target_zoom;
        t::key(&f.view, keys::equal, true);
        t::pump(120);
        t::key(&f.view, keys::equal, false);
        assert!(t::info(&f.view).target_zoom > z0);
        t::key(&f.view, keys::minus, true);
        t::pump(300);
        t::key(&f.view, keys::minus, false);
        assert!(t::settle(&f.view, 10_000));
        // Enter opens the hovered file
        let (x, y, _) = f.screen("a.md");
        t::motion(&f.view, x, y);
        t::key(&f.view, keys::Return, true);
        assert_eq!(
            f.seen().opened.last().map(|o| o.0.clone()),
            Some(f.path("a.md"))
        );
        // Escape closes the panel, then clears the hover
        assert!(f.view.panel_open());
        t::key(&f.view, keys::Escape, true);
        assert!(!f.view.panel_open());
        t::key(&f.view, keys::Escape, true);
        assert_eq!(t::info(&f.view).hover, None);
    });
}

#[test]
fn the_context_menu_has_exactly_the_five_documented_entries_and_acts() {
    run(|| {
        let f = Fixture::new();
        assert!(t::open_context_menu(&f.view, "a.md"));
        let menu = t::context_menu(&f.view).expect("menu");
        let labels = activatable_labels(&menu);
        let want: Vec<&str> = CONTEXT_ITEMS.iter().map(|i| i.0).collect();
        assert_eq!(labels, want);
        assert_eq!(
            labels,
            [
                "Open file",
                "Open another tab",
                "Open in right pane",
                "Locate in project tree",
                "Copy file location"
            ]
        );
        // the title shows the file's name and a separator splits the groups
        let children = menu.children();
        assert_eq!(children.len(), 7, "title + 3 + separator + 2");
        assert!(children[4].is::<gtk::SeparatorMenuItem>());
        let title = children[0]
            .clone()
            .downcast::<gtk::MenuItem>()
            .expect("item");
        assert_eq!(title.label().as_deref(), Some("a"));
        assert!(!title.is_sensitive());
        let activate = |index: usize| {
            t::open_context_menu(&f.view, "a.md");
            let menu = t::context_menu(&f.view).expect("menu");
            let item = menu
                .children()
                .into_iter()
                .filter_map(|c| c.downcast::<gtk::MenuItem>().ok())
                .filter(|i| {
                    i.is_sensitive() && i.downcast_ref::<gtk::SeparatorMenuItem>().is_none()
                })
                .nth(index)
                .expect("item");
            item.activate();
        };
        activate(0);
        activate(1);
        activate(2);
        activate(3);
        activate(4);
        let seen = f.seen();
        assert_eq!(
            seen.opened,
            [
                (f.path("a.md"), OpenMode::Current),
                (f.path("a.md"), OpenMode::NewTab),
                (f.path("a.md"), OpenMode::RightPane)
            ]
        );
        assert_eq!(seen.revealed, [f.path("a.md")]);
        assert_eq!(seen.copied, [f.path("a.md")]);
    });
}

#[test]
fn right_click_follows_the_kind_of_marker() {
    run(|| {
        let f = Fixture::with(&sample_files(), tags_on());
        // tags search, missing targets have no menu
        let (x, y, _) = f.screen("#topic");
        t::click(&f.view, x, y, 3, none());
        assert_eq!(f.seen().tags, ["topic"]);
        assert!(t::context_menu(&f.view).is_none());
        let (x, y, _) = f.screen("?Missing Note");
        t::click(&f.view, x, y, 3, none());
        assert!(t::context_menu(&f.view).is_none());
        assert!(f.seen().created.is_empty());
        // empty space has no menu either
        t::click(&f.view, 2.0, 2.0, 3, none());
        assert!(t::context_menu(&f.view).is_none());
    });
}

#[test]
fn the_pointer_cannot_re_enter_the_view_while_a_signal_handler_runs() {
    run(|| {
        // hover crash class: handlers may call back into the view
        let f = Fixture::new();
        let view = f.view.clone();
        f.view.connect_open_file(move |_, _| {
            view.rebuild();
            view.set_active_file(None);
            let _ = view.prefs();
            view.reset_camera();
        });
        let (x, y, _) = f.screen("a.md");
        t::click(&f.view, x, y, 1, none());
        t::pump(300);
        assert!(f.view.summary().nodes > 0);
    });
}

/// Pans from `from` by `step` per pointer move, letting one real frame run
/// between moves as a pointer device does; returns the last event time.
fn pan_with_frames(f: &Fixture, from: (f64, f64), step: (f64, f64), moves: u32) -> u32 {
    let mut time = 1_000;
    t::press_at(&f.view, from.0, from.1, 1, none(), time);
    for i in 1..=moves {
        time += 16;
        let (x, y) = (from.0 + step.0 * i as f64, from.1 + step.1 * i as f64);
        t::motion_at(&f.view, x, y, time);
        assert!(
            t::wait_frames(&f.view, 1, 5_000),
            "the canvas stopped drawing"
        );
    }
    time
}

#[test]
fn a_held_pan_stays_under_the_pointer_between_moves_and_after_release() {
    run(|| {
        // Regression: inertia ran while the button was still down, so every
        // frame between pointer moves pushed the view by the filtered move
        // (pixels per event) times the frame length, thousands of pixels in
        // total; a release after holding still left the graph off-screen.
        let f = Fixture::new();
        let from = empty_spot(&f, 4.0);
        let shown = t::nodes_on_screen(&f.view);
        assert!(shown > 0);
        let pan0 = t::info(&f.view).pan;
        let time = pan_with_frames(&f, from, (12.0, 5.0), 8);
        let expected = (pan0.0 + 96.0, pan0.1 + 40.0);
        let pan = t::info(&f.view).pan;
        assert!(
            (pan.0 - expected.0).abs() < 1e-6 && (pan.1 - expected.1).abs() < 1e-6,
            "the view drifted while the pointer moved: {pan:?}, expected {expected:?}"
        );
        // holding still: nothing moves
        assert!(t::wait_frames(&f.view, 10, 5_000));
        let held = t::info(&f.view).pan;
        assert!(
            (held.0 - expected.0).abs() < 1e-6 && (held.1 - expected.1).abs() < 1e-6,
            "the view drifted while the pointer held still: {held:?}, expected {expected:?}"
        );
        // a release after holding still does not fling
        t::release_at(&f.view, from.0 + 96.0, from.1 + 40.0, 1, none(), time + 400);
        assert!(t::settle(&f.view, 15_000));
        let rest = t::info(&f.view).pan;
        assert!((rest.0 - expected.0).abs() < 1e-6 && (rest.1 - expected.1).abs() < 1e-6);
        assert_eq!(t::nodes_on_screen(&f.view), shown, "{rest:?}");
    });
}

#[test]
fn a_quick_drag_and_release_flings_a_short_way_from_the_pointer() {
    run(|| {
        let f = Fixture::new();
        let from = empty_spot(&f, 4.0);
        let pan0 = t::info(&f.view).pan;
        let time = pan_with_frames(&f, from, (10.0, 0.0), 6);
        let moved = t::info(&f.view).pan;
        assert!(
            (moved.0 - pan0.0 - 60.0).abs() < 1e-6,
            "the view drifted during the drag: {moved:?} from {pan0:?}"
        );
        t::release_at(&f.view, from.0 + 60.0, from.1, 1, none(), time + 8);
        // the fling continues in the drag direction and stays modest:
        // 10 px per 16 ms with 86% kept per frame is about 75 px
        assert!(t::wait_for(2_000, || t::info(&f.view).pan.0 > moved.0 + 1.0));
        assert!(t::settle(&f.view, 15_000), "inertia must end");
        let rest = t::info(&f.view).pan;
        assert!(
            rest.0 - moved.0 > 1.0 && rest.0 - moved.0 < 200.0,
            "fling of {} px",
            rest.0 - moved.0
        );
        assert!((rest.1 - moved.1).abs() < 1e-6);
        assert!(t::nodes_on_screen(&f.view) > 0);
    });
}

#[test]
fn a_press_whose_release_never_arrives_does_not_keep_panning() {
    run(|| {
        // A release can be lost (a grab taken mid-press, the window losing the
        // pointer). Motion without the button held then ends the press instead
        // of dragging the view along with the bare pointer.
        let f = Fixture::new();
        let from = empty_spot(&f, 4.0);
        let pan0 = t::info(&f.view).pan;
        let held = gdk::ModifierType::BUTTON1_MASK;
        send_pointer(&f.view, gdk::EventType::ButtonPress, from, 1, none(), 2_000);
        send_pointer(
            &f.view,
            gdk::EventType::MotionNotify,
            (from.0 + 20.0, from.1),
            0,
            held,
            2_016,
        );
        let pan = t::info(&f.view).pan;
        assert!((pan.0 - pan0.0 - 20.0).abs() < 1e-6, "{pan:?}");
        // the release went elsewhere; the pointer now moves with no button down
        send_pointer(
            &f.view,
            gdk::EventType::MotionNotify,
            (from.0 + 220.0, from.1 + 90.0),
            0,
            none(),
            2_500,
        );
        assert!(t::settle(&f.view, 15_000));
        let after = t::info(&f.view).pan;
        assert!(
            (after.0 - pan.0).abs() < 1e-6 && (after.1 - pan.1).abs() < 1e-6,
            "the bare pointer dragged the view: {after:?}"
        );
        // and the next click works normally
        let (x, y, _) = f.screen("a.md");
        send_pointer(
            &f.view,
            gdk::EventType::ButtonPress,
            (x, y),
            1,
            none(),
            3_000,
        );
        send_pointer(
            &f.view,
            gdk::EventType::ButtonRelease,
            (x, y),
            1,
            held,
            3_050,
        );
        assert_eq!(f.seen().opened, [(f.path("a.md"), OpenMode::Current)]);
    });
}

#[test]
fn a_double_click_opens_once_per_click_and_leaves_no_press_behind() {
    run(|| {
        let f = Fixture::new();
        let (x, y, _) = f.screen("a.md");
        let held = gdk::ModifierType::BUTTON1_MASK;
        let mut time = 4_000;
        for kind in [
            gdk::EventType::ButtonPress,
            gdk::EventType::ButtonRelease,
            gdk::EventType::ButtonPress,
            gdk::EventType::DoubleButtonPress,
            gdk::EventType::ButtonRelease,
        ] {
            let state = if kind == gdk::EventType::ButtonRelease {
                held
            } else {
                none()
            };
            send_pointer(&f.view, kind, (x, y), 1, state, time);
            time += 40;
        }
        assert_eq!(f.seen().opened.len(), 2);
        // nothing is still pressed: moving the pointer only hovers
        let pan = t::info(&f.view).pan;
        send_pointer(
            &f.view,
            gdk::EventType::MotionNotify,
            (x + 150.0, y + 60.0),
            0,
            none(),
            time,
        );
        assert!(t::settle(&f.view, 15_000));
        assert_eq!(t::info(&f.view).pan, pan);
        assert!(!t::info(&f.view).dragging);
    });
}

#[test]
fn a_right_click_finishes_its_highlight_fade_and_a_dismissed_menu_is_forgotten() {
    run(|| {
        // Regression: the right click highlighted the file without asking for
        // frames, so the fade froze part-way until the next input; a menu
        // dismissed outside the view stayed recorded as open.
        let f = Fixture::new();
        assert!(t::open_context_menu(&f.view, "a.md"));
        assert!(
            !t::needs_frames(&f.view) || f.view.frame_stats().animating,
            "the highlight fade has no frames: {:?}",
            t::frame_reasons(&f.view)
        );
        t::context_menu(&f.view).expect("menu").cancel();
        assert!(
            t::context_menu(&f.view).is_none(),
            "a dismissed menu is forgotten"
        );
        assert!(
            t::settle(&f.view, 5_000),
            "the fade stalled: {:?}",
            t::frame_reasons(&f.view)
        );
        // Escape now goes to the panel, not to the forgotten menu
        assert!(f.view.panel_open());
        t::key(&f.view, keys::Escape, true);
        assert!(!f.view.panel_open());
    });
}
