//! Shared GTK test harness: every test closure runs on one dedicated GTK thread.
#![allow(
    dead_code,
    clippy::type_complexity,
    clippy::missing_transmute_annotations
)]
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

/// A key press delivered to `window` as GDK would (with its GdkWindow attached), so the
/// window's default handler propagates it to the focus widget and its ancestors.
pub fn window_key_event(
    window: &gtk::Window,
    keyval: gdk::keys::Key,
    state: gdk::ModifierType,
) -> gdk::Event {
    use glib::translate::*;
    let event = key_event(keyval, state);
    let mut key = event.downcast::<gdk::EventKey>().expect("key event");
    let raw: &mut gdk::ffi::GdkEventKey = key.as_mut();
    raw.window = window.window().expect("realized window").to_glib_full();
    raw.send_event = 1;
    (*key).clone()
}

/// Emit `key-press-event` on `widget`; whether a handler stopped the event.
pub fn press(
    widget: &impl IsA<gtk::Widget>,
    keyval: gdk::keys::Key,
    state: gdk::ModifierType,
) -> bool {
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
pub fn window_pixbuf(
    window: &gdk::Window,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Option<gdk_pixbuf::Pixbuf> {
    use glib::translate::*;
    // SAFETY: the window is alive and the call returns a new reference or NULL.
    unsafe {
        from_glib_full(gdk::ffi::gdk_pixbuf_get_from_window(
            window.to_glib_none().0,
            x,
            y,
            width,
            height,
        ))
    }
}

/// `gtk_widget_destroy`, unsafe in the bindings because other references may remain.
pub fn destroy(widget: &impl IsA<gtk::Widget>) {
    // SAFETY: tests only destroy widgets they own, and use no GTK objects afterwards.
    unsafe { widget.destroy() }
}

use impro_explorer::explorer::NameAction;
use impro_explorer::explorer::{DialogHooks, ExplorerPanel};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Pump until the explorer has merged every listing and its search has settled.
pub fn wait_idle(panel: &ExplorerPanel) {
    let panel = panel.clone();
    pump_until("explorer idle", move || {
        !panel.is_busy() && !panel.search().is_searching() && !panel.search().timeout_pending()
    });
}

/// Run an asynchronous explorer action and wait for its completion and the tree to settle.
pub fn await_op(
    panel: &ExplorerPanel,
    start: impl FnOnce(Box<dyn FnOnce(Result<PathBuf, impro_core::fsops::FsOpError>)>),
) -> Result<PathBuf, impro_core::fsops::FsOpError> {
    let slot = Rc::new(RefCell::new(None));
    let sink = slot.clone();
    start(Box::new(move |result| *sink.borrow_mut() = Some(result)));
    let watched = slot.clone();
    pump_until("explorer operation", move || watched.borrow().is_some());
    wait_idle(panel);
    let result = slot.borrow_mut().take().expect("completed");
    result
}

pub fn create(
    panel: &ExplorerPanel,
    parent: &Path,
    name: &str,
    folder: bool,
) -> Result<PathBuf, impro_core::fsops::FsOpError> {
    await_op(panel, |done| panel.create(parent, name, folder, done))
}

pub fn rename(
    panel: &ExplorerPanel,
    path: &Path,
    name: &str,
) -> Result<PathBuf, impro_core::fsops::FsOpError> {
    await_op(panel, |done| panel.rename(path, name, done))
}

pub fn delete(
    panel: &ExplorerPanel,
    path: &Path,
    permanent: bool,
) -> Result<PathBuf, impro_core::fsops::FsOpError> {
    await_op(panel, |done| panel.delete(path, permanent, done))
}

/// Rows `[icon, name, path]` below `path` (loaded children only).
pub fn children(panel: &ExplorerPanel, path: &Path) -> Vec<[Option<String>; 3]> {
    let store = panel.store();
    let mut rows = Vec::new();
    let Some(row) = panel.find_row(path) else {
        return rows;
    };
    let Some(child) = store.iter_children(Some(&row)) else {
        return rows;
    };
    loop {
        rows.push([
            impro_explorer::util::opt_string(&store, &child, 0),
            impro_explorer::util::opt_string(&store, &child, 1),
            impro_explorer::util::opt_string(&store, &child, 2),
        ]);
        if !store.iter_next(&child) {
            break;
        }
    }
    rows
}

pub fn names(panel: &ExplorerPanel, path: &Path) -> Vec<String> {
    children(panel, path)
        .into_iter()
        .map(|row| row[1].clone().unwrap_or_default())
        .collect()
}

pub fn expand(panel: &ExplorerPanel, path: &Path) {
    let row = panel.find_row(path).expect("row to expand");
    let tree_path = impro_explorer::util::path_of(&panel.store(), &row);
    panel.tree().expand_row(&tree_path, false);
    wait_idle(panel);
}

pub fn collapse(panel: &ExplorerPanel, path: &Path) {
    let row = panel.find_row(path).expect("row to collapse");
    let tree_path = impro_explorer::util::path_of(&panel.store(), &row);
    panel.tree().collapse_row(&tree_path);
}

/// A dialog hook set that records name-dialog requests and optionally auto-answers.
#[derive(Default, Clone)]
pub struct Recorder {
    pub names: Rc<RefCell<Vec<(String, String)>>>,
    pub confirms: Rc<RefCell<Vec<impro_explorer::explorer::ConfirmRequest>>>,
    pub errors: Rc<RefCell<Vec<String>>>,
}

impl Recorder {
    /// Install recording hooks. `name` answers name dialogs (None: record only),
    /// `confirm` answers every removal question.
    pub fn install(
        &self,
        panel: &ExplorerPanel,
        name: Option<Rc<dyn Fn(&str) -> String>>,
        confirm: impl Fn(&impro_explorer::explorer::ConfirmRequest) -> bool + 'static,
    ) {
        let (names, confirms, errors) = (
            self.names.clone(),
            self.confirms.clone(),
            self.errors.clone(),
        );
        let name_hook: Rc<dyn Fn(&str, &str, NameAction)> =
            Rc::new(move |title, initial, action| {
                names
                    .borrow_mut()
                    .push((title.to_owned(), initial.to_owned()));
                if let Some(answer) = &name {
                    action(answer(title), Box::new(|_| {}));
                }
            });
        panel.set_dialog_hooks(DialogHooks {
            name_dialog: Some(name_hook),
            confirm_removal: Some(Rc::new(move |request| {
                confirms.borrow_mut().push(request.clone());
                confirm(request)
            })),
            error_dialog: Some(Rc::new(move |message| {
                errors.borrow_mut().push(message.to_owned())
            })),
            remove: None,
        });
    }
}

/// Minimal XTest driver (loaded at run time) to push real pointer events at the X server.
pub struct XTest {
    display: *mut std::ffi::c_void,
    motion: unsafe extern "C" fn(*mut std::ffi::c_void, i32, i32, i32, u64) -> i32,
    button: unsafe extern "C" fn(*mut std::ffi::c_void, u32, i32, u64) -> i32,
    flush: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
    close: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
}

impl XTest {
    pub fn open() -> Option<Self> {
        unsafe {
            let open = |name: &str| {
                let name = std::ffi::CString::new(name).unwrap();
                libc::dlopen(name.as_ptr(), libc::RTLD_NOW)
            };
            let x11 = open("libX11.so.6");
            let xtst = open("libXtst.so.6");
            if x11.is_null() || xtst.is_null() {
                return None;
            }
            let sym = |lib: *mut std::ffi::c_void, name: &str| {
                let name = std::ffi::CString::new(name).unwrap();
                libc::dlsym(lib, name.as_ptr())
            };
            let open_display: unsafe extern "C" fn(*const i8) -> *mut std::ffi::c_void =
                std::mem::transmute(sym(x11, "XOpenDisplay"));
            let display = open_display(std::ptr::null());
            if display.is_null() {
                return None;
            }
            Some(Self {
                display,
                motion: std::mem::transmute(sym(xtst, "XTestFakeMotionEvent")),
                button: std::mem::transmute(sym(xtst, "XTestFakeButtonEvent")),
                flush: std::mem::transmute(sym(x11, "XFlush")),
                close: std::mem::transmute(sym(x11, "XCloseDisplay")),
            })
        }
    }
    pub fn motion(&self, x: i32, y: i32) {
        unsafe {
            (self.motion)(self.display, -1, x, y, 0);
            (self.flush)(self.display);
        }
    }
    pub fn button(&self, number: u32, press: bool) {
        unsafe {
            (self.button)(self.display, number, i32::from(press), 0);
            (self.flush)(self.display);
        }
    }
}

impl Drop for XTest {
    fn drop(&mut self) {
        unsafe {
            (self.close)(self.display);
        }
    }
}
