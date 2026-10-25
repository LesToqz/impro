//! Shared GTK test harness: every test closure runs on one dedicated GTK thread, each
//! with its own settings directory (`XDG_CONFIG_HOME`).
#![allow(dead_code, clippy::type_complexity)]
use gtk::prelude::*;
use impro_core::settings::Values;
use impro_settings_ui::SettingsDialog;
use serde_json::Value;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{mpsc, OnceLock},
    time::{Duration, Instant},
};

pub fn run(f: impl FnOnce() + Send + 'static) {
    type Job = Box<dyn FnOnce() + Send>;
    static GTK: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let worker = GTK.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            #[cfg(unix)]
            std::env::set_var("GDK_BACKEND", "x11");
            gtk::init().expect("Xvfb display");
            impro_ui::init();
            impro_settings_ui::init();
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

pub fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Deliver a key press to the toplevel of `widget` as GDK would.
pub fn send_key(widget: &impl IsA<gtk::Widget>, keyval: gdk::keys::Key, state: gdk::ModifierType) {
    use glib::translate::*;
    let toplevel = widget
        .toplevel()
        .and_downcast::<gtk::Window>()
        .expect("toplevel window");
    let window = toplevel.window().expect("realized window");
    let event = gdk::Event::new(gdk::EventType::KeyPress);
    let mut key = event.downcast::<gdk::EventKey>().expect("key event");
    let keycode = gdk::Keymap::for_display(&window.display())
        .and_then(|map| map.entries_for_keyval(*keyval).first().map(|k| k.keycode()))
        .unwrap_or(0);
    {
        let raw: &mut gdk::ffi::GdkEventKey = key.as_mut();
        raw.keyval = keyval.into_glib();
        raw.state = state.bits();
        raw.window = window.to_glib_full();
        raw.send_event = 1;
        raw.hardware_keycode = keycode as u16;
        raw.time = gtk::current_event_time();
    }
    let mut event: gdk::Event = (*key).clone();
    if let Some(keyboard) = window
        .display()
        .default_seat()
        .and_then(|seat| seat.keyboard())
    {
        event.set_device(Some(&keyboard));
    }
    gtk::main_do_event(&mut event);
    pump(0);
}

pub fn key(widget: &impl IsA<gtk::Widget>, keyval: gdk::keys::Key) {
    send_key(widget, keyval, gdk::ModifierType::empty());
}

pub fn click(widget: &gtk::Widget) {
    widget
        .downcast_ref::<gtk::Button>()
        .expect("button control")
        .clicked();
}

pub fn obj(value: Value) -> Values {
    match value {
        Value::Object(map) => map,
        other => panic!("not an object: {other}"),
    }
}

pub type Changes = Rc<RefCell<Vec<(String, Value)>>>;

/// A dialog with a private, empty settings directory.
pub struct Fixture {
    pub dialog: SettingsDialog,
    pub root: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", root.path());
        impro_ui::motion::set_reduce_motion(true);
        impro_ui::apply_style(Some(impro_core::themes::get_theme(Some("impro-dark")))).unwrap();
        let dialog = SettingsDialog::new(None, None, None);
        Self { dialog, root }
    }

    pub fn show(&self) {
        self.dialog.show_all();
        self.dialog.present(None);
        pump(80);
    }

    pub fn changes(&self) -> Changes {
        let events: Changes = Rc::default();
        let sink = events.clone();
        self.dialog
            .connect_setting_changed(move |k, v| sink.borrow_mut().push((k.to_owned(), v.clone())));
        events
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        impro_settings_ui::flush();
        unsafe { self.dialog.destroy() };
        pump(10);
        impro_ui::motion::set_reduce_motion(false);
    }
}

pub fn last(changes: &Changes) -> (String, Value) {
    changes.borrow().last().cloned().expect("a change")
}

/// The id of the one page whose section is visible (`None` while several are, as in a search).
pub fn visible_page(dialog: &SettingsDialog) -> Option<String> {
    let ids: Vec<_> = impro_settings_ui::PAGES
        .iter()
        .filter(|p| dialog.section(p.0).unwrap().get_visible())
        .map(|p| p.0)
        .collect();
    (ids.len() == 1).then(|| ids[0].to_owned())
}
