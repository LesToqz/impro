#![allow(dead_code)]
use gtk::prelude::*;
use impro_editor::{EditorPanel, EditorTab, Response};
use impro_markdown::livemd::{self, LiveMarkdown};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::{mpsc, OnceLock},
};
/// All GTK tests execute on the same dedicated GTK thread, including main-context pumping.
pub fn run(f: impl FnOnce() + Send + 'static) {
    type Job = Box<dyn FnOnce() + Send>;
    static GTK: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let worker = GTK.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            gtk::init().expect("Xvfb display");
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
        let message = error
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "GTK assertion".into());
        panic!("{message}");
    }
}
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
pub fn window(child: &impl gtk::prelude::IsA<gtk::Widget>) -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(500, 300);
    window.add(child);
    window.show_all();
    pump(30);
    window
}
/// Pump until `f` holds (30 s limit).
pub fn poll(f: impl Fn() -> bool) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !f() {
        assert!(std::time::Instant::now() < until, "operation timed out");
        pump(2);
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

/// An editor panel in a window, like the Python suites' fixture.
pub struct Harness {
    pub dir: tempfile::TempDir,
    pub panel: EditorPanel,
    pub window: gtk::Window,
}
impl Harness {
    pub fn new() -> Self {
        Self::sized(1000, 1000)
    }
    pub fn sized(width: i32, height: i32) -> Self {
        Self::build(width, height, false)
    }
    /// Like `sized`, with the temp dir as the project root (so `.impro/Notes` are notes).
    pub fn rooted(width: i32, height: i32) -> Self {
        Self::build(width, height, true)
    }
    fn build(width: i32, height: i32, rooted: bool) -> Self {
        impro_ui::init();
        impro_editor::init();
        impro_markdown::init();
        impro_ui::apply_style(None).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let panel = if rooted {
            EditorPanel::new(Some(&dir.path().canonicalize().unwrap()))
        } else {
            EditorPanel::default()
        };
        panel.set_dialog_hook(Some(Rc::new(|_, _| Response::Cancel)));
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(width, height);
        window.add(&panel);
        window.show_all();
        pump(30);
        Self { dir, panel, window }
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    /// Open `name` (relative to the temp dir) and wait for its tab.
    pub fn open(&self, name: &str, text: &str) -> Rc<EditorTab> {
        let path = self.path(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, text).unwrap();
        self.open_path(&path)
    }
    pub fn open_path(&self, path: &Path) -> Rc<EditorTab> {
        self.panel.open_file(path);
        poll(|| !self.panel.is_busy());
        self.panel.tab_for_path(path).expect("opened tab")
    }
    /// A note under `.impro/Notes`.
    pub fn note(&self, name: &str, text: &str) -> Rc<EditorTab> {
        self.open(&format!(".impro/Notes/{name}"), text)
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        unsafe { self.window.destroy() };
        pump(5);
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
