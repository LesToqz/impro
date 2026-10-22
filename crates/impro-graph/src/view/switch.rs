//! The panel's 40 × 22 toggle switch: a pill with a sliding 16 px thumb.

use std::cell::{Cell, RefCell};
use std::f64::consts::TAU;
use std::rc::Rc;

use atk::prelude::*;
use gtk::prelude::*;
use impro_ui::motion::{self, AnimateOptions, MotionKey};

pub const SIZE: (i32, i32) = (40, 22);

/// Tells assistive technology that a state flag changed.
pub fn notify_state(object: &atk::Object, state: atk::StateType, value: bool) {
    use atk::glib::translate::IntoGlib;
    let bit: u64 = 1u64 << (state.into_glib() as u32);
    object.notify_state_change(bit, value);
}
const THUMB: f64 = 16.0;
const INSET: f64 = 3.0;
const TRAVEL: f64 = 18.0;
const DURATION_MS: i32 = 120;

struct Inner {
    area: gtk::DrawingArea,
    active: Cell<bool>,
    /// 0 = off, 1 = on, between while sliding.
    position: Cell<f64>,
    animate: Cell<bool>,
    key: MotionKey,
    handlers: RefCell<Vec<Rc<dyn Fn(bool)>>>,
}

/// A switch; clones share state.
#[derive(Clone)]
pub struct Switch {
    inner: Rc<Inner>,
}

fn color(ctx: &gtk::StyleContext, name: &str, fallback: (f64, f64, f64)) -> gdk::RGBA {
    ctx.lookup_color(name)
        .unwrap_or_else(|| gdk::RGBA::new(fallback.0, fallback.1, fallback.2, 1.0))
}

fn mix(a: &gdk::RGBA, b: &gdk::RGBA, t: f64) -> (f64, f64, f64) {
    (
        a.red() + (b.red() - a.red()) * t,
        a.green() + (b.green() - a.green()) * t,
        a.blue() + (b.blue() - a.blue()) * t,
    )
}

