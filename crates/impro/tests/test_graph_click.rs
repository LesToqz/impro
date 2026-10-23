//! The Graph view hosted in the right dock of the real window stays mapped,
//! sized and painted through every kind of click a user makes. Pointer events
//! go through GTK's normal dispatch and real frames run between pointer moves,
//! as with a pointer device; the painted check reads the canvas back from the
//! X server.
mod common;
use common::*;
use gtk::prelude::*;
use impro_graph::{view::testing, GraphType, GraphView};
use std::{cell::Cell, rc::Rc};

const NONE: gdk::ModifierType = gdk::ModifierType::empty();
const B1: gdk::ModifierType = gdk::ModifierType::BUTTON1_MASK;

/// A few hundred linked notes, tags, missing targets and source files.
fn project(h: &Harness) {
    for i in 0..240 {
        let mut text = format!("# Note {i}\n");
        for j in [i * 7 + 1, i * 3 + 2, i / 2] {
            text.push_str(&format!("[[n{}]] ", j % 240));
        }
        if i % 12 == 0 {
            text.push_str("#topic [[Missing thing]]\n");
        }
        h.write(&format!("notes/n{i}.md"), &text);
    }
    for i in 0..30 {
        h.write(
            &format!("src/m{i}.rs"),
            &format!("mod m{};\n", (i + 1) % 30),
        );
    }
}

/// The window with the graph open in the right dock, animations on (the
/// user's setting), the first graph arrived.
fn open(h: &Harness) -> GraphView {
    project(h);
    h.w.set_setting("reduce_motion", false.into(), false)
        .unwrap();
    let view = h.w.open_graph().expect("graph pane");
    poll(|| view.summary().nodes > 100);
    settle(&view);
    view
}

fn settle(view: &GraphView) {
    assert!(
        testing::settle(view, 20_000),
        "the graph never came to rest: {:?}, needs frames: {:?}, {:?}",
        testing::info(view),
        testing::frame_reasons(view),
        view.frame_stats()
    );
}

thread_local!(static CLOCK: Cell<u32> = const { Cell::new(10_000) });
/// Event time, advanced by `ms`.
fn tick(ms: u32) -> u32 {
    CLOCK.with(|c| {
        c.set(c.get() + ms);
        c.get()
    })
}

/// Delivers a pointer event to the canvas as the windowing system would.
fn send(
    view: &GraphView,
    kind: gdk::EventType,
    (x, y): (f64, f64),
    button: u32,
    state: gdk::ModifierType,
    time: u32,
) {
    use glib::translate::*;
    let canvas = view.canvas();
    let win = canvas.window().expect("realized canvas");
    let (_, ox, oy) = win.origin();
    let mut event = gdk::Event::new(kind);
    // SAFETY: `event` owns a zeroed GdkEvent of the union member written here.
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        if kind == gdk::EventType::MotionNotify {
            let m = &mut (*raw).motion;
            m.window = win.to_glib_full();
            m.send_event = 1;
            m.time = time;
            (m.x, m.y) = (x, y);
            (m.x_root, m.y_root) = (x + ox as f64, y + oy as f64);
            m.state = state.into_glib();
        } else {
            let b = &mut (*raw).button;
            b.window = win.to_glib_full();
            b.send_event = 1;
            b.time = time;
            (b.x, b.y) = (x, y);
            (b.x_root, b.y_root) = (x + ox as f64, y + oy as f64);
            b.button = button;
            b.state = state.into_glib();
        }
    }
    event.set_device(canvas.display().default_seat().unwrap().pointer().as_ref());
    gtk::main_do_event(&mut event);
}

