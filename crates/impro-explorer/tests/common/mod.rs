//! Shared GTK test harness: every test closure runs on one dedicated GTK thread.
#![allow(dead_code)]
use gtk::prelude::*;
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};

/// All GTK tests of a test executable run on the same dedicated GTK thread.
pub fn run(f: impl FnOnce() + Send + 'static) {
    type Job = Box<dyn FnOnce() + Send>;
    static GTK: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let worker = GTK.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            std::env::set_var("GDK_BACKEND", "x11");
            gtk::init().expect("Xvfb display");
            impro_ui::init();
            impro_explorer::init();
            while let Ok(job) = rx.recv() {
                job();
            }
        });
        tx
    });
    let (tx, rx) = mpsc::channel();
    worker
        .send(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            tx.send(result).unwrap();
        }))
        .unwrap();
    if let Err(error) = rx.recv().unwrap() {
        std::panic::resume_unwind(error);
    }
}

/// Run pending main-context work for `ms` milliseconds.
pub fn pump(ms: u64) {
    let deadline = Instant::now() + Duration::from_millis(ms);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Pump the main loop until `condition` holds; `false` after `timeout_ms`.
pub fn poll(timeout_ms: u64, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Like [`poll`] but panics with `what` on timeout.
pub fn pump_until(what: &str, condition: impl FnMut() -> bool) {
    assert!(poll(4000, condition), "timed out pumping GTK: {what}");
}

pub fn key_event(keyval: gdk::keys::Key, state: gdk::ModifierType) -> gdk::Event {
    use glib::translate::IntoGlib;
    let event = gdk::Event::new(gdk::EventType::KeyPress);
    let mut key = event.downcast::<gdk::EventKey>().expect("key event");
    let raw: &mut gdk::ffi::GdkEventKey = key.as_mut();
    raw.keyval = keyval.into_glib();
    raw.state = state.bits();
    (*key).clone()
}

/// Emit `key-press-event` on `widget`; whether a handler stopped the event.
pub fn press(widget: &impl IsA<gtk::Widget>, keyval: gdk::keys::Key, state: gdk::ModifierType) -> bool {
    let event = key_event(keyval, state);
    widget.emit_by_name::<bool>("key-press-event", &[&event])
}

pub fn window(child: &impl IsA<gtk::Widget>, width: i32, height: i32) -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(width, height);
    window.add(child);
    window.show_all();
    pump(30);
    window
}

/// Copy a rectangle of a realized window (the GTK 3 `gdk_pixbuf_get_from_window`).
pub fn window_pixbuf(window: &gdk::Window, x: i32, y: i32, width: i32, height: i32) -> Option<gdk_pixbuf::Pixbuf> {
    use glib::translate::*;
    // SAFETY: the window is alive and the call returns a new reference or NULL.
    unsafe { from_glib_full(gdk::ffi::gdk_pixbuf_get_from_window(window.to_glib_none().0, x, y, width, height)) }
}

/// `gtk_widget_destroy`, unsafe in the bindings because other references may remain.
pub fn destroy(widget: &impl IsA<gtk::Widget>) {
    // SAFETY: tests only destroy widgets they own, and use no GTK objects afterwards.
    unsafe { widget.destroy() }
}
