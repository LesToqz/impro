mod common;
use common::*;
use gtk::prelude::*;
use impro_ui::motion::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[test]
fn test_endpoints_and_clamping() {
    for key in EASING_POINTS.keys() {
        let e = easing(key).unwrap();
        assert_eq!(e.sample(-1.0), 0.0);
        assert_eq!(e.sample(2.0), 1.0);
        assert_eq!(e.sample(0.0), 0.0);
        assert_eq!(e.sample(1.0), 1.0);
    }
}
#[test]
fn test_non_spring_easings_are_monotonic_without_overshoot() {
    for key in EASING_POINTS.keys().filter(|k| k.as_str() != "jumpy") {
        let e = easing(key).unwrap();
        let mut previous = 0.0;
        for i in 0..=1000 {
            let value = e.sample(i as f64 / 1000.0);
            assert!(value >= previous - 1e-8 && value <= 1.0);
            previous = value;
        }
    }
}
#[test]
fn test_jumpy_preserves_overshoot() {
    let e = easing("jumpy").unwrap();
    assert!((0..100).any(|i| e.sample(i as f64 / 100.0) < 0.0));
    assert!((0..100).any(|i| e.sample(i as f64 / 100.0) > 1.0));
}
#[test]
fn test_balanced_curve_is_symmetric() {
    let e = easing("balanced").unwrap();
    for i in 0..=100 {
        let t = i as f64 / 100.0;
        assert!((e.sample(t) + e.sample(1.0 - t) - 1.0).abs() < 1e-6);
    }
}
#[test]
fn test_default_curve_finishes_most_travel_before_half_time() {
    assert!(easing("smooth").unwrap().sample(0.5) > 0.75);
}
#[test]
fn test_flat_derivatives_use_bisection() {
    let e = cubic_bezier(1.0, 0.0, 0.0, 1.0).unwrap();
    assert!((e.sample(0.5) - 0.5).abs() < 1e-7);
}
#[test]
fn test_invalid_controls_and_inputs() {
    assert!(cubic_bezier(-1.0, 0.0, 1.0, 1.0).is_err());
    assert!(cubic_bezier(0.0, f64::NAN, 1.0, 1.0).is_err());
    assert!(easing("smooth").unwrap().evaluate(f64::NAN).is_err());
    assert!(duration_ms(f64::INFINITY).is_err());
}
#[test]
fn test_duration_tokens_and_numbers() {
    for (key, n) in [
        ("instant", 0.0),
        ("feedback", 96.0),
        ("state", 128.0),
        ("travel", 224.0),
        ("settle", 352.0),
        ("pause", 640.0),
        ("notice_hold", 3840.0),
    ] {
        assert_eq!(duration_ms(key).unwrap(), n);
    }
    assert_eq!(duration_ms(-2).unwrap(), -2.0);
    assert!(duration_ms("bad").is_err());
}
#[test]
fn test_hex_and_rgb_midpoints() {
    assert_eq!(
        lerp_color("#000000", "#ffffff", 0.5).unwrap(),
        "rgba(128, 128, 128, 1)"
    );
    assert_eq!(
        lerp_color("rgb(0, 0, 0)", "rgb(255, 255, 255)", 0.5).unwrap(),
        "rgba(128, 128, 128, 1)"
    );
}
#[test]
fn test_alpha_and_short_hex() {
    assert_eq!(
        lerp_color("#0000", "#ffff", 0.5).unwrap(),
        "rgba(128, 128, 128, 0.5)"
    );
    assert_eq!(
        lerp_color("rgba(100%, 0%, 0%, 50%)", "#00f", 0.5).unwrap(),
        "rgba(128, 0, 128, 0.75)"
    );
    assert!(lerp_color("red", "#000", 0.5).is_err());
}
#[test]
fn test_frame_clock_timing_progress_and_done_once() {
    run(|| {
        set_reduce_motion(false);
        let child = gtk::Label::new(Some("frame"));
        let window = window(&child);
        let values = Rc::new(RefCell::new(Vec::new()));
        let done = Rc::new(Cell::new(0));
        let (v, d) = (values.clone(), done.clone());
        let tween = animate(
            &child,
            150,
            move |p| v.borrow_mut().push(p),
            AnimateOptions {
                on_done: Some(Box::new(move || d.set(d.get() + 1))),
                ..Default::default()
            },
        );
        pump(250);
        assert!(tween.finished());
        assert_eq!(done.get(), 1);
        let values = values.borrow();
        assert_eq!(values.first(), Some(&0.0));
        assert_eq!(values.last(), Some(&1.0));
        assert!(values.len() >= 5 && values.len() <= 20, "{}", values.len());
        assert!(!clock_attached());
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_cancel_stops_updates_without_done() {
    run(|| {
        set_reduce_motion(false);
        let child = gtk::Label::new(None);
        let window = window(&child);
        let done = Rc::new(Cell::new(false));
        let d = done.clone();
        let tween = animate(
            &child,
            150,
            |_| {},
            AnimateOptions {
                on_done: Some(Box::new(move || d.set(true))),
                ..Default::default()
            },
        );
        pump(30);
        tween.cancel();
        pump(180);
        assert!(tween.finished());
        assert!(!done.get());
        assert!(!clock_attached());
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_hidden_fallback_completes() {
    run(|| {
        set_reduce_motion(false);
        let child = gtk::Label::new(None);
        let tween = animate(&child, 30, |_| {}, Default::default());
        pump(100);
        assert!(tween.finished());
        assert!(!clock_attached());
    });
}
#[test]
fn test_update_can_replace_itself() {
    run(|| {
        set_reduce_motion(false);
        let child = gtk::Label::new(None);
        let window = window(&child);
        let count = Rc::new(Cell::new(0));
        let count_c = count.clone();
        let weak = child.downgrade();
        let replacement = Rc::new(RefCell::new(None));
        let r = replacement.clone();
        let first = animate(
            &child,
            40,
            move |p| {
                if p > 0.0 && count_c.get() == 0 {
                    count_c.set(1);
                    if let Some(child) = weak.upgrade() {
                        let next = animate(&child, 20, |_| {}, Default::default());
                        *r.borrow_mut() = Some(next);
                    }
                }
            },
            Default::default(),
        );
        pump(140);
        assert!(first.finished());
        assert!(replacement.borrow().as_ref().unwrap().finished());
        assert_eq!(active_count(), 0);
        assert!(!clock_attached());
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_done_can_start_next_tween() {
    run(|| {
        let child = gtk::Label::new(None);
        let weak = child.downgrade();
        let complete = Rc::new(Cell::new(false));
        let c = complete.clone();
        animate(
            &child,
            20,
            |_| {},
            AnimateOptions {
                on_done: Some(Box::new(move || {
                    if let Some(w) = weak.upgrade() {
                        animate(&w, 20, move |p| c.set(p == 1.0), Default::default());
                    }
                })),
                ..Default::default()
            },
        );
        pump(150);
        assert!(complete.get());
        assert!(!clock_attached());
    });
}
#[test]
fn test_independent_keys() {
    run(|| {
        let child = gtk::Label::new(None);
        let a = animate(
            &child,
            40,
            |_| {},
            AnimateOptions {
                key: MotionKey::Opacity,
                ..Default::default()
            },
        );
        let b = animate(
            &child,
            40,
            |_| {},
            AnimateOptions {
                key: MotionKey::Width,
                ..Default::default()
            },
        );
        a.cancel();
        assert!(!b.finished());
        pump(100);
        assert!(b.finished());
    });
}
#[test]
fn test_helpers_settle_synchronously_with_reduced_motion() {
    run(|| {
        set_reduce_motion(true);
        let child = gtk::Label::new(None);
        child.set_margin_top(7);
        assert!(slide_in(&child, 12, "travel", None).finished());
        assert_eq!(child.margin_top(), 7);
        assert_eq!(child.opacity(), 1.0);
        assert!(fade(&child, false, "fast", true, None).finished());
        assert!(!child.is_visible());
        assert!(
            animate_size_request(&child, Axis::Width, 100, "moderate", true, None)
                .unwrap()
                .finished()
        );
        assert_eq!(child.width_request(), -1);
        let value = Rc::new(Cell::new(0.0));
        let v = value.clone();
        let f = StateFader::new(&child, move |p| v.set(p), 128.0, 0.0);
        f.set(1.0);
        assert_eq!(value.get(), 1.0);
        f.disconnect();
        assert_eq!(active_count(), 0);
        set_reduce_motion(false);
    });
}
#[test]
fn test_destroy_and_unrealize_cancel_quietly() {
    run(|| {
        let child = gtk::Label::new(None);
        let window = window(&child);
        let tween = animate(&child, 100, |_| {}, Default::default());
        unsafe {
            window.destroy();
        }
        pump(20);
        assert!(tween.finished());
        assert!(!clock_attached());
    });
}
#[test]
fn test_reverse_and_end_state() {
    run(|| {
        let child = gtk::Label::new(None);
        let window = window(&child);
        let f = StateFader::new(&child, |_| {}, 50.0, 0.0);
        f.set(1.0);
        pump(30);
        assert!(f.value() > 0.0 && f.value() < 1.0);
        f.set(0.0);
        pump(100);
        assert_eq!(f.value(), 0.0);
        f.disconnect();
        assert!(!clock_attached());
        unsafe {
            window.destroy();
        }
    });
}
#[test]
fn test_update_can_replace_another_widgets_tween() {
    run(|| {
        set_reduce_motion(false);
        let first = gtk::Label::new(None);
        let second = gtk::Label::new(None);
        let replaced = Rc::new(Cell::new(false));
        let r = replaced.clone();
        let second_weak = second.downgrade();
        animate(
            &first,
            40,
            move |p| {
                if p > 0.0 && !r.replace(true) {
                    if let Some(second) = second_weak.upgrade() {
                        animate(&second, 0, |_| {}, Default::default());
                    }
                }
            },
            Default::default(),
        );
        let old = animate(&second, 300, |_| {}, Default::default());
        pump(120);
        assert!(replaced.get());
        assert!(old.finished());
        assert!(!clock_attached());
    });
}
#[test]
fn test_cancel_releases_update_captures() {
    run(|| {
        let child = gtk::Label::new(None);
        let owned = Rc::new(());
        let weak = Rc::downgrade(&owned);
        let tween = animate(
            &child,
            300,
            move |_| {
                let _ = &owned;
            },
            Default::default(),
        );
        assert!(weak.upgrade().is_some());
        tween.cancel();
        assert!(weak.upgrade().is_none());
        assert!(tween.finished());
    });
}
#[test]
fn test_clock_moves_to_another_window() {
    run(|| {
        set_reduce_motion(false);
        let first = gtk::Label::new(None);
        let second = gtk::Label::new(None);
        let a = window(&first);
        let b = window(&second);
        let old = animate(&first, 300, |_| {}, Default::default());
        let next = animate(&second, 60, |_| {}, Default::default());
        unsafe {
            a.destroy();
        }
        pump(150);
        assert!(old.finished());
        assert!(next.finished());
        assert!(!clock_attached());
        unsafe {
            b.destroy();
        }
    });
}
