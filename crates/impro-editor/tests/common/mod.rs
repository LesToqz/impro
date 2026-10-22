#![allow(dead_code)]
use gtk::prelude::*;
use std::sync::{mpsc, OnceLock};
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
    use gtk::prelude::*;
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(500, 300);
    window.add(child);
    window.show_all();
    pump(30);
    window
}
pub fn poll(f: impl Fn() -> bool) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !f() {
        assert!(std::time::Instant::now() < until, "operation timed out");
        pump(2);
    }
}

use impro_editor::{EditorPanel, EditorTab, Response};
use std::{cell::Cell, path::Path, rc::Rc};
pub struct Harness {
    pub dir: tempfile::TempDir,
    pub panel: EditorPanel,
    pub window: gtk::Window,
}
impl Harness {
    pub fn new() -> Self {
        impro_editor::init();
        impro_ui::apply_style(None).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let panel = EditorPanel::default();
        panel.set_dialog_hook(Some(Rc::new(|_, _| Response::Cancel)));
        let window = window(&panel);
        Self { dir, panel, window }
    }
    pub fn open(&self, name: &str, text: &str) -> Rc<EditorTab> {
        let path = self.dir.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, text).unwrap();
        self.panel.open_file(&path);
        poll(|| !self.panel.is_busy());
        self.panel.tab_for_path(path).expect("opened tab")
    }
    pub fn settled(&self) {
        poll(|| {
            self.panel
                .current_tab()
                .is_none_or(|t| !t.highlighter.pending())
        });
        pump(20);
    }
    pub fn answer(&self, response: Response) {
        self.panel
            .set_dialog_hook(Some(Rc::new(move |_, _| response)));
    }
    pub fn save(&self) -> bool {
        result(|cb| self.panel.save_current(cb))
    }
    pub fn save_all(&self) -> bool {
        result(|cb| self.panel.save_all(cb))
    }
    pub fn close(&self, tab: &Rc<EditorTab>) -> bool {
        result(|cb| self.panel.close_tab(tab, cb))
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        unsafe {
            self.window.destroy();
        }
        pump(5);
    }
}
pub fn result(call: impl FnOnce(Box<dyn FnOnce(bool)>)) -> bool {
    let result = Rc::new(Cell::new(None));
    let value = result.clone();
    call(Box::new(move |ok| value.set(Some(ok))));
    poll(|| result.get().is_some());
    result.get().unwrap()
}
pub fn replace(t: &EditorTab, text: &str) {
    t.buffer.begin_user_action();
    t.buffer.set_text(text);
    t.buffer.end_user_action();
}
pub fn key(panel: &EditorPanel, key: gdk::keys::Key, ctrl: bool, shift: bool) -> bool {
    use glib::translate::*;
    let mut event = gdk::Event::new(gdk::EventType::KeyPress);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).key.keyval = *key;
        (*raw).key.state = (if ctrl { gdk::ffi::GDK_CONTROL_MASK } else { 0 })
            | (if shift { gdk::ffi::GDK_SHIFT_MASK } else { 0 });
    }
    let event = event.downcast::<gdk::EventKey>().unwrap();
    panel.handle_key(&event)
}
pub fn selected(t: &EditorTab) -> String {
    t.buffer
        .selection_bounds()
        .map(|(a, b)| t.buffer.text(&a, &b, true).unwrap_or_default().to_string())
        .unwrap_or_default()
}
pub fn tag(t: &EditorTab, offset: i32) -> Option<String> {
    t.buffer
        .iter_at_offset(offset)
        .tags()
        .iter()
        .filter_map(|t| t.name())
        .find(|s| s.starts_with("hl-"))
        .map(|s| s.to_string())
}
pub fn set_path_time(path: &Path, mtime: std::time::SystemTime) {
    let file = std::fs::File::open(path).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(mtime))
        .unwrap();
}
pub fn pixels(window: &gtk::Window) -> gdk_pixbuf::Pixbuf {
    pump(40);
    window
        .window()
        .unwrap()
        .pixbuf(0, 0, window.allocated_width(), window.allocated_height())
        .unwrap()
}
pub fn pixel(image: &gdk_pixbuf::Pixbuf, x: i32, y: i32) -> (u8, u8, u8) {
    let data = image.read_pixel_bytes();
    let start = (y * image.rowstride() + x * image.n_channels()) as usize;
    let bytes = data.as_ref();
    (bytes[start], bytes[start + 1], bytes[start + 2])
}
pub fn rgb(value: &str) -> (u8, u8, u8) {
    (
        u8::from_str_radix(&value[1..3], 16).unwrap(),
        u8::from_str_radix(&value[3..5], 16).unwrap(),
        u8::from_str_radix(&value[5..7], 16).unwrap(),
    )
}
pub fn button_event(button: u32) -> gdk::Event {
    use glib::translate::*;
    let mut event = gdk::Event::new(gdk::EventType::ButtonPress);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).button.button = button;
    }
    event
}
pub fn real_key(window: &gtk::Window, widget: &impl IsA<gtk::Widget>, key: gdk::keys::Key) -> bool {
    use glib::translate::*;
    let mut event = gdk::Event::new(gdk::EventType::KeyPress);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).key.keyval = *key;
        (*raw).key.state = gdk::ffi::GDK_CONTROL_MASK;
        (*raw).key.window = widget.as_ref().window().unwrap().to_glib_full();
        (*raw).key.send_event = 1;
    }
    let device = window.display().default_seat().unwrap().keyboard();
    event.set_device(device.as_ref());
    let event = event.downcast::<gdk::EventKey>().unwrap();
    window.propagate_key_event(&event)
}