/// Waits for `n` frames of the canvas frame clock.
fn frames(view: &GraphView, n: u32) {
    let seen = Rc::new(Cell::new(0));
    let count = seen.clone();
    view.canvas().add_tick_callback(move |_, _| {
        count.set(count.get() + 1);
        if count.get() >= n {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    poll(|| seen.get() >= n);
}

fn mask(button: u32) -> gdk::ModifierType {
    match button {
        1 => B1,
        2 => gdk::ModifierType::BUTTON2_MASK,
        _ => gdk::ModifierType::BUTTON3_MASK,
    }
}

/// A press, `path` pointer moves with one frame after each, a hold of
/// `hold` frames and the release, with the times a real device reports.
fn gesture(
    view: &GraphView,
    button: u32,
    state: gdk::ModifierType,
    path: &[(f64, f64)],
    hold: u32,
) {
    let first = path[0];
    send(
        view,
        gdk::EventType::ButtonPress,
        first,
        button,
        state,
        tick(16),
    );
    for p in &path[1..] {
        send(
            view,
            gdk::EventType::MotionNotify,
            *p,
            0,
            state | mask(button),
            tick(16),
        );
        frames(view, 1);
    }
    frames(view, hold.max(1));
    let last = *path.last().unwrap();
    let idle = if hold > 3 { 200 } else { 8 };
    send(
        view,
        gdk::EventType::ButtonRelease,
        last,
        button,
        state | mask(button),
        tick(idle),
    );
}

fn click(view: &GraphView, at: (f64, f64), button: u32, state: gdk::ModifierType) {
    gesture(view, button, state, &[at], 1);
}

/// A click as a hand makes it: a few one-pixel wobbles while the button is
/// down, over about 100 ms.
fn shaky_click(view: &GraphView, at: (f64, f64)) {
    let path: Vec<(f64, f64)> = [
        (0.0, 0.0),
        (1.0, 0.0),
        (1.0, 1.0),
        (2.0, 1.0),
        (1.0, 1.0),
        (1.0, 2.0),
    ]
    .iter()
    .map(|(dx, dy)| (at.0 + dx, at.1 + dy))
    .collect();
    gesture(view, 1, NONE, &path, 6);
}

/// Canvas pixels that differ from the graph background, read back from the
/// X server; the controls panel (open or as its chip) is left out.
fn painted(view: &GraphView) -> usize {
    let canvas = view.canvas();
    let (w, h) = (canvas.allocated_width(), canvas.allocated_height());
    let Some(pix) = canvas.window().and_then(|win| win.pixbuf(0, 0, w, h)) else {
        return 0;
    };
    let panel = &view.panel().root;
    let skip = panel
        .translate_coordinates(canvas, 0, 0)
        .filter(|_| panel.is_mapped())
        .map(|(x, y)| {
            (
                x,
                y,
                x + panel.allocated_width(),
                y + panel.allocated_height(),
            )
        })
        .unwrap_or((0, 0, 0, 0));
    let bg =
        u32::from_str_radix(testing::background_hex(view).trim_start_matches('#'), 16).unwrap();
    let bg = [
        (bg >> 16) as i32,
        ((bg >> 8) & 255) as i32,
        (bg & 255) as i32,
    ];
    let bytes = pix.read_pixel_bytes();
    let (stride, n) = (pix.rowstride() as usize, pix.n_channels() as usize);
    let mut count = 0;
    for y in 0..h {
        for x in 0..w {
            if x >= skip.0 && x < skip.2 && y >= skip.1 && y < skip.3 {
                continue;
            }
            let o = y as usize * stride + x as usize * n;
            let d: i32 = (0..3).map(|c| (bytes[o + c] as i32 - bg[c]).abs()).sum();
            if d > 24 {
                count += 1;
            }
        }
    }
    count
}

/// Waits for the view to come to rest and a window frame, then checks the
/// pane is mapped, sized, has a usable camera and shows the graph.
fn assert_shown(h: &Harness, view: &GraphView, after: &str) {
    settle(view);
    frame(&h.w.window);
    if let Ok(dir) = std::env::var("IMPRO_GRAPH_SHOTS") {
        let window = &h.w.window;
        let (w, hh) = (window.allocated_width(), window.allocated_height());
        if let Some(pix) = window.window().and_then(|win| win.pixbuf(0, 0, w, hh)) {
            let name = after.replace(|c: char| !c.is_ascii_alphanumeric(), "-");
            let _ = pix.savev(format!("{dir}/{name}.png"), "png", &[]);
        }
    }
    let canvas = view.canvas();
    let info = testing::info(view);
    let state = format!(
        "after {after}: mapped={} size={}x{} nodes={} on screen={} zoom={} pan={:?} status={:?}",
        canvas.is_mapped(),
        canvas.allocated_width(),
        canvas.allocated_height(),
        info.nodes,
        testing::nodes_on_screen(view),
        info.zoom,
        info.pan,
        view.status_text(),
    );
    assert!(
        view.widget().is_mapped() && canvas.is_mapped() && canvas.is_drawable(),
        "{state}"
    );
    assert!(view.widget().parent().is_some(), "{state}");
    assert!(
        canvas.allocated_width() >= 200 && canvas.allocated_height() >= 200,
        "{state}"
    );
    assert!(
        info.zoom.is_finite() && info.pan.0.is_finite() && info.pan.1.is_finite(),
        "{state}"
    );
    assert!(info.nodes > 0, "{state}");
    assert!(
        testing::nodes_on_screen(view) > 0,
        "the graph left the view {state}"
    );
    let area = (canvas.allocated_width() * canvas.allocated_height()) as usize;
    let painted = painted(view);
    // markers, labels and connections: a few dozen pixels per shown file
    assert!(
        painted >= info.nodes.min(60) * 40,
        "only {painted} of {area} pixels painted {state}"
    );
    assert!(
        !view.frame_stats().animating,
        "a resting graph keeps a frame callback {state}"
    );
}

fn spot(view: &GraphView, id: &str) -> (f64, f64) {
    let (x, y, _) = testing::node_screen(view, id).unwrap_or_else(|| panic!("no node {id}"));
    (x, y)
}

/// The first node id that ends with `suffix`.
fn node(view: &GraphView, suffix: &str) -> String {
    testing::node_ids(view)
        .into_iter()
        .find(|id| id.ends_with(suffix))
        .unwrap_or_else(|| panic!("no node *{suffix}"))
}

/// A canvas point away from every marker and from the controls panel.
fn empty(view: &GraphView) -> (f64, f64) {
    let (w, h) = testing::info(view).size;
    let ids = testing::node_ids(view);
    let panel = &view.panel().root;
    let (px, py) = panel
        .translate_coordinates(view.canvas(), 0, 0)
        .unwrap_or((0, 0));
    let (pw, ph) = (panel.allocated_width(), panel.allocated_height());
    let mut y = 20.0;
    while y < h - 20.0 {
        let mut x = 20.0;
        while x < w - 20.0 {
            let in_panel = x as i32 >= px - 8
                && x as i32 <= px + pw + 8
                && y as i32 >= py - 8
                && y as i32 <= py + ph + 8;
            if !in_panel
                && ids.iter().all(|id| {
                    let (sx, sy, r) = testing::node_screen(view, id).unwrap();
                    (sx - x).hypot(sy - y) > r + 14.0
                })
            {
                return (x, y);
            }
            x += 12.0;
        }
        y += 12.0;
    }
    panic!("no empty canvas spot");
}

#[test]
fn clicks_on_the_canvas_leave_the_graph_in_view() {
    run(|| {
        let h = Harness::new();
        let view = open(&h);
        assert_shown(&h, &view, "opening");

        // The reported bug: a click on the background, held for a moment with
        // the slight wobble of a hand, threw the whole graph off-screen.
        let at = empty(&view);
        let pan0 = testing::info(&view).pan;
        shaky_click(&view, at);
        assert_shown(&h, &view, "a click on the background");
        let pan = testing::info(&view).pan;
        assert!(
            (pan.0 - pan0.0 - 1.0).abs() < 1e-6 && (pan.1 - pan0.1 - 2.0).abs() < 1e-6,
            "a click on the background moved the view from {pan0:?} to {pan:?}"
        );
        let at = empty(&view);
        click(&view, at, 1, NONE);
        assert_shown(&h, &view, "a still click on the background");

        // a pan drag, a quick flick and a held drag
        let at = empty(&view);
        let path: Vec<_> = (0..=8)
            .map(|i| (at.0 + 6.0 * i as f64, at.1 + 3.0 * i as f64))
            .collect();
        gesture(&view, 1, NONE, &path, 8);
        assert_shown(&h, &view, "a held drag of the background");
        let at = empty(&view);
        let path: Vec<_> = (0..=5).map(|i| (at.0 - 8.0 * i as f64, at.1)).collect();
        gesture(&view, 1, NONE, &path, 0);
        assert_shown(&h, &view, "a flick of the background");
        let at = empty(&view);
        let path: Vec<_> = (0..=6).map(|i| (at.0, at.1 + 5.0 * i as f64)).collect();
        gesture(&view, 2, NONE, &path, 2);
        assert_shown(&h, &view, "a middle-button drag");

        // a file marker: open, another tab, middle click, double click
        let id = node(&view, "notes/n5.md");
        click(&view, spot(&view, &id), 1, NONE);
        wait_path(&h.w, &h.dir.path().join(&id));
        assert_shown(&h, &view, "a click on a file");
        let id = node(&view, "notes/n6.md");
        click(&view, spot(&view, &id), 1, gdk::ModifierType::CONTROL_MASK);
        wait_path(&h.w, &h.dir.path().join(&id));
        assert_shown(&h, &view, "a control click on a file");
        let id = node(&view, "notes/n7.md");
        click(&view, spot(&view, &id), 2, NONE);
        wait_path(&h.w, &h.dir.path().join(&id));
        assert_shown(&h, &view, "a middle click on a file");
        let id = node(&view, "notes/n8.md");
        let at = spot(&view, &id);
        send(&view, gdk::EventType::ButtonPress, at, 1, NONE, tick(16));
        send(&view, gdk::EventType::ButtonRelease, at, 1, B1, tick(60));
        send(&view, gdk::EventType::ButtonPress, at, 1, NONE, tick(60));
        send(
            &view,
            gdk::EventType::DoubleButtonPress,
            at,
            1,
            NONE,
            tick(0),
        );
        send(&view, gdk::EventType::ButtonRelease, at, 1, B1, tick(60));
        wait_path(&h.w, &h.dir.path().join(&id));
        assert_shown(&h, &view, "a double click on a file");
        shaky_click(&view, spot(&view, &node(&view, "notes/n9.md")));
        wait_path(&h.w, &h.dir.path().join("notes/n9.md"));
        assert_shown(&h, &view, "a shaky click on a file");

        // dragging a marker
        let id = node(&view, "notes/n10.md");
        let at = spot(&view, &id);
        let path: Vec<_> = (0..=8)
            .map(|i| (at.0 + 5.0 * i as f64, at.1 - 4.0 * i as f64))
            .collect();
        gesture(&view, 1, NONE, &path, 4);
        assert_shown(&h, &view, "dragging a file marker");

        // wheel and keys
        view.canvas().grab_focus();
        let at = empty(&view);
        for notch in [
            gdk::ScrollDirection::Up,
            gdk::ScrollDirection::Down,
            gdk::ScrollDirection::Down,
        ] {
            let mut event = gdk::Event::new(gdk::EventType::Scroll);
            {
                use glib::translate::*;
                // SAFETY: a scroll event; the window reference is owned by the event.
                unsafe {
                    let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
                    (*raw).scroll.window = view.canvas().window().unwrap().to_glib_full();
                    (*raw).scroll.direction = notch.into_glib();
                    ((*raw).scroll.x, (*raw).scroll.y) = at;
                    (*raw).scroll.time = tick(30);
                    (*raw).scroll.send_event = 1;
                }
            }
            event.set_device(
                view.canvas()
                    .display()
                    .default_seat()
                    .unwrap()
                    .pointer()
                    .as_ref(),
            );
            gtk::main_do_event(&mut event);
            frames(&view, 2);
        }
        assert_shown(&h, &view, "wheel zoom");
        for key in [
            gdk::keys::constants::Left,
            gdk::keys::constants::Up,
            gdk::keys::constants::plus,
            gdk::keys::constants::minus,
        ] {
            testing::key(&view, key, true);
            frames(&view, 3);
            testing::key(&view, key, false);
        }
        assert_shown(&h, &view, "arrow and zoom keys");
        let id = node(&view, "notes/n11.md");
        let at = spot(&view, &id);
        send(&view, gdk::EventType::MotionNotify, at, 0, NONE, tick(16));
        testing::key(&view, gdk::keys::constants::Return, true);
        wait_path(&h.w, &h.dir.path().join(&id));
        assert_shown(&h, &view, "Enter on a hovered file");
    });
}

#[test]
fn menus_markers_panel_and_modes_leave_the_graph_in_view() {
    run(|| {
        let h = Harness::new();
        let view = open(&h);

        // the context menu, every entry
        for label in [
            "Open file",
            "Open another tab",
            "Open in right pane",
            "Locate in project tree",
            "Copy file location",
        ] {
            let id = node(&view, "notes/n3.md");
            send(
                &view,
                gdk::EventType::ButtonPress,
                spot(&view, &id),
                3,
                NONE,
                tick(16),
            );
            let menu = testing::context_menu(&view).expect("context menu");
            let item = menu
                .children()
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::MenuItem>().ok())
                .find(|m| m.label().as_deref() == Some(label))
                .unwrap_or_else(|| panic!("no entry {label}"));
            item.activate();
            menu.popdown();
            send(
                &view,
                gdk::EventType::ButtonRelease,
                spot(&view, &id),
                3,
                gdk::ModifierType::BUTTON3_MASK,
                tick(80),
            );
            assert_shown(&h, &view, label);
        }
        let id = node(&view, "notes/n4.md");
        send(
            &view,
            gdk::EventType::ButtonPress,
            spot(&view, &id),
            3,
            NONE,
            tick(16),
        );
        // dismissed outside the view (a click elsewhere, Escape in the menu)
        testing::context_menu(&view).expect("context menu").cancel();
        assert!(testing::context_menu(&view).is_none());
        assert!(view.panel_open());
        testing::key(&view, gdk::keys::constants::Escape, true);
        assert!(
            !view.panel_open(),
            "Escape after a dismissed menu closes the panel"
        );
        view.set_panel_open(true);
        assert_shown(&h, &view, "dismissing the context menu");

        // tag and missing-target markers
        let tags = view.panel().switch("showTags").unwrap();
        if !tags.is_active() {
            tags.toggle();
        }
        poll(|| {
            testing::node_ids(&view)
                .iter()
                .any(|id| id.contains("topic"))
        });
        settle(&view);
        click(&view, spot(&view, &node(&view, "topic")), 1, NONE);
        assert_shown(&h, &view, "a click on a tag");
        click(&view, spot(&view, &node(&view, "Missing thing")), 1, NONE);
        wait_path(&h.w, &h.dir.path().join("Missing thing.md"));
        assert_shown(&h, &view, "a click on a missing target");

        // the controls panel
        for key in [
            "showAttachments",
            "hideUnresolved",
            "showOrphans",
            "showTags",
        ] {
            view.panel().switch(key).unwrap().toggle();
        }
        for key in ["nodeSizeMultiplier", "linkDistance", "repelStrength"] {
            let scale = view.panel().scale(key).unwrap();
            let a = scale.adjustment();
            scale.set_value((a.lower() + a.upper()) / 2.0);
        }
        for section in [
            "collapse-filter",
            "collapse-display",
            "collapse-forces",
            "collapse-color-groups",
        ] {
            view.panel().click_section(section);
        }
        view.panel().add_rule_button().clicked();
        assert_shown(&h, &view, "panel switches, sliders and sections");
        view.panel().reset_button().clicked();
        assert_shown(&h, &view, "Reset");
        view.panel().close_button().clicked();
        assert!(!view.panel_open());
        assert_shown(&h, &view, "closing the panel");
        view.set_panel_open(true);
        assert_shown(&h, &view, "reopening the panel");

        // an empty result shows the status, then the graph comes back
        view.panel()
            .search_entry()
            .set_text("no-such-file-anywhere");
        poll(|| view.summary().nodes == 0);
        settle(&view);
        assert_eq!(
            view.status_text(),
            ("No files match this view".to_string(), true)
        );
        assert!(view.canvas().is_mapped());
        view.panel().search_entry().set_text("");
        poll(|| view.summary().nodes > 100);
        assert_shown(&h, &view, "clearing the search");

        // local, note and global graphs; the replay
        h.w.editor.open_file(h.dir.path().join("notes/n2.md"));
        wait_path(&h.w, &h.dir.path().join("notes/n2.md"));
        h.w.open_local_graph();
        poll(|| {
            view.graph_type() == GraphType::Local
                && view.summary().nodes > 0
                && view.summary().nodes < 100
        });
        assert_shown(&h, &view, "the local graph");
        let id = testing::node_ids(&view)
            .into_iter()
            .find(|id| id.ends_with(".md") && !id.ends_with("n2.md"))
            .unwrap();
        click(&view, spot(&view, &id), 1, NONE);
        wait_path(&h.w, &h.dir.path().join(&id));
        poll(|| view.summary().nodes > 0 && testing::node_ids(&view).contains(&id));
        assert_shown(&h, &view, "following a file in the local graph");
        view.set_graph_type(GraphType::Note);
        poll(|| view.summary().nodes == 0);
        assert!(view.status_text().1 && view.canvas().is_mapped());
        h.w.open_graph();
        poll(|| view.graph_type() == GraphType::Global && view.summary().nodes > 100);
        assert_shown(&h, &view, "back to the global graph");
        view.replay_history();
        assert_shown(&h, &view, "the history replay");
    });
}

#[test]
fn the_dock_tab_buttons_theme_and_reopening_leave_the_graph_in_view() {
    run(|| {
        let h = Harness::new();
        let view = open(&h);
        let pane = h.w.terminal_right.panes()[0].clone();

        pane.header.label_button.clicked();
        assert_shown(&h, &view, "a click on the dock tab");
        pane.header.fullscreen_button.button.clicked();
        assert!(h.w.terminal_right.is_maximized());
        assert_shown(&h, &view, "maximizing the pane");
        shaky_click(&view, empty(&view));
        assert_shown(&h, &view, "a click on the maximized pane");
        let id = node(&view, "notes/n5.md");
        click(&view, spot(&view, &id), 1, NONE);
        wait_path(&h.w, &h.dir.path().join(&id));
        assert!(!h.w.terminal_right.is_maximized());
        assert_shown(&h, &view, "opening a file from the maximized pane");
        h.w.dock.set_position(h.w.dock.position() - 300);
        frame(&h.w.window);
        assert_shown(&h, &view, "widening the dock");
        h.w.terminal_right.add_button().widget().clicked();
        h.w.terminal_right.add_button().choose("graph");
        assert_eq!(h.w.terminal_right.graph_count(), 1);
        assert_shown(&h, &view, "the dock + button");

        h.w.set_theme("impro-light", false);
        assert_shown(&h, &view, "a light theme");
        h.w.set_theme("impro-dark", false);
        h.w.set_setting("reduce_motion", true.into(), false)
            .unwrap();
        shaky_click(&view, empty(&view));
        assert_shown(&h, &view, "a click with reduced motion");
        h.w.set_setting("reduce_motion", false.into(), false)
            .unwrap();

        // hide and show the dock, close and reopen the pane
        h.w.toggle_right_dock();
        assert!(!view.canvas().is_mapped());
        assert!(
            !view.frame_stats().animating,
            "a hidden graph needs no frames"
        );
        h.w.toggle_right_dock();
        assert_shown(&h, &view, "showing the dock again");
        for _ in 0..3 {
            pane_delete(&h);
            let reopened = h.w.open_graph().expect("graph pane");
            poll(|| reopened.summary().nodes > 100);
            assert_shown(&h, &reopened, "reopening the pane");
            shaky_click(&reopened, empty(&reopened));
            assert_shown(&h, &reopened, "a click after reopening");
        }
    });
}

fn pane_delete(h: &Harness) {
    let pane = h.w.terminal_right.panes()[0].clone();
    pane.header.delete_button.button.clicked();
    poll(|| h.w.graph_views().is_empty());
    drain();
}
