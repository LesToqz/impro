//! The controls strip and panel hug the top-right corner of the pane, in every
//! state and at every size, with their icons on one axis.
//!
//! Reference geometry (the original's `.gc` rules, measured from its CSS and
//! page): the controls sit 12 px from the top and right edges; the closed strip
//! is a column of 26 px buttons in a 1 px border and 6 px padding with an 8 px
//! gap, so the icons' centre line is 32 px from the right edge and their pitch
//! 34 px. The open panel is 240 px wide; its header buttons (26 px, 4 px gap)
//! end 8 px inside the frame's 1 px border and sit 6 px below it.

mod gtk_common;

use gtk::prelude::*;
use serde_json::json;

use gtk_common::*;
use impro_graph::view::testing::{self as t, Rect};

const MARGIN: f64 = 12.0;
const ICON_AXIS: f64 = 32.0;
const PITCH: f64 = 34.0;

fn empty_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("dir");
    write(dir.path(), "only.md", "x");
    dir
}

fn near(a: f64, b: f64, what: &str, state: &str) {
    assert!((a - b).abs() <= 0.5, "{state}: {what}: {a} != {b}");
}

fn check_strip(f: &Fixture, state: &str) {
    t::pump(400);
    let g = t::controls_geometry(&f.view);
    let pane = g.pane.expect("pane");
    let chip = g
        .chip
        .unwrap_or_else(|| panic!("{state}: the strip is not shown"));
    assert_eq!(g.chip_buttons.len(), 3, "{state}");
    near(
        pane.right() - chip.right(),
        MARGIN,
        &format!("strip right margin ({g:?})"),
        state,
    );
    near(chip.y - pane.y, MARGIN, "strip top margin", state);
    assert!(g.panel.is_none(), "{state}: panel and strip both shown");
    for (k, b) in g.chip_buttons.iter().enumerate() {
        let (cx, cy) = b.centre();
        near(
            pane.right() - cx,
            ICON_AXIS,
            &format!("icon {k} axis"),
            state,
        );
        near(
            cy - pane.y,
            ICON_AXIS + PITCH * k as f64,
            &format!("icon {k} row"),
            state,
        );
    }
}

fn check_panel(f: &Fixture, state: &str) {
    t::pump(400);
    let g = t::controls_geometry(&f.view);
    let pane = g.pane.expect("pane");
    let panel = g.panel.unwrap_or_else(|| panic!("{state}: no panel"));
    near(
        pane.right() - panel.right(),
        MARGIN,
        "panel right margin",
        state,
    );
    near(panel.y - pane.y, MARGIN, "panel top margin", state);
    near(panel.w, 240.0, "panel width", state);
    let close = g.head_buttons[0];
    near(
        panel.right() - close.right(),
        9.0,
        "close button inset",
        state,
    );
    near(close.y - panel.y, 7.0, "close button top", state);
    let reset = g.head_buttons[1];
    near(close.x - reset.right(), 4.0, "head button gap", state);
}

/// Ink bounding boxes of the icons, from the painted view.
fn ink_boxes(f: &Fixture, buttons: &[Rect]) -> Vec<Option<(f64, f64, f64, f64)>> {
    let (w, h, stride, data) = t::snapshot(&f.view).expect("snapshot");
    let scale = f.view.scale_factor() as f64;
    buttons
        .iter()
        .map(|b| {
            let (x0, y0) = ((b.x * scale) as i32, (b.y * scale) as i32);
            let (x1, y1) = (((b.x + b.w) * scale) as i32, ((b.y + b.h) * scale) as i32);
            let px = |x: i32, y: i32| -> [i32; 3] {
                let i = (y * stride + x * 4) as usize;
                [data[i] as i32, data[i + 1] as i32, data[i + 2] as i32]
            };
            let back = px(x0 + 1, y0 + 1);
            let mut bb: Option<(i32, i32, i32, i32)> = None;
            for y in y0.max(0)..y1.min(h) {
                for x in x0.max(0)..x1.min(w) {
                    let c = px(x, y);
                    let d = (0..3).map(|k| (c[k] - back[k]).abs()).sum::<i32>();
                    if d > 60 {
                        let e = bb.get_or_insert((x, y, x, y));
                        *e = (e.0.min(x), e.1.min(y), e.2.max(x), e.3.max(y));
                    }
                }
            }
            bb.map(|(a, b, c, d)| {
                (
                    a as f64 / scale,
                    b as f64 / scale,
                    (c + 1) as f64 / scale,
                    (d + 1) as f64 / scale,
                )
            })
        })
        .collect()
}

fn check_ink(f: &Fixture, state: &str) {
    let g = t::controls_geometry(&f.view);
    let boxes = ink_boxes(f, &g.chip_buttons);
    let mut centres = Vec::new();
    for (k, b) in boxes.iter().enumerate() {
        let (x0, y0, x1, y1) = b.unwrap_or_else(|| panic!("{state}: icon {k} paints nothing"));
        centres.push(((x0 + x1) / 2.0, (y0 + y1) / 2.0, x1 - x0, y1 - y0));
    }
    let first = centres[0];
    for (k, c) in centres.iter().enumerate() {
        assert!(
            (c.0 - first.0).abs() <= 0.75,
            "{state}: icon {k} ink centre x {} vs {} ({centres:?})",
            c.0,
            first.0
        );
        let b = g.chip_buttons[k].centre();
        assert!(
            (c.0 - b.0).abs() <= 0.75 && (c.1 - b.1).abs() <= 0.75,
            "{state}: icon {k} ink centre {:?} off its button centre {b:?}",
            (c.0, c.1)
        );
        assert!(
            (c.2 - first.2).abs() <= 1.5 && (c.3 - first.3).abs() <= 1.5,
            "{state}: icon {k} ink size differs ({centres:?})"
        );
    }
}

fn sized(prefs: serde_json::Value, size: (i32, i32)) -> Fixture {
    Fixture::in_dir_sized(empty_project(), prefs, true, size)
}

#[test]
fn the_closed_strip_hugs_the_corner_with_icons_on_one_axis() {
    run(|| {
        for size in [(904, 1240), (1920, 1080), (640, 480)] {
            let f = sized(json!({"close": true}), size);
            check_strip(&f, &format!("saved close {size:?}"));
            check_ink(&f, &format!("saved close {size:?}"));
            // the host shows everything again when it re-adds the page
            f.window.show_all();
            check_strip(&f, &format!("after show_all {size:?}"));
            f.window.resize(size.0 / 2 + 100, size.1 / 2 + 100);
            check_strip(&f, &format!("resized {size:?}"));
        }
    });
}

#[test]
fn closing_the_open_panel_leaves_the_strip_in_the_corner() {
    run(|| {
        let f = sized(json!({}), (904, 700));
        check_panel(&f, "open");
        f.view.panel().close_button().clicked();
        check_strip(&f, "closed by its button");
        check_ink(&f, "closed by its button");
        f.window.show_all();
        check_strip(&f, "closed, shown again");
        f.view.panel().set_open(true);
        check_panel(&f, "reopened");
        f.window.resize(1100, 800);
        check_panel(&f, "reopened and resized");
        f.view.panel().set_open(false);
        check_strip(&f, "closed again");
    });
}
