//! Labels must not blink or shimmer while the camera only translates.

mod gtk_common;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk_common::drive::*;
use gtk_common::*;
use impro_graph::view::draw::LabelDrawn;
use impro_graph::view::testing as t;

struct Frame {
    pan: (f64, f64),
    zoom: f64,
    motion: bool,
    labels: HashMap<String, LabelDrawn>,
}

fn logical(w: i32, h: i32) -> (i32, i32) {
    let scale: i32 = std::env::var("GDK_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    (w / scale, h / scale)
}

fn scale() -> f64 {
    std::env::var("GDK_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0)
}

/// Pans by `step` device pixels per frame with the pointer held, returning the frames painted.
fn pan(f: &Fixture, step: (f64, f64), frames: u32) -> Vec<Frame> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    t::set_probe(&f.view, move |shot| {
        let info = shot.info();
        sink.borrow_mut().push(Frame {
            pan: info.pan,
            zoom: info.zoom,
            motion: shot.moving(),
            labels: shot.labels().into_iter().collect(),
        });
    });
    let (w, h) = t::info(&f.view).size;
    let mut p = Pointer::new(&f.view, (w / 2.0, h / 2.0));
    // start on a marker-free spot
    let spot = empty_spot(f, 6.0);
    p.at = spot;
    p.frame();
    p.press();
    let s = scale();
    p.drag((step.0 / s, step.1 / s), frames);
    p.frame();
    t::clear_probe(&f.view);
    let frames = std::mem::take(&mut *log.borrow_mut());
    p.release();
    frames
}

/// Labels that were fully visible in one frame and then vanished, changed
/// shape or brightness, or moved other than with the camera in the next.
fn flickers(frames: &[Frame], size: (f64, f64)) -> Vec<String> {
    let mut bad = Vec::new();
    for (n, pair) in frames.windows(2).enumerate() {
        let (a, b) = (&pair[0], &pair[1]);
        if a.zoom != b.zoom {
            continue;
        }
        let (dx, dy) = (b.pan.0 - a.pan.0, b.pan.1 - a.pan.1);
        for (id, la) in &a.labels {
            let inside = |x: f64, y: f64| {
                x >= 1.0 && y >= 1.0 && x + la.w <= size.0 - 1.0 && y + la.h <= size.1 - 1.0
            };
            if la.alpha < 0.05 || !inside(la.x, la.y) || !inside(la.x + dx, la.y + dy) {
                continue;
            }
            match b.labels.get(id) {
                None => bad.push(format!(
                    "frame {n}: {id} vanished (motion {}→{})",
                    a.motion, b.motion
                )),
                Some(lb) => {
                    let (mx, my) = ((lb.x - la.x - dx).abs(), (lb.y - la.y - dy).abs());
                    if mx > 1.0 || my > 1.0 {
                        bad.push(format!(
                            "frame {n}: {id} moved by ({mx:.2}, {my:.2}) off the camera"
                        ));
                    }
                    if (lb.alpha - la.alpha).abs() > 0.02 {
                        bad.push(format!("frame {n}: {id} alpha {} → {}", la.alpha, lb.alpha));
                    }
                    if lb.bitmap_px != la.bitmap_px || lb.stretched != la.stretched {
                        bad.push(format!(
                            "frame {n}: {id} bitmap {}/{} → {}/{}",
                            la.bitmap_px, la.stretched, lb.bitmap_px, lb.stretched
                        ));
                    }
                }
            }
        }
    }
    bad
}

fn big() -> Fixture {
    let (w, h) = logical(1920, 1080);
    let f = Fixture::big((w, h));
    // a compositor may still be sizing the window; the camera follows it
    t::pump(600);
    assert!(t::settle(&f.view, 20_000), "never settled");
    t::capture_frames(&f.view, true);
    f
}

fn zoom_to(f: &Fixture, steps: i32) {
    let (w, h) = t::info(&f.view).size;
    wheel_frames(
        &f.view,
        (w / 2.0, h / 2.0),
        if steps > 0 { -60.0 } else { 60.0 },
        steps.unsigned_abs(),
    );
    assert!(t::settle(&f.view, 20_000), "never settled");
}

fn check(name: &str, f: &Fixture, step: (f64, f64), frames: u32) {
    let size = t::info(&f.view).size;
    let log = pan(f, step, frames);
    let with_labels = log.iter().filter(|fr| !fr.labels.is_empty()).count();
    let bad = flickers(&log, size);
    eprintln!(
        "{name}: {} frames ({with_labels} with labels, {} moving), {} flickers",
        log.len(),
        log.iter().filter(|fr| fr.motion).count(),
        bad.len()
    );
    for b in bad.iter().take(12) {
        eprintln!("  {b}");
    }
    assert!(
        with_labels > frames as usize / 2,
        "{name}: no labels were drawn"
    );
    assert!(bad.is_empty(), "{name}: {} flickers", bad.len());
}

#[test]
fn labels_stay_put_while_the_camera_pans_slowly() {
    run(|| {
        let f = big();
        for step in [(1.0, 0.0), (2.0, 1.0), (4.0, 3.0)] {
            check(&format!("zoom 1, {step:?} px/frame"), &f, step, 60);
        }
    });
}

#[test]
fn labels_stay_put_zoomed_in_and_out() {
    run(|| {
        let f = big();
        zoom_to(&f, 6);
        check("zoomed in", &f, (3.0, 2.0), 60);
        zoom_to(&f, -9);
        check("zoomed out", &f, (3.0, 2.0), 60);
        check("zoomed out, fast", &f, (17.0, 9.0), 60);
    });
}
