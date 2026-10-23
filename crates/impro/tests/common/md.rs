//! Helpers for the live-note cases that run inside the real main window
//! (the editor-level counterparts live in the `impro-markdown` tests).
#![allow(dead_code)]
use gtk::prelude::*;
use impro_editor::{EditorPanel, EditorTab};
use impro_markdown::livemd::{self, LiveMarkdown};

/// Run the main context for `ms` milliseconds (bounded, for events that need a
/// frame or two to be delivered).
pub fn pump(ms: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
/// Pump until the renderer has nothing queued and everything is painted.
pub fn settle(live: &LiveMarkdown) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if !live.pending() && live.unpainted_blocks() == 0 && live.jobs_in_flight() == 0 {
            break;
        }
        assert!(std::time::Instant::now() < until, "renderer did not settle");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    // One more round for work the settle itself queued (caret, layout).
    for _ in 0..3 {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if live.pending() {
            settle(live);
        }
    }
}

pub fn live_of(tab: &EditorTab) -> LiveMarkdown {
    livemd::for_view(&tab.view).expect("live renderer")
}
pub fn text_of(buffer: &gtk::TextBuffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .map(|s| s.to_string())
        .unwrap_or_default()
}
/// A key-press event with the given key and modifier state.
pub fn key_event(key: gdk::keys::Key, state: gdk::ModifierType) -> gdk::EventKey {
    use glib::translate::*;
    let mut event = gdk::Event::new(gdk::EventType::KeyPress);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).key.keyval = *key;
        (*raw).key.state = state.bits();
        (*raw).key.send_event = 1;
    }
    event.downcast::<gdk::EventKey>().expect("key event")
}
/// Key through the editor panel's routing (like the Python `_key_press`).
pub fn panel_key(panel: &EditorPanel, key: gdk::keys::Key, state: gdk::ModifierType) -> bool {
    panel.handle_key(&key_event(key, state))
}
/// Emit `key-press-event` on a widget like real input (window set so GTK accepts it).
pub fn emit_key(
    widget: &impl IsA<gtk::Widget>,
    key: gdk::keys::Key,
    state: gdk::ModifierType,
) -> bool {
    use glib::translate::*;
    let mut event = gdk::Event::new(gdk::EventType::KeyPress);
    let window = widget.as_ref().window();
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).key.keyval = *key;
        (*raw).key.state = state.bits();
        (*raw).key.send_event = 1;
        if let Some(seat) = gdk::Display::default().and_then(|d| d.default_seat()) {
            gdk::Event::set_device(&mut event, seat.keyboard().as_ref());
        }
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        if let Some(window) = window {
            (*raw).key.window = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_full(&window);
        }
        let keymap = gdk::Keymap::for_display(&gdk::Display::default().unwrap());
        if let Some(keymap) = keymap {
            if let Some(first) = keymap.entries_for_keyval(*key).first() {
                (*raw).key.hardware_keycode = first.keycode() as u16;
                (*raw).key.group = first.group() as u8;
            }
        }
    }
    widget
        .as_ref()
        .emit_by_name::<bool>("key-press-event", &[&event])
}
/// Dispatch a real button event (press/release) to `window` through GTK's main event path.
pub fn send_button(kind: gdk::EventType, window: &gdk::Window, x: f64, y: f64, button: u32) {
    use glib::translate::*;
    let mut event = gdk::Event::new(kind);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).button.window = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_full(window);
        (*raw).button.send_event = 1;
        (*raw).button.x = x;
        (*raw).button.y = y;
        (*raw).button.button = button;
    }
    let seat = gdk::Display::default().and_then(|d| d.default_seat());
    if let Some(seat) = seat {
        event.set_device(seat.pointer().as_ref());
    }
    gtk::main_do_event(&mut event);
}
/// A bare event of `kind` (focus, scroll, button...) for emitting on a widget.
pub fn bare_event(kind: gdk::EventType) -> gdk::Event {
    gdk::Event::new(kind)
}
/// A button event with window, position and pointer device set.
pub fn button_event_at(
    kind: gdk::EventType,
    window: &gdk::Window,
    x: f64,
    y: f64,
    button: u32,
) -> gdk::Event {
    use glib::translate::*;
    let mut event = gdk::Event::new(kind);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).button.window = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_full(window);
        (*raw).button.send_event = 1;
        (*raw).button.x = x;
        (*raw).button.y = y;
        (*raw).button.button = button;
    }
    if let Some(seat) = gdk::Display::default().and_then(|d| d.default_seat()) {
        event.set_device(seat.pointer().as_ref());
    }
    event
}
