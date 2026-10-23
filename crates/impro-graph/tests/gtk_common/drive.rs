//! Synthetic user input with real frames in between: every step goes through
//! GTK's event dispatch and then waits for the frame clock.
#![allow(dead_code)]

use std::time::Instant;

use impro_graph::view::testing as t;
use impro_graph::GraphView;

use super::{none, send_pointer};

/// A pointer with its own millisecond clock (events carry real time, so the
/// camera's velocity filter sees what a mouse would give it).
pub struct Pointer<'a> {
    pub view: &'a GraphView,
    start: Instant,
    pub at: (f64, f64),
}

impl<'a> Pointer<'a> {
    pub fn new(view: &'a GraphView, at: (f64, f64)) -> Pointer<'a> {
        Pointer {
            view,
            start: Instant::now(),
            at,
        }
    }

    pub fn time(&self) -> u32 {
        1_000 + self.start.elapsed().as_millis() as u32
    }

    pub fn press(&mut self) {
        send_pointer(
            self.view,
            gdk::EventType::ButtonPress,
            self.at,
            1,
            none(),
            self.time(),
        );
    }

    /// Moves to `to`; `held` marks button 1 as down.
    pub fn move_to(&mut self, to: (f64, f64), held: bool) {
        self.at = to;
        let state = if held {
            gdk::ModifierType::BUTTON1_MASK
        } else {
            none()
        };
        send_pointer(
            self.view,
            gdk::EventType::MotionNotify,
            to,
            0,
            state,
            self.time(),
        );
    }

    pub fn release(&mut self) {
        send_pointer(
            self.view,
            gdk::EventType::ButtonRelease,
            self.at,
            1,
            gdk::ModifierType::BUTTON1_MASK,
            self.time(),
        );
    }

    /// One frame of the canvas's clock.
    pub fn frame(&self) {
        assert!(t::wait_frames(self.view, 1, 5_000), "no frame arrived");
    }

    /// Drags by `step` per frame for `frames` frames, one motion event each.
    pub fn drag(&mut self, step: (f64, f64), frames: u32) {
        for _ in 0..frames {
            self.frame();
            self.move_to((self.at.0 + step.0, self.at.1 + step.1), true);
        }
    }
}

/// Wheel steps at `at`, one per frame.
pub fn wheel_frames(view: &GraphView, at: (f64, f64), pixels: f64, steps: u32) {
    for _ in 0..steps {
        assert!(t::wait_frames(view, 1, 5_000), "no frame arrived");
        t::wheel(view, at.0, at.1, pixels, false);
    }
}
