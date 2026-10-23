//! Connections and markers must come from one camera in every frame.

mod gtk_common;

use std::cell::RefCell;
use std::rc::Rc;

use gtk_common::drive::*;
use gtk_common::*;
use impro_graph::view::testing as t;

fn logical(w: i32, h: i32) -> (i32, i32) {
    let scale: i32 = std::env::var("GDK_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    (w / scale, h / scale)
}

#[test]
fn probe_edges_during_pan_after_zoom_out() {
    run(|| {
        let (w, h) = logical(1920, 1080);
        let f = Fixture::big((w, h));
        t::capture_frames(&f.view, true);
        let checks = Rc::new(RefCell::new(Vec::new()));
        let sink = checks.clone();
        t::set_probe(&f.view, move |shot| {
            let t0 = std::time::Instant::now();
            if let Some(c) = shot.compare_fresh() {
                sink.borrow_mut().push((shot.info().pan, c));
            }
            let _ = t0;
        });
        let centre = (w as f64 / 2.0, h as f64 / 2.0);
        wheel_frames(&f.view, centre, 120.0, 40);
        let mut p = Pointer::new(&f.view, (30.0, 30.0));
        p.frame();
        p.press();
        p.drag((6.0, 3.0), 40);
        p.drag((20.0, 10.0), 20);
        p.release();
        for _ in 0..60 {
            p.frame();
        }
        t::clear_probe(&f.view);
        let checks = checks.borrow();
        let bad: Vec<_> = checks.iter().filter(|(_, c)| c.differing > 0).collect();
        eprintln!("frames {} bad {}", checks.len(), bad.len());
        for (pan, c) in bad.iter().take(20) {
            eprintln!("{pan:?} {c:?}");
        }
        assert!(bad.is_empty());
    });
}
