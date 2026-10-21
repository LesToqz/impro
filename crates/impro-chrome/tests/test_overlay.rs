mod common;
use common::*;
use gtk::prelude::*;
use impro_chrome::overlay::*;
use std::{cell::RefCell, rc::Rc};
#[test]
fn test_stack_position_order_and_gap() {
    run(|| {
        let f = Fixture::new();
        let first = f.host.notify("first", 0, "info").unwrap();
        let second=f.host.notify("second, a much longer notice that wraps onto more lines than one because it exceeds three hundred pixels",0,"info").unwrap();
        pump(60);
        let (x1, y1, w1, h1) = notice_rect(&f.host, &first);
        let (x2, y2, w2, h2) = notice_rect(&f.host, &second);
        assert_eq!((y1, x1 + w1), (10, 890));
        assert_eq!((y2, x2 + w2), (y1 + h1 + 14, 890));
        assert_eq!(w2, 300);
        assert!(h2 > h1);
        assert!(w1 < 300);
        assert!(second.label().is_line_wrap());
    });
}
#[test]
fn test_style_black_ninety_percent_and_text() {
    run(|| {
        let f = Fixture::new();
        let n = f.host.notify("styled", 0, "info").unwrap();
        pump(40);
        let value = n
            .box_widget()
            .style_context()
            .style_property_for_state("background-color", gtk::StateFlags::NORMAL)
            .get::<gdk::RGBA>()
            .unwrap();
        assert_eq!((value.red(), value.green(), value.blue()), (0., 0., 0.));
        assert!((value.alpha() - 0.9).abs() < 0.002);
        assert_eq!(
            n.box_widget()
                .style_context()
                .style_property_for_state("border-radius", gtk::StateFlags::NORMAL)
                .get::<i32>()
                .unwrap(),
            8
        );
        assert_eq!(n.text(), "styled");
    });
}
#[test]
fn test_max_five_dismisses_oldest() {
    run(|| {
        let f = Fixture::new();
        let notices = (0..7)
            .map(|i| f.host.notify(&format!("n{i}"), 0, "info").unwrap())
            .collect::<Vec<_>>();
        assert_eq!(f.host.notices().len(), 5);
        assert!(notices[0].parent().is_none());
        assert!(notices[1].parent().is_none());
        assert_eq!(f.host.notices()[0].text(), "n2");
    });
}
#[test]
fn test_timeout_dismisses() {
    run(|| {
        let f = Fixture::new();
        let n = f.host.notify("short", 60, "info").unwrap();
        let sticky = f.host.notify("sticky", 0, "info").unwrap();
        poll(|| n.parent().is_none());
        pump(100);
        assert_eq!(f.host.notices().len(), 1);
        assert_eq!(notice_rect(&f.host, &sticky).1, 10);
    });
}
#[test]
fn test_click_dismisses() {
    run(|| {
        let f = Fixture::new();
        let n = f.host.notify("click me", 0, "info").unwrap();
        f.host.notify("stay", 0, "info").unwrap();
        pump(30);
        click(n.widget(), 10., 10.);
        assert_eq!(f.host.notices().len(), 1);
        assert_eq!(f.host.notices()[0].text(), "stay");
        assert!(f.text.has_focus());
    });
}
fn crossing(n: &Notice, kind: gdk::EventType) {
    use glib::translate::*;
    let mut event = gdk::Event::new(kind);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).crossing.detail = gdk::NotifyType::Nonlinear.into_glib();
    }
    n.widget().emit_by_name::<bool>(
        if kind == gdk::EventType::EnterNotify {
            "enter-notify-event"
        } else {
            "leave-notify-event"
        },
        &[&event],
    );
}
#[test]
fn test_hover_pauses_and_leave_restarts_grace_timer() {
    run(|| {
        let f = Fixture::new();
        let n = f.host.notify("hover", 200, "info").unwrap();
        crossing(&n, gdk::EventType::EnterNotify);
        assert!(!n.timer_active());
        pump(300);
        assert_eq!(f.host.notices().len(), 1);
        crossing(&n, gdk::EventType::LeaveNotify);
        assert!(n.timer_active());
        poll(|| n.parent().is_none());
    });
}
#[test]
fn test_invalid_arguments() {
    run(|| {
        let f = Fixture::new();
        assert!(f.host.notify("x", 0, "fatal").is_err());
        assert!(f.host.notify("x", -1, "info").is_err());
        for level in NOTICE_LEVELS {
            let n = f.host.notify(level, 0, level).unwrap();
            assert!(n
                .box_widget()
                .style_context()
                .has_class(&format!("impro-notice-{level}")));
        }
    });
}
#[test]
fn test_reduced_motion_is_instant() {
    run(|| {
        let f = Fixture::new();
        let n = f.host.notify("instant", 0, "info").unwrap();
        assert_eq!(n.offset(), 0.);
        f.host.dismiss(&n);
        assert!(n.parent().is_none());
        assert!(f.host.notices().is_empty());
        assert!(!n.timer_active());
    });
}
#[test]
fn test_slide_in_and_fade_out_animate() {
    run(|| {
        let f = Fixture::new();
        impro_ui::motion::set_reduce_motion(false);
        let n = f.host.notify("animated", 0, "info").unwrap();
        assert_eq!(n.offset(), 350.);
        pump(40);
        let mid = notice_rect(&f.host, &n).0;
        poll(|| n.offset() == 0.);
        pump(30);
        assert!(mid > notice_rect(&f.host, &n).0);
        let below = f.host.notify("below", 0, "info").unwrap();
        poll(|| below.offset() == 0.);
        pump(30);
        let before = notice_rect(&f.host, &below).1;
        f.host.dismiss(&n);
        assert_eq!(f.host.notices().len(), 1);
        assert!(n.parent().is_some());
        pump(50);
        assert!(n.opacity() < 1.);
        poll(|| n.parent().is_none());
        pump(30);
        assert!(notice_rect(&f.host, &below).1 < before);
        assert_eq!(notice_rect(&f.host, &below).1, 10);
        f.host.clear_notices();
        pump(30);
        assert_eq!(impro_ui::motion::active_count(), 0);
        assert!(!impro_ui::motion::clock_attached());
    });
}
#[test]
fn test_notices_stay_above_prompt() {
    run(|| {
        let f = Fixture::new();
        let n = f.host.notify("above", 0, "info").unwrap();
        let p = prompt(1);
        f.show(&p);
        let children = f.host.children();
        let index = |w: &gtk::Widget| children.iter().position(|c| c == w).unwrap();
        assert!(index(n.widget().upcast_ref()) > index(p.widget().upcast_ref()));
    });
}
#[test]
fn test_css_parses_for_every_theme() {
    run(|| {
        for theme in impro_core::themes::themes() {
            gtk::CssProvider::new()
                .load_from_data(impro_chrome::overlay::css(&theme.ui).as_bytes())
                .unwrap();
        }
    });
}
#[test]
fn test_host_wraps_child_and_prompt_signals() {
    run(|| {
        let f = Fixture::new();
        assert_eq!(f.host.child(), Some(f.text.clone().upcast()));
        let events = Rc::new(RefCell::new(vec![]));
        let out = events.clone();
        f.host
            .connect_prompt_shown(move |_| out.borrow_mut().push("shown"));
        let out = events.clone();
        f.host
            .connect_prompt_hidden(move |_| out.borrow_mut().push("hidden"));
        let p = prompt(0);
        f.host.show_prompt(&p);
        f.host.show_prompt(&p);
        f.host.hide_prompt();
        f.host.hide_prompt();
        assert_eq!(*events.borrow(), ["shown", "hidden"]);
        assert_eq!(f.host.children().len(), 1);
    });
}
#[test]
fn test_restore_skips_destroyed_previous_focus() {
    run(|| {
        let f = Fixture::new();
        f.host.remove(&f.text);
        let entry = gtk::Entry::new();
        f.host.add(&entry);
        entry.show();
        entry.grab_focus();
        let p = prompt(0);
        f.show(&p);
        unsafe {
            entry.destroy();
        }
        p.close();
        assert!(f.host.prompt().is_none());
    });
}
#[test]
fn test_prompt_does_not_dim_the_content() {
    run(|| {
        let f = Fixture::new();
        let p = prompt(0);
        f.show(&p);
        assert_eq!(f.host.opacity(), 1.);
        assert_eq!(f.text.opacity(), 1.);
        let catcher = f
            .host
            .children()
            .into_iter()
            .find(|w| w.style_context().has_class("impro-overlay-catcher"))
            .unwrap()
            .downcast::<gtk::EventBox>()
            .unwrap();
        assert!(!catcher.is_visible_window());
        assert_eq!(catcher.opacity(), 1.);
    });
}
#[test]
#[ignore]
fn benchmark_notices() {
    run(|| {
        let f = Fixture::new();
        let start = std::time::Instant::now();
        for i in 0..100 {
            f.host.notify(&format!("notice {i}"), 0, "info").unwrap();
        }
        println!(
            "100 notices: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.
        );
        assert_eq!(f.host.notices().len(), 5);
    });
}