impl Switch {
    pub fn new(label: &str) -> Switch {
        let area = gtk::DrawingArea::new();
        area.set_size_request(SIZE.0, SIZE.1);
        area.set_halign(gtk::Align::End);
        area.set_valign(gtk::Align::Center);
        area.set_can_focus(true);
        area.add_events(
            gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::BUTTON_RELEASE_MASK
                | gdk::EventMask::KEY_PRESS_MASK,
        );
        area.style_context().add_class("impro-graph-switch");
        if let Some(a) = area.accessible() {
            a.set_role(atk::Role::for_name("switch"));
            a.set_name(label);
        }
        let inner = Rc::new(Inner {
            area: area.clone(),
            active: Cell::new(false),
            position: Cell::new(0.0),
            animate: Cell::new(true),
            key: MotionKey::unique(),
            handlers: RefCell::new(Vec::new()),
        });
        let switch = Switch { inner };
        {
            let inner = switch.inner.clone();
            area.connect_draw(move |area, cr| {
                let ctx = area.style_context();
                let track = color(&ctx, "impro_graph_track", (0.31, 0.4, 0.47));
                let accent = color(&ctx, "impro_graph_accent", (0.39, 0.7, 0.93));
                let t = inner.position.get();
                let (r, g, b) = mix(&track, &accent, t);
                let (w, h) = (
                    area.allocated_width() as f64,
                    area.allocated_height() as f64,
                );
                let radius = h / 2.0;
                cr.new_sub_path();
                cr.arc(
                    radius,
                    radius,
                    radius,
                    std::f64::consts::FRAC_PI_2,
                    3.0 * std::f64::consts::FRAC_PI_2,
                );
                cr.arc(
                    w - radius,
                    radius,
                    radius,
                    -std::f64::consts::FRAC_PI_2,
                    std::f64::consts::FRAC_PI_2,
                );
                cr.close_path();
                cr.set_source_rgb(r, g, b);
                let _ = cr.fill();
                let x = INSET + THUMB / 2.0 + TRAVEL * t;
                let y = h / 2.0;
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.3);
                cr.arc(x, y + 1.0, THUMB / 2.0 + 0.5, 0.0, TAU);
                let _ = cr.fill();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.arc(x, y, THUMB / 2.0, 0.0, TAU);
                let _ = cr.fill();
                if area.has_focus() {
                    let text = color(&ctx, "impro_graph_text", (0.9, 0.93, 0.96));
                    cr.set_source_rgba(text.red(), text.green(), text.blue(), 1.0);
                    cr.set_line_width(2.0);
                    cr.new_sub_path();
                    cr.arc(
                        radius,
                        radius,
                        radius + 1.0,
                        std::f64::consts::FRAC_PI_2,
                        3.0 * std::f64::consts::FRAC_PI_2,
                    );
                    cr.arc(
                        w - radius,
                        radius,
                        radius + 1.0,
                        -std::f64::consts::FRAC_PI_2,
                        std::f64::consts::FRAC_PI_2,
                    );
                    cr.close_path();
                    let _ = cr.stroke();
                }
                glib::Propagation::Proceed
            });
        }
        {
            let switch = switch.clone();
            area.connect_button_press_event(move |area, _| {
                area.grab_focus();
                glib::Propagation::Stop
            });
            let release = switch.clone();
            area.connect_button_release_event(move |area, event| {
                let (x, y) = event.position();
                if x >= 0.0
                    && y >= 0.0
                    && x <= area.allocated_width() as f64
                    && y <= area.allocated_height() as f64
                {
                    release.toggle();
                }
                glib::Propagation::Stop
            });
            let keys = switch.clone();
            area.connect_key_press_event(move |_, event| {
                let key = event.keyval();
                if key == gdk::keys::constants::space
                    || key == gdk::keys::constants::Return
                    || key == gdk::keys::constants::KP_Enter
                {
                    keys.toggle();
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
            area.connect_focus_in_event(|a, _| {
                a.queue_draw();
                glib::Propagation::Proceed
            });
            area.connect_focus_out_event(|a, _| {
                a.queue_draw();
                glib::Propagation::Proceed
            });
        }
        switch
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.inner.area
    }

    pub fn is_active(&self) -> bool {
        self.inner.active.get()
    }

    /// Turns animation of the thumb on or off (reduced motion).
    pub fn set_animated(&self, animated: bool) {
        self.inner.animate.set(animated);
    }

    /// Runs `f` whenever the user flips the switch.
    pub fn connect_toggled(&self, f: impl Fn(bool) + 'static) {
        self.inner.handlers.borrow_mut().push(Rc::new(f));
    }

    /// Sets the state without notifying handlers.
    pub fn set_active(&self, active: bool) {
        if self.inner.active.get() == active {
            return;
        }
        self.inner.active.set(active);
        self.slide();
    }

    /// Flips the switch as the user would and notifies handlers.
    pub fn toggle(&self) {
        let active = !self.inner.active.get();
        self.inner.active.set(active);
        self.slide();
        let handlers: Vec<_> = self.inner.handlers.borrow().clone();
        for h in handlers {
            h(active);
        }
    }

    fn slide(&self) {
        let inner = &self.inner;
        let target = if inner.active.get() { 1.0 } else { 0.0 };
        if let Some(a) = inner.area.accessible() {
            notify_state(&a, atk::StateType::Checked, inner.active.get());
        }
        let from = inner.position.get();
        if !inner.animate.get() || !inner.area.is_mapped() {
            inner.position.set(target);
            inner.area.queue_draw();
            return;
        }
        let weak = Rc::downgrade(inner);
        motion::animate(
            &inner.area,
            DURATION_MS,
            move |p| {
                if let Some(inner) = weak.upgrade() {
                    inner.position.set(from + (target - from) * p);
                    inner.area.queue_draw();
                }
            },
            AnimateOptions {
                key: inner.key,
                ..Default::default()
            },
        );
    }

    /// Thumb position, `0.0..=1.0` (for tests).
    pub fn position(&self) -> f64 {
        self.inner.position.get()
    }
}
