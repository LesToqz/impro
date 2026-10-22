#![allow(dead_code)]
use gtk::prelude::*;
use impro::MainWindow;
use impro_core::settings;
use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{mpsc, OnceLock},
    time::{Duration, Instant},
};

pub fn run(f: impl FnOnce() + Send + 'static) {
    type Job = Box<dyn FnOnce() + Send>;
    static GTK: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let sender = GTK.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            gtk::init().expect("X11 display");
            while let Ok(job) = rx.recv() {
                job();
            }
        });
        tx
    });
    let (tx, rx) = mpsc::channel();
    sender
        .send(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            tx.send(result).unwrap();
        }))
        .unwrap();
    if let Err(error) = rx.recv().unwrap() {
        std::panic::resume_unwind(error);
    }
}

pub fn poll(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !predicate() {
        assert!(Instant::now() < deadline, "operation deadline exceeded");
        glib::MainContext::default().iteration(false);
        std::thread::yield_now();
    }
}
pub fn drain() {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(3);
    while context.pending() && Instant::now() < deadline {
        context.iteration(false);
    }
}
pub fn frame(window: &gtk::ApplicationWindow) {
    let drawn = Rc::new(Cell::new(false));
    let result = drawn.clone();
    let signal = window.connect_draw(move |_, _| {
        result.set(true);
        glib::Propagation::Proceed
    });
    window.queue_draw();
    poll(|| drawn.get());
    window.disconnect(signal);
    drain();
}
pub fn result(call: impl FnOnce(Box<dyn FnOnce(bool)>)) -> bool {
    let result = Rc::new(Cell::new(None));
    let value = result.clone();
    call(Box::new(move |ok| value.set(Some(ok))));
    poll(|| result.get().is_some());
    result.get().unwrap()
}
pub struct Harness {
    pub dir: tempfile::TempDir,
    pub app: gtk::Application,
    pub w: MainWindow,
}
impl Harness {
    pub fn new() -> Self {
        Self::with(settings::defaults().clone())
    }
    pub fn with(mut values: settings::Values) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("XDG_CONFIG_HOME", dir.path().join("config"));
        values.insert("reduce_motion".into(), true.into());
        thread_local! { static APP: std::cell::OnceCell<gtk::Application> = const { std::cell::OnceCell::new() }; }
        let app = APP.with(|slot| {
            slot.get_or_init(|| {
                let app = gtk::Application::new(
                    Some("dev.impro.WindowTests"),
                    gio::ApplicationFlags::NON_UNIQUE,
                );
                app.register(None::<&gio::Cancellable>).unwrap();
                app
            })
            .clone()
        });
        let w = MainWindow::with_values(&app, dir.path(), values);
        w.editor
            .set_dialog_hook(Some(Rc::new(|_, _| impro_editor::Response::Cancel)));
        w.show();
        poll(|| w.layout_ready());
        frame(&w.window);
        Self { dir, app, w }
    }
    pub fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
    pub fn open(&self, name: &str, text: &str) -> Rc<impro_editor::EditorTab> {
        let path = self.write(name, text);
        self.w.editor.open_file(&path);
        poll(|| !self.w.editor.is_busy());
        self.w.editor.tab_for_path(&path).unwrap()
    }
    pub fn shell(&self, right: bool) -> Rc<impro_terminal::Pane> {
        let panel = if right {
            &self.w.terminal_right
        } else {
            &self.w.terminal_bottom
        };
        if !panel.is_visible() {
            self.w.show_dock(panel);
        }
        poll(|| panel.panes().first().is_some_and(|p| p.pid().is_some()));
        panel.panes()[0].clone()
    }
    pub fn command(&self, label: &str) {
        (self
            .w
            .commands()
            .into_iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("missing command {label}"))
            .callback)();
    }
    pub fn accelerate(&self, accelerator: &str) {
        let (key, modifiers) = gtk::accelerator_parse(accelerator);
        assert!(
            gtk::accel_groups_activate(&self.w.window, key, modifiers),
            "{accelerator}"
        );
        drain();
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        unsafe {
            self.w.window.destroy();
        }
        drain();
    }
}
pub fn event(window: &gtk::ApplicationWindow, accelerator: &str) -> gdk::EventKey {
    use glib::translate::*;
    let (key, modifiers) = gtk::accelerator_parse(accelerator);
    let mapping = gdk::Keymap::for_display(&window.display())
        .and_then(|map| map.entries_for_keyval(key).into_iter().next());
    let mut event = gdk::Event::new(gdk::EventType::KeyPress);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).key.keyval = key;
        (*raw).key.state = modifiers.into_glib();
        (*raw).key.window = window.window().unwrap().to_glib_full();
        (*raw).key.send_event = 1;
        if let Some(mapping) = mapping {
            (*raw).key.hardware_keycode = mapping.keycode() as u16;
            (*raw).key.group = mapping.group() as u8;
        }
    }
    event.set_device(window.display().default_seat().unwrap().keyboard().as_ref());
    event.downcast().unwrap()
}
pub fn key(w: &MainWindow, accelerator: &str) -> bool {
    w.handle_key(&event(&w.window, accelerator))
}
pub fn wait_path(w: &MainWindow, path: &Path) {
    poll(|| w.editor.current_path().as_deref() == Some(path));
}

pub fn terminal_text(pane: &impro_terminal::Pane) -> String {
    pane.terminal
        .as_ref()
        .unwrap()
        .text_format(impro_vte::Format::Text)
        .unwrap_or_default()
        .to_string()
}
pub fn stable_prompt(pane: &impro_terminal::Pane) {
    pane.terminal
        .as_ref()
        .unwrap()
        .feed_child(b"stty -echo; PS1='impro-check> '; printf '\\033[2J\\033[Hhistory-once\\n'\n");
    poll(|| {
        terminal_text(pane)
            .lines()
            .any(|line| line == "history-once")
    });
}

pub fn terminal_history(pane: &impro_terminal::Pane) -> String {
    let terminal = pane.terminal.as_ref().unwrap();
    terminal
        .text_range_format(
            impro_vte::Format::Text,
            0,
            0,
            terminal.cursor_position().1 + terminal.row_count(),
            terminal.column_count(),
        )
        .unwrap_or_default()
        .to_string()
}
